//! Closed, exact citation value grammars. Tables and syntax are part of the
//! claim-reasoning rules; no locale, tolerance, or inferred unit is consulted.

use crate::{Quantity, text};

/// A contract's built-in object type.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ValueType {
    Number,
    Money,
    Date,
}

impl ValueType {
    /// Reads a built-in endpoint name; other names remain entity endpoints.
    #[must_use]
    pub fn parse(name: &str) -> Option<Self> {
        match name {
            "number" => Some(Self::Number),
            "money" => Some(Self::Money),
            "date" => Some(Self::Date),
            _ => None,
        }
    }

    /// The contract spelling.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Number => "number",
            Self::Money => "money",
            Self::Date => "date",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
enum Value {
    Number(Quantity),
    Money { currency: String, amount: Quantity },
    Date { year: u16, month: u8, day: u8 },
}

impl Value {
    fn describe(&self) -> String {
        match self {
            Self::Number(amount) => amount.to_string(),
            Self::Money { currency, amount } => format!("{currency} {amount}"),
            Self::Date { year, month, day } => format!("{year:04}-{month:02}-{day:02}"),
        }
    }
}

/// A target is read once, then compared to complete, already-folded lexemes.
pub(crate) struct Matcher<'a> {
    kind: ValueType,
    wanted: String,
    values: Vec<Value>,
    ambiguous: bool,
    currency: Option<&'a str>,
    order: Option<&'a str>,
}

impl<'a> Matcher<'a> {
    pub(crate) fn new(
        kind: ValueType,
        wanted: &str,
        currency: Option<&'a str>,
        order: Option<&'a str>,
    ) -> Option<Self> {
        let wanted = text::fold(wanted);
        let (values, ambiguous) = parse(kind, &wanted, currency, order)?;
        Some(Self {
            kind,
            wanted,
            values,
            ambiguous,
            currency,
            order,
        })
    }

    pub(crate) fn compare_folded(&self, actual: &str) -> Option<Vec<String>> {
        let actual = actual.trim();
        let (values, ambiguous) = parse(self.kind, actual, self.currency, self.order)?;
        if !values.iter().any(|a| self.values.iter().any(|b| a == b)) {
            return None;
        }
        let mut residuals = Vec::new();
        for (input, readings, ambiguous) in [
            (actual, values.as_slice(), ambiguous),
            (self.wanted.as_str(), self.values.as_slice(), self.ambiguous),
        ] {
            if ambiguous {
                let readings = readings
                    .iter()
                    .map(Value::describe)
                    .collect::<Vec<_>>()
                    .join(" or ");
                let policy = if self.kind == ValueType::Money {
                    "default_currency"
                } else {
                    "date_order"
                };
                let rationale = format!(
                    "{input:?} requires statement_policy {policy}; possible readings: {readings}"
                );
                if !residuals.contains(&rationale) {
                    residuals.push(rationale);
                }
            }
        }
        Some(residuals)
    }
}

fn parse(
    kind: ValueType,
    input: &str,
    currency: Option<&str>,
    order: Option<&str>,
) -> Option<(Vec<Value>, bool)> {
    if input.len() > 128 {
        return None;
    }
    match kind {
        ValueType::Date => date(input, order),
        ValueType::Number => number(input).map(|amount| (vec![Value::Number(amount)], false)),
        ValueType::Money => money(input, currency),
    }
}

fn number(input: &str) -> Option<Quantity> {
    let (digits, multiplier) = match input.rsplit_once(' ') {
        Some((digits, scale)) => (
            digits,
            match scale {
                "thousand" => 1_000,
                "million" => 1_000_000,
                "billion" => 1_000_000_000,
                "trillion" => 1_000_000_000_000,
                _ => return None,
            },
        ),
        None => (input, 1),
    };
    let (sign, digits) = if let Some(rest) = digits.strip_prefix('-') {
        ("-", rest)
    } else {
        ("", digits.strip_prefix('+').unwrap_or(digits))
    };
    let integral = digits.split('.').next()?;
    if integral.contains(',') {
        let groups: Vec<&str> = integral.split(',').collect();
        if groups[0].is_empty()
            || groups[0].len() > 3
            || groups[0].starts_with('0')
            || !groups
                .iter()
                .all(|group| group.bytes().all(|c| c.is_ascii_digit()))
            || !groups[1..].iter().all(|group| group.len() == 3)
        {
            return None;
        }
    }
    if digits
        .split_once('.')
        .is_some_and(|(_, fractional)| fractional.contains(','))
    {
        return None;
    }
    let canonical = format!("{sign}{}", digits.replace(',', ""));
    Quantity::parse(&canonical)
        .ok()?
        .checked_mul(Quantity::integer(multiplier))
}

fn money(input: &str, default: Option<&str>) -> Option<(Vec<Value>, bool)> {
    let symbols: &[(&str, &[&str])] = &[
        (
            "$",
            &[
                "ARS", "AUD", "BBD", "BMD", "BND", "BSD", "BZD", "CAD", "CLP", "COP", "FJD", "GYD",
                "HKD", "JMD", "KYD", "LRD", "MXN", "NAD", "NZD", "SBD", "SGD", "SRD", "TTD", "TWD",
                "USD", "UYU", "XCD",
            ],
        ),
        ("€", &["EUR"]),
        ("£", &["FKP", "GBP", "GIP", "SHP"]),
        ("¥", &["CNY", "JPY"]),
        ("₹", &["INR"]),
    ];
    for &(symbol, currencies) in symbols {
        let amount = input
            .strip_prefix(symbol)
            .or_else(|| input.strip_suffix(symbol));
        if let Some(amount) = amount {
            let amount = number(amount.trim())?;
            let selected: Vec<&str> = match default {
                Some(currency) if currencies.contains(&currency) => vec![currency],
                _ => currencies.to_vec(),
            };
            let ambiguous = selected.len() > 1;
            return Some((
                selected
                    .into_iter()
                    .map(|currency| Value::Money {
                        currency: currency.into(),
                        amount,
                    })
                    .collect(),
                ambiguous,
            ));
        }
    }
    let (first, rest) = input.split_once(' ')?;
    let code = first.to_ascii_uppercase();
    if capsulet_core::is_currency_code(&code) {
        return Some((
            vec![Value::Money {
                currency: code,
                amount: number(rest)?,
            }],
            false,
        ));
    }
    let (amount, last) = input.rsplit_once(' ')?;
    let code = last.to_ascii_uppercase();
    capsulet_core::is_currency_code(&code).then(|| {
        number(amount).map(|amount| {
            (
                vec![Value::Money {
                    currency: code,
                    amount,
                }],
                false,
            )
        })
    })?
}

fn date(input: &str, order: Option<&str>) -> Option<(Vec<Value>, bool)> {
    if input.contains('/') {
        let parts: Vec<&str> = input.split('/').collect();
        if parts.len() != 3
            || parts[2].len() != 4
            || parts[..2].iter().any(|p| p.is_empty() || p.len() > 2)
            || !parts.iter().all(|p| p.bytes().all(|c| c.is_ascii_digit()))
        {
            return None;
        }
        let first: u8 = parts[0].parse().ok()?;
        let second: u8 = parts[1].parse().ok()?;
        let year = parts[2].parse().ok()?;
        let day_first = calendar(year, second, first);
        let month_first = calendar(year, first, second);
        return match order {
            Some("day_first") => day_first.map(|date| (vec![date], false)),
            Some("month_first") => month_first.map(|date| (vec![date], false)),
            _ => {
                let mut readings: Vec<Value> = day_first.into_iter().collect();
                if let Some(date) = month_first
                    && !readings.contains(&date)
                {
                    readings.push(date);
                }
                (!readings.is_empty()).then_some((readings, true))
            }
        };
    }
    named_or_iso_date(input).map(|value| (vec![value], false))
}

fn named_or_iso_date(input: &str) -> Option<Value> {
    let parts: Vec<&str> = input.split('-').collect();
    if parts.len() == 3
        && parts[0].len() == 4
        && parts[1].len() == 2
        && parts[2].len() == 2
        && parts
            .iter()
            .all(|part| part.bytes().all(|c| c.is_ascii_digit()))
    {
        return calendar(
            parts[0].parse().ok()?,
            parts[1].parse().ok()?,
            parts[2].parse().ok()?,
        );
    }
    let months = [
        "january",
        "february",
        "march",
        "april",
        "may",
        "june",
        "july",
        "august",
        "september",
        "october",
        "november",
        "december",
    ];
    let parts: Vec<&str> = input.split_whitespace().collect();
    if parts.len() != 3 {
        return None;
    }
    if let Some(month) = months.iter().position(|&month| month == parts[0]) {
        if parts[2].len() != 4 {
            return None;
        }
        return calendar(
            digits(parts[2])?,
            u8::try_from(month + 1).ok()?,
            digits(parts[1].strip_suffix(',').unwrap_or(parts[1]))?,
        );
    }
    let month = months.iter().position(|&month| month == parts[1])?;
    if parts[2].len() != 4 {
        return None;
    }
    calendar(
        digits(parts[2])?,
        u8::try_from(month + 1).ok()?,
        digits(parts[0])?,
    )
}

fn digits<T: std::str::FromStr>(input: &str) -> Option<T> {
    if input.is_empty() || !input.bytes().all(|c| c.is_ascii_digit()) {
        return None;
    }
    input.parse().ok()
}

fn calendar(year: u16, month: u8, day: u8) -> Option<Value> {
    let leap = year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap => 29,
        2 => 28,
        _ => return None,
    };
    (year > 0 && year <= 9999 && day > 0 && day <= days).then_some(Value::Date { year, month, day })
}

/// A candidate must not be only a prefix of a larger value lexeme.
pub(crate) fn closed_region(text: &str, start: usize, end: usize) -> bool {
    let before = &text[..start];
    let after = &text[end..];
    if before
        .trim_end()
        .ends_with(['$', '€', '£', '¥', '₹', '+', '-'])
    {
        return false;
    }
    if before
        .split_whitespace()
        .next_back()
        .is_some_and(|word| capsulet_core::is_currency_code(&word.to_ascii_uppercase()))
    {
        return false;
    }
    if after.starts_with(['/', '-']) {
        return false;
    }
    if after.starts_with(['.', ',']) && after.chars().nth(1).is_some_and(|c| c.is_ascii_digit()) {
        return false;
    }
    let next = after
        .split_whitespace()
        .next()
        .unwrap_or("")
        .trim_end_matches(['.', ',', ';']);
    !matches!(next, "thousand" | "million" | "billion" | "trillion")
        && !capsulet_core::is_currency_code(&next.to_ascii_uppercase())
        && !after.trim_start().starts_with(['$', '€', '£', '¥', '₹'])
}

/// Finds an equal complete lexeme at word boundaries, preferring an exact
/// reading over an ambiguous occurrence elsewhere in the span.
pub(crate) fn find(
    kind: ValueType,
    excerpt: &str,
    wanted: &str,
    currency: Option<&str>,
    order: Option<&str>,
) -> Option<Vec<String>> {
    let matcher = Matcher::new(kind, wanted, currency, order)?;
    let folded = text::fold(excerpt);
    let boundaries = text::word_boundaries(&folded);
    let mut ambiguous = None;
    for &start in &boundaries {
        if folded[start..].starts_with(char::is_whitespace) {
            continue;
        }
        for &end in boundaries[boundaries.partition_point(|&at| at <= start)..]
            .iter()
            .take_while(|&&end| end - start <= 128)
        {
            if folded[..end].ends_with(char::is_whitespace) || !closed_region(&folded, start, end) {
                continue;
            }
            if let Some(found) = matcher.compare_folded(&folded[start..end]) {
                if found.is_empty() {
                    return Some(found);
                }
                ambiguous.get_or_insert(found);
            }
        }
    }
    ambiguous
}
