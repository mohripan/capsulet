//! Reading JSON back into the canonical model.
//!
//! Two readers with different jobs live here. [`from_json_slice`] is lenient
//! about presentation — whitespace and key order are the author's business — but
//! strict about meaning: floats, duplicate keys, unnormalized text, and invalid
//! UTF-8 are refused before a digest could ever be taken. [`verify_canonical`]
//! additionally proves the bytes are already the canonical encoding, which is
//! what a stored definition or certificate must satisfy.
//!
//! The parser is written here rather than delegated so the failures stay typed.
//! A duplicate key that a general-purpose JSON parser silently collapses is
//! exactly the kind of ambiguity this crate exists to refuse.

use std::collections::BTreeMap;

use unicode_normalization::is_nfc;

use crate::canonical::{CanonicalError, CanonicalValue};

/// The deepest nesting the reader will follow.
///
/// Bounded so the reader is total on adversarial input: refusing a document is a
/// decision, overflowing the stack is not.
const MAX_DEPTH: usize = 128;

/// Parses JSON text into the canonical model.
///
/// Presentation is free: any key order and any whitespace are accepted, and two
/// such documents produce one canonical value and therefore one digest. Meaning
/// is not free: floating point, duplicate keys, unnormalized text, and trailing
/// content are refused.
///
/// # Errors
///
/// Returns [`CanonicalError`] describing the first problem found.
pub fn from_json_slice(bytes: &[u8]) -> Result<CanonicalValue, CanonicalError> {
    let text = std::str::from_utf8(bytes).map_err(|_| CanonicalError::NotUtf8)?;
    let mut reader = Reader {
        text,
        offset: 0,
        depth: 0,
    };
    reader.skip_whitespace();
    let value = reader.read_value()?;
    reader.skip_whitespace();
    if reader.offset != text.len() {
        return Err(reader.syntax("unexpected trailing content"));
    }
    Ok(value)
}

/// Parses JSON text and proves it is already in canonical form.
///
/// # Errors
///
/// Returns [`CanonicalError::NotCanonical`] when the bytes parse but are not the
/// canonical encoding of the value they denote, or any parse error from
/// [`from_json_slice`].
pub fn verify_canonical(bytes: &[u8]) -> Result<CanonicalValue, CanonicalError> {
    let value = from_json_slice(bytes)?;
    if value.to_canonical_bytes() == bytes {
        Ok(value)
    } else {
        Err(CanonicalError::NotCanonical)
    }
}

struct Reader<'a> {
    text: &'a str,
    offset: usize,
    depth: usize,
}

impl Reader<'_> {
    fn syntax(&self, reason: &str) -> CanonicalError {
        CanonicalError::Syntax {
            offset: self.offset,
            reason: reason.to_string(),
        }
    }

    fn peek(&self) -> Option<char> {
        self.text[self.offset..].chars().next()
    }

    fn bump(&mut self, character: char) {
        self.offset += character.len_utf8();
    }

    fn skip_whitespace(&mut self) {
        while let Some(character) = self.peek() {
            if matches!(character, ' ' | '\t' | '\n' | '\r') {
                self.bump(character);
            } else {
                break;
            }
        }
    }

    fn expect(&mut self, expected: char) -> Result<(), CanonicalError> {
        match self.peek() {
            Some(character) if character == expected => {
                self.bump(character);
                Ok(())
            }
            _ => Err(self.syntax(&format!("expected `{expected}`"))),
        }
    }

    fn read_value(&mut self) -> Result<CanonicalValue, CanonicalError> {
        if self.depth >= MAX_DEPTH {
            return Err(self.syntax("nesting is deeper than the reader accepts"));
        }
        match self.peek() {
            None => Err(self.syntax("unexpected end of input")),
            Some('n') => self.read_literal("null", CanonicalValue::Null),
            Some('t') => self.read_literal("true", CanonicalValue::Bool(true)),
            Some('f') => self.read_literal("false", CanonicalValue::Bool(false)),
            Some('"') => Ok(CanonicalValue::Text(self.read_string()?)),
            Some('[') => self.read_list(),
            Some('{') => self.read_map(),
            Some(character) if character == '-' || character.is_ascii_digit() => self.read_number(),
            Some(character) => Err(self.syntax(&format!("unexpected `{character}`"))),
        }
    }

    fn read_literal(
        &mut self,
        literal: &str,
        value: CanonicalValue,
    ) -> Result<CanonicalValue, CanonicalError> {
        if self.text[self.offset..].starts_with(literal) {
            self.offset += literal.len();
            Ok(value)
        } else {
            Err(self.syntax(&format!("expected `{literal}`")))
        }
    }

    fn read_number(&mut self) -> Result<CanonicalValue, CanonicalError> {
        let start = self.offset;
        if self.peek() == Some('-') {
            self.bump('-');
        }
        let digits_start = self.offset;
        while let Some(character) = self.peek() {
            if character.is_ascii_digit() {
                self.bump(character);
            } else {
                break;
            }
        }
        if self.offset == digits_start {
            return Err(self.syntax("expected a digit"));
        }
        let integral = &self.text[digits_start..self.offset];
        if integral.len() > 1 && integral.starts_with('0') {
            return Err(self.syntax("integers must not carry leading zeros"));
        }
        if matches!(self.peek(), Some('.' | 'e' | 'E')) {
            return Err(CanonicalError::Float);
        }
        self.text[start..self.offset]
            .parse::<i128>()
            .map(CanonicalValue::Integer)
            .map_err(|_| CanonicalError::IntegerRange {
                found: self.text[start..self.offset].to_string(),
            })
    }

    fn read_string(&mut self) -> Result<String, CanonicalError> {
        self.expect('"')?;
        let mut value = String::new();
        loop {
            let character = self
                .peek()
                .ok_or_else(|| self.syntax("unterminated text"))?;
            self.bump(character);
            match character {
                '"' => break,
                '\\' => {
                    let escaped = self
                        .peek()
                        .ok_or_else(|| self.syntax("unterminated escape"))?;
                    self.bump(escaped);
                    match escaped {
                        '"' => value.push('"'),
                        '\\' => value.push('\\'),
                        '/' => value.push('/'),
                        'b' => value.push('\u{8}'),
                        'f' => value.push('\u{c}'),
                        'n' => value.push('\n'),
                        'r' => value.push('\r'),
                        't' => value.push('\t'),
                        'u' => value.push(self.read_unicode_escape()?),
                        other => {
                            return Err(self.syntax(&format!("unknown escape `\\{other}`")));
                        }
                    }
                }
                control if control < '\u{20}' => {
                    return Err(self.syntax("control characters must be escaped"));
                }
                other => value.push(other),
            }
        }

        if is_nfc(&value) {
            Ok(value)
        } else {
            Err(CanonicalError::NotNormalized { text: value })
        }
    }

    fn read_unicode_escape(&mut self) -> Result<char, CanonicalError> {
        let first = self.read_code_unit()?;
        if (0xd800..0xdc00).contains(&first) {
            if !self.text[self.offset..].starts_with("\\u") {
                return Err(self.syntax("expected a low surrogate"));
            }
            self.offset += 2;
            let second = self.read_code_unit()?;
            if !(0xdc00..0xe000).contains(&second) {
                return Err(self.syntax("expected a low surrogate"));
            }
            let combined = 0x1_0000 + ((first - 0xd800) << 10) + (second - 0xdc00);
            return char::from_u32(combined).ok_or_else(|| self.syntax("invalid surrogate pair"));
        }
        char::from_u32(first).ok_or_else(|| self.syntax("invalid escape"))
    }

    fn read_code_unit(&mut self) -> Result<u32, CanonicalError> {
        let end = self.offset + 4;
        if end > self.text.len() || !self.text.is_char_boundary(end) {
            return Err(self.syntax("truncated escape"));
        }
        let digits = &self.text[self.offset..end];
        let value =
            u32::from_str_radix(digits, 16).map_err(|_| self.syntax("invalid escape digits"))?;
        self.offset = end;
        Ok(value)
    }

    fn read_list(&mut self) -> Result<CanonicalValue, CanonicalError> {
        self.expect('[')?;
        self.depth += 1;
        let mut values = Vec::new();
        self.skip_whitespace();
        if self.peek() == Some(']') {
            self.bump(']');
            self.depth -= 1;
            return Ok(CanonicalValue::List(values));
        }
        loop {
            self.skip_whitespace();
            values.push(self.read_value()?);
            self.skip_whitespace();
            match self.peek() {
                Some(',') => self.bump(','),
                Some(']') => {
                    self.bump(']');
                    break;
                }
                _ => return Err(self.syntax("expected `,` or `]`")),
            }
        }
        self.depth -= 1;
        Ok(CanonicalValue::List(values))
    }

    fn read_map(&mut self) -> Result<CanonicalValue, CanonicalError> {
        self.expect('{')?;
        self.depth += 1;
        let mut entries: BTreeMap<String, CanonicalValue> = BTreeMap::new();
        self.skip_whitespace();
        if self.peek() == Some('}') {
            self.bump('}');
            self.depth -= 1;
            return Ok(CanonicalValue::Map(entries));
        }
        loop {
            self.skip_whitespace();
            let key = self.read_string()?;
            if entries.contains_key(&key) {
                return Err(CanonicalError::DuplicateKey { key });
            }
            self.skip_whitespace();
            self.expect(':')?;
            self.skip_whitespace();
            let value = self.read_value()?;
            entries.insert(key, value);
            self.skip_whitespace();
            match self.peek() {
                Some(',') => self.bump(','),
                Some('}') => {
                    self.bump('}');
                    break;
                }
                _ => return Err(self.syntax("expected `,` or `}`")),
            }
        }
        self.depth -= 1;
        Ok(CanonicalValue::Map(entries))
    }
}
