//! Exact fixed-point numbers, for arithmetic a certificate can stand behind.
//!
//! The kernel used to compute in `f64` and compare against a claimed value with
//! an absolute tolerance of `1e-9`. Three things were wrong with that, and the
//! third is the one that matters.
//!
//! Order changed the answer: floating-point addition is not associative, so the
//! same operands summed in a different order could land either side of the
//! tolerance. The tolerance was absolute, so it meant something at `0.5` and
//! nothing at all near `1e18`, where the gap between adjacent representable
//! values is already larger than `1e-9` — every pair of neighbours compared
//! equal. And a decision that depends on how a value was rounded is not
//! reproducible, which is the property the whole system is built on: two
//! machines replaying the same certificate must reach the same verdict, and
//! `capsulet-ir` refuses floating point outright for exactly this reason.
//!
//! So arithmetic here is integer arithmetic with a recorded scale. Nothing
//! rounds, and equality is equality.
//!
//! The wire form is a decimal string, matching [`capsulet_ir::Decimal`] — a JSON
//! number would invite a float back in somewhere between here and the digest.

use std::cmp::Ordering;
use std::fmt;

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};
use thiserror::Error;

/// The most decimal places a quantity may carry.
///
/// An `i128` holds a little over 38 decimal digits. Multiplication adds scales,
/// so two operands at the maximum produce 36 places and still leave room for the
/// integral part. Beyond that the type would silently stop being exact, which is
/// the failure it exists to prevent.
pub const MAX_SCALE: u8 = 18;

/// Why a quantity could not be read or computed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum QuantityError {
    #[error("`{text}` is not a fixed-point decimal")]
    Malformed { text: String },
    #[error("`{text}` carries more than {MAX_SCALE} decimal places")]
    ScaleTooLarge { text: String },
    #[error("the result does not fit in an exact fixed-point number")]
    OutOfRange,
}

/// An exact number: `units`, scaled by ten to the negative `scale`.
///
/// `1.250` is `units: 1250, scale: 3`. Trailing zeros are kept, because a scale
/// is a statement about precision and dropping it would change what the proposer
/// said. They do not affect equality: [`Quantity::cmp`] compares values, so
/// `1.5` and `1.50` are equal.
#[derive(Debug, Clone, Copy)]
pub struct Quantity {
    units: i128,
    scale: u8,
}

impl Quantity {
    /// A whole number.
    #[must_use]
    pub const fn integer(units: i128) -> Self {
        Self { units, scale: 0 }
    }

    /// Builds a quantity from its parts.
    ///
    /// # Errors
    ///
    /// Returns [`QuantityError::ScaleTooLarge`] beyond [`MAX_SCALE`].
    pub fn new(units: i128, scale: u8) -> Result<Self, QuantityError> {
        if scale > MAX_SCALE {
            return Err(QuantityError::ScaleTooLarge {
                text: format!("{units}e-{scale}"),
            });
        }
        Ok(Self { units, scale })
    }

    /// Reads a fixed-point decimal such as `-12`, `0.5`, or `1.250`.
    ///
    /// # Errors
    ///
    /// Returns [`QuantityError::Malformed`] when the text is not a fixed-point
    /// decimal, and [`QuantityError::ScaleTooLarge`] when it carries more places
    /// than an exact result can hold.
    pub fn parse(text: &str) -> Result<Self, QuantityError> {
        let malformed = || QuantityError::Malformed {
            text: text.to_string(),
        };

        let negative = text.starts_with('-');
        let digits = text.strip_prefix('-').unwrap_or(text);
        let (integral, fractional) = match digits.split_once('.') {
            Some((integral, fractional)) => (integral, fractional),
            None => (digits, ""),
        };

        if integral.is_empty() || !integral.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(malformed());
        }
        // No leading zeros, so one value has one spelling.
        if integral.len() > 1 && integral.starts_with('0') {
            return Err(malformed());
        }
        if digits.contains('.')
            && (fractional.is_empty() || !fractional.bytes().all(|b| b.is_ascii_digit()))
        {
            return Err(malformed());
        }

        let scale = u8::try_from(fractional.len()).map_err(|_| QuantityError::ScaleTooLarge {
            text: text.to_string(),
        })?;
        if scale > MAX_SCALE {
            return Err(QuantityError::ScaleTooLarge {
                text: text.to_string(),
            });
        }

        let mut units: i128 = format!("{integral}{fractional}")
            .parse()
            .map_err(|_| QuantityError::OutOfRange)?;
        if negative {
            units = -units;
        }
        Ok(Self { units, scale })
    }

    /// The integer count of units.
    #[must_use]
    pub const fn units(self) -> i128 {
        self.units
    }

    /// How many decimal places those units are scaled by.
    #[must_use]
    pub const fn scale(self) -> u8 {
        self.scale
    }

    /// This value with `scale` decimal places, when that is exact.
    fn rescaled(self, scale: u8) -> Option<Self> {
        if scale < self.scale || scale > MAX_SCALE {
            return None;
        }
        let factor = 10_i128.checked_pow(u32::from(scale - self.scale))?;
        Some(Self {
            units: self.units.checked_mul(factor)?,
            scale,
        })
    }

    /// Both values at a common scale, without rounding either.
    fn aligned(self, other: Self) -> Option<(i128, i128, u8)> {
        let scale = self.scale.max(other.scale);
        Some((
            self.rescaled(scale)?.units,
            other.rescaled(scale)?.units,
            scale,
        ))
    }

    /// Sum, exactly.
    #[must_use]
    pub fn checked_add(self, other: Self) -> Option<Self> {
        let (left, right, scale) = self.aligned(other)?;
        Some(Self {
            units: left.checked_add(right)?,
            scale,
        })
    }

    /// Difference, exactly.
    #[must_use]
    pub fn checked_sub(self, other: Self) -> Option<Self> {
        let (left, right, scale) = self.aligned(other)?;
        Some(Self {
            units: left.checked_sub(right)?,
            scale,
        })
    }

    /// Product, exactly. Scales add, because that is what multiplying does.
    #[must_use]
    pub fn checked_mul(self, other: Self) -> Option<Self> {
        let scale = self.scale.checked_add(other.scale)?;
        if scale > MAX_SCALE {
            return None;
        }
        Some(Self {
            units: self.units.checked_mul(other.units)?,
            scale,
        })
    }

    /// Compares values, ignoring how each was spelled.
    ///
    /// `None` when the two cannot be brought to a common scale without
    /// overflowing, which no comparison should silently guess at.
    #[must_use]
    pub fn partial_cmp_exact(self, other: Self) -> Option<Ordering> {
        let (left, right, _) = self.aligned(other)?;
        Some(left.cmp(&right))
    }
}

impl PartialEq for Quantity {
    /// Value equality: `1.5` and `1.50` are the same number.
    fn eq(&self, other: &Self) -> bool {
        self.partial_cmp_exact(*other) == Some(Ordering::Equal)
    }
}

impl Eq for Quantity {}

impl fmt::Display for Quantity {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.scale == 0 {
            return write!(formatter, "{}", self.units);
        }
        let sign = if self.units < 0 { "-" } else { "" };
        let digits = self.units.unsigned_abs().to_string();
        let places = usize::from(self.scale);
        let padded = if digits.len() <= places {
            format!("{}{digits}", "0".repeat(places - digits.len() + 1))
        } else {
            digits
        };
        let split = padded.len() - places;
        write!(formatter, "{sign}{}.{}", &padded[..split], &padded[split..])
    }
}

impl Serialize for Quantity {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.to_string())
    }
}

impl<'de> Deserialize<'de> for Quantity {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(D::Error::custom)
    }
}
