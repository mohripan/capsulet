//! Canonical bytes: the exact encoding every digest in this milestone is taken
//! over.
//!
//! Two documents that mean the same thing must produce the same bytes, and two
//! documents that mean different things must not. That is the whole contract.
//! Everything else here follows from it:
//!
//! - object keys are sorted by Unicode scalar value, so authoring order cannot
//!   change a digest (the one deliberate deviation from RFC 8785, which sorts by
//!   UTF-16 code unit; scalar order is what `String` ordering already gives us
//!   and it is equally total);
//! - list order is preserved, because order is meaning in a list;
//! - there is no floating point anywhere in the model, so a value that cannot be
//!   reproduced bit-for-bit on another platform cannot enter a digest;
//! - text must be valid UTF-8 in Unicode NFC, so two spellings of one string
//!   cannot produce two digests.
//!
//! Changing the bytes this module produces changes every stored digest. It is a
//! breaking change and requires a schema major bump; see [`crate::version`].

use std::collections::BTreeMap;
use std::fmt;

use serde::{Serialize, Serializer, ser};
use thiserror::Error;
use unicode_normalization::is_nfc;

/// Why a value could not be encoded, parsed, or accepted as canonical.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CanonicalError {
    #[error("floating point has no canonical encoding; use an integer or a fixed-point decimal")]
    Float,
    #[error("integer {found} is outside the canonical signed 128-bit range")]
    IntegerRange { found: String },
    #[error("duplicate object key `{key}`")]
    DuplicateKey { key: String },
    #[error("object keys must be text, found {found}")]
    NonTextKey { found: &'static str },
    #[error("text is not Unicode NFC normalized: `{text}`")]
    NotNormalized { text: String },
    #[error("raw bytes have no canonical encoding; reference them by digest instead")]
    RawBytes,
    #[error("input is not valid UTF-8")]
    NotUtf8,
    #[error("invalid JSON at byte {offset}: {reason}")]
    Syntax { offset: usize, reason: String },
    #[error("input is not in canonical form")]
    NotCanonical,
    #[error("{message}")]
    Message { message: String },
}

impl ser::Error for CanonicalError {
    fn custom<T: fmt::Display>(message: T) -> Self {
        Self::Message {
            message: message.to_string(),
        }
    }
}

/// A fixed-point decimal, carried as text so it round-trips exactly.
///
/// Encoded as a JSON string, never as a JSON number: a number would invite a
/// float somewhere in the pipeline, and a float would make the digest depend on
/// whoever formatted it.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct Decimal(String);

impl Decimal {
    /// Parses a fixed-point decimal such as `-12`, `0.5`, or `1.250`.
    ///
    /// Trailing zeros are significant and preserved: `1.5` and `1.50` are
    /// different values here, because a scale is a statement about precision.
    ///
    /// # Errors
    ///
    /// Returns [`CanonicalError::Message`] when the text is not a finite
    /// fixed-point decimal.
    pub fn parse(text: &str) -> Result<Self, CanonicalError> {
        let invalid = || CanonicalError::Message {
            message: format!("`{text}` is not a fixed-point decimal"),
        };
        let digits = text.strip_prefix('-').unwrap_or(text);
        let (integral, fractional) = match digits.split_once('.') {
            Some((integral, fractional)) => (integral, Some(fractional)),
            None => (digits, None),
        };
        if integral.is_empty() || !integral.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(invalid());
        }
        if integral.len() > 1 && integral.starts_with('0') {
            return Err(invalid());
        }
        if let Some(fractional) = fractional
            && (fractional.is_empty() || !fractional.bytes().all(|byte| byte.is_ascii_digit()))
        {
            return Err(invalid());
        }
        Ok(Self(text.to_string()))
    }

    /// The exact text this decimal was parsed from.
    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for Decimal {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.0)
    }
}

impl Serialize for Decimal {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(&self.0)
    }
}

impl<'de> serde::Deserialize<'de> for Decimal {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let text = String::deserialize(deserializer)?;
        Self::parse(&text).map_err(serde::de::Error::custom)
    }
}

/// The canonical data model.
///
/// Deliberately narrower than JSON: no floats, and text is validated on the way
/// in. A value that exists here can always be encoded, and encoding it always
/// produces the same bytes.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord)]
pub enum CanonicalValue {
    Null,
    Bool(bool),
    Integer(i128),
    Text(String),
    List(Vec<CanonicalValue>),
    Map(BTreeMap<String, CanonicalValue>),
}

impl CanonicalValue {
    /// Builds validated canonical text.
    ///
    /// # Errors
    ///
    /// Returns [`CanonicalError::NotNormalized`] when the text is not NFC.
    pub fn text(value: impl Into<String>) -> Result<Self, CanonicalError> {
        let value = value.into();
        if is_nfc(&value) {
            Ok(Self::Text(value))
        } else {
            Err(CanonicalError::NotNormalized { text: value })
        }
    }

    /// The canonical bytes of this value.
    #[must_use]
    pub fn to_canonical_bytes(&self) -> Vec<u8> {
        let mut out = String::new();
        self.write(&mut out);
        out.into_bytes()
    }

    fn write(&self, out: &mut String) {
        match self {
            Self::Null => out.push_str("null"),
            Self::Bool(true) => out.push_str("true"),
            Self::Bool(false) => out.push_str("false"),
            Self::Integer(value) => out.push_str(&value.to_string()),
            Self::Text(value) => write_text(value, out),
            Self::List(values) => {
                out.push('[');
                for (index, value) in values.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    value.write(out);
                }
                out.push(']');
            }
            Self::Map(entries) => {
                out.push('{');
                for (index, (key, value)) in entries.iter().enumerate() {
                    if index > 0 {
                        out.push(',');
                    }
                    write_text(key, out);
                    out.push(':');
                    value.write(out);
                }
                out.push('}');
            }
        }
    }
}

fn write_text(value: &str, out: &mut String) {
    out.push('"');
    for character in value.chars() {
        match character {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{8}' => out.push_str("\\b"),
            '\u{c}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            control if control < '\u{20}' => {
                out.push_str("\\u");
                let code = control as u32;
                for shift in [12_u32, 8, 4, 0] {
                    let nibble = (code >> shift) & 0xf;
                    out.push(char::from_digit(nibble, 16).unwrap_or('0'));
                }
            }
            other => out.push(other),
        }
    }
    out.push('"');
}

/// Encodes any serializable value into canonical bytes.
///
/// # Errors
///
/// Returns [`CanonicalError`] when the value contains floating point, raw bytes,
/// non-text map keys, duplicate keys, an out-of-range integer, or text that is
/// not NFC normalized.
pub fn to_canonical_bytes<T: Serialize>(value: &T) -> Result<Vec<u8>, CanonicalError> {
    Ok(to_canonical_value(value)?.to_canonical_bytes())
}

/// Converts any serializable value into the canonical model.
///
/// # Errors
///
/// Returns [`CanonicalError`] for the same reasons as [`to_canonical_bytes`].
pub fn to_canonical_value<T: Serialize>(value: &T) -> Result<CanonicalValue, CanonicalError> {
    value.serialize(CanonicalSerializer)
}

struct CanonicalSerializer;

impl Serializer for CanonicalSerializer {
    type Ok = CanonicalValue;
    type Error = CanonicalError;
    type SerializeSeq = SeqSerializer;
    type SerializeTuple = SeqSerializer;
    type SerializeTupleStruct = SeqSerializer;
    type SerializeTupleVariant = TupleVariantSerializer;
    type SerializeMap = MapSerializer;
    type SerializeStruct = MapSerializer;
    type SerializeStructVariant = StructVariantSerializer;

    fn serialize_bool(self, value: bool) -> Result<Self::Ok, Self::Error> {
        Ok(CanonicalValue::Bool(value))
    }

    fn serialize_i8(self, value: i8) -> Result<Self::Ok, Self::Error> {
        Ok(CanonicalValue::Integer(i128::from(value)))
    }

    fn serialize_i16(self, value: i16) -> Result<Self::Ok, Self::Error> {
        Ok(CanonicalValue::Integer(i128::from(value)))
    }

    fn serialize_i32(self, value: i32) -> Result<Self::Ok, Self::Error> {
        Ok(CanonicalValue::Integer(i128::from(value)))
    }

    fn serialize_i64(self, value: i64) -> Result<Self::Ok, Self::Error> {
        Ok(CanonicalValue::Integer(i128::from(value)))
    }

    fn serialize_i128(self, value: i128) -> Result<Self::Ok, Self::Error> {
        Ok(CanonicalValue::Integer(value))
    }

    fn serialize_u8(self, value: u8) -> Result<Self::Ok, Self::Error> {
        Ok(CanonicalValue::Integer(i128::from(value)))
    }

    fn serialize_u16(self, value: u16) -> Result<Self::Ok, Self::Error> {
        Ok(CanonicalValue::Integer(i128::from(value)))
    }

    fn serialize_u32(self, value: u32) -> Result<Self::Ok, Self::Error> {
        Ok(CanonicalValue::Integer(i128::from(value)))
    }

    fn serialize_u64(self, value: u64) -> Result<Self::Ok, Self::Error> {
        Ok(CanonicalValue::Integer(i128::from(value)))
    }

    fn serialize_u128(self, value: u128) -> Result<Self::Ok, Self::Error> {
        i128::try_from(value)
            .map(CanonicalValue::Integer)
            .map_err(|_| CanonicalError::IntegerRange {
                found: value.to_string(),
            })
    }

    fn serialize_f32(self, _value: f32) -> Result<Self::Ok, Self::Error> {
        Err(CanonicalError::Float)
    }

    fn serialize_f64(self, _value: f64) -> Result<Self::Ok, Self::Error> {
        Err(CanonicalError::Float)
    }

    fn serialize_char(self, value: char) -> Result<Self::Ok, Self::Error> {
        CanonicalValue::text(value.to_string())
    }

    fn serialize_str(self, value: &str) -> Result<Self::Ok, Self::Error> {
        CanonicalValue::text(value)
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<Self::Ok, Self::Error> {
        Err(CanonicalError::RawBytes)
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Ok(CanonicalValue::Null)
    }

    fn serialize_some<T: ?Sized + Serialize>(self, value: &T) -> Result<Self::Ok, Self::Error> {
        value.serialize(self)
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Ok(CanonicalValue::Null)
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok, Self::Error> {
        Ok(CanonicalValue::Null)
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        CanonicalValue::text(variant)
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        value.serialize(self)
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        let mut entries = BTreeMap::new();
        entries.insert(variant.to_string(), value.serialize(CanonicalSerializer)?);
        Ok(CanonicalValue::Map(entries))
    }

    fn serialize_seq(self, _length: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Ok(SeqSerializer { values: Vec::new() })
    }

    fn serialize_tuple(self, length: usize) -> Result<Self::SerializeTuple, Self::Error> {
        self.serialize_seq(Some(length))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        length: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        self.serialize_seq(Some(length))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Ok(TupleVariantSerializer {
            variant,
            values: Vec::new(),
        })
    }

    fn serialize_map(self, _length: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Ok(MapSerializer {
            entries: BTreeMap::new(),
            pending_key: None,
        })
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        self.serialize_map(None)
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Ok(StructVariantSerializer {
            variant,
            entries: BTreeMap::new(),
        })
    }
}

struct SeqSerializer {
    values: Vec<CanonicalValue>,
}

impl ser::SerializeSeq for SeqSerializer {
    type Ok = CanonicalValue;
    type Error = CanonicalError;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.values.push(value.serialize(CanonicalSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(CanonicalValue::List(self.values))
    }
}

impl ser::SerializeTuple for SeqSerializer {
    type Ok = CanonicalValue;
    type Error = CanonicalError;

    fn serialize_element<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        ser::SerializeSeq::end(self)
    }
}

impl ser::SerializeTupleStruct for SeqSerializer {
    type Ok = CanonicalValue;
    type Error = CanonicalError;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        ser::SerializeSeq::serialize_element(self, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        ser::SerializeSeq::end(self)
    }
}

struct TupleVariantSerializer {
    variant: &'static str,
    values: Vec<CanonicalValue>,
}

impl ser::SerializeTupleVariant for TupleVariantSerializer {
    type Ok = CanonicalValue;
    type Error = CanonicalError;

    fn serialize_field<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        self.values.push(value.serialize(CanonicalSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        let mut entries = BTreeMap::new();
        entries.insert(self.variant.to_string(), CanonicalValue::List(self.values));
        Ok(CanonicalValue::Map(entries))
    }
}

struct MapSerializer {
    entries: BTreeMap<String, CanonicalValue>,
    pending_key: Option<String>,
}

impl MapSerializer {
    fn insert(&mut self, key: String, value: CanonicalValue) -> Result<(), CanonicalError> {
        if self.entries.contains_key(&key) {
            return Err(CanonicalError::DuplicateKey { key });
        }
        self.entries.insert(key, value);
        Ok(())
    }
}

impl ser::SerializeMap for MapSerializer {
    type Ok = CanonicalValue;
    type Error = CanonicalError;

    fn serialize_key<T: ?Sized + Serialize>(&mut self, key: &T) -> Result<(), Self::Error> {
        self.pending_key = Some(key.serialize(KeySerializer)?);
        Ok(())
    }

    fn serialize_value<T: ?Sized + Serialize>(&mut self, value: &T) -> Result<(), Self::Error> {
        let key = self
            .pending_key
            .take()
            .ok_or_else(|| CanonicalError::Message {
                message: "map value serialized before its key".to_string(),
            })?;
        let value = value.serialize(CanonicalSerializer)?;
        self.insert(key, value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(CanonicalValue::Map(self.entries))
    }
}

impl ser::SerializeStruct for MapSerializer {
    type Ok = CanonicalValue;
    type Error = CanonicalError;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        let value = value.serialize(CanonicalSerializer)?;
        self.insert(key.to_string(), value)
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        Ok(CanonicalValue::Map(self.entries))
    }
}

struct StructVariantSerializer {
    variant: &'static str,
    entries: BTreeMap<String, CanonicalValue>,
}

impl ser::SerializeStructVariant for StructVariantSerializer {
    type Ok = CanonicalValue;
    type Error = CanonicalError;

    fn serialize_field<T: ?Sized + Serialize>(
        &mut self,
        key: &'static str,
        value: &T,
    ) -> Result<(), Self::Error> {
        let key = key.to_string();
        if self.entries.contains_key(&key) {
            return Err(CanonicalError::DuplicateKey { key });
        }
        self.entries
            .insert(key, value.serialize(CanonicalSerializer)?);
        Ok(())
    }

    fn end(self) -> Result<Self::Ok, Self::Error> {
        let mut outer = BTreeMap::new();
        outer.insert(self.variant.to_string(), CanonicalValue::Map(self.entries));
        Ok(CanonicalValue::Map(outer))
    }
}

/// Serializes map keys, which must be text.
struct KeySerializer;

impl KeySerializer {
    fn reject(found: &'static str) -> CanonicalError {
        CanonicalError::NonTextKey { found }
    }
}

impl Serializer for KeySerializer {
    type Ok = String;
    type Error = CanonicalError;
    type SerializeSeq = ser::Impossible<String, CanonicalError>;
    type SerializeTuple = ser::Impossible<String, CanonicalError>;
    type SerializeTupleStruct = ser::Impossible<String, CanonicalError>;
    type SerializeTupleVariant = ser::Impossible<String, CanonicalError>;
    type SerializeMap = ser::Impossible<String, CanonicalError>;
    type SerializeStruct = ser::Impossible<String, CanonicalError>;
    type SerializeStructVariant = ser::Impossible<String, CanonicalError>;

    fn serialize_str(self, value: &str) -> Result<Self::Ok, Self::Error> {
        if is_nfc(value) {
            Ok(value.to_string())
        } else {
            Err(CanonicalError::NotNormalized {
                text: value.to_string(),
            })
        }
    }

    fn serialize_char(self, value: char) -> Result<Self::Ok, Self::Error> {
        self.serialize_str(&value.to_string())
    }

    fn serialize_unit_variant(
        self,
        _name: &'static str,
        _index: u32,
        variant: &'static str,
    ) -> Result<Self::Ok, Self::Error> {
        self.serialize_str(variant)
    }

    fn serialize_newtype_struct<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        value.serialize(self)
    }

    fn serialize_bool(self, _value: bool) -> Result<Self::Ok, Self::Error> {
        Err(Self::reject("a boolean"))
    }

    fn serialize_i8(self, _value: i8) -> Result<Self::Ok, Self::Error> {
        Err(Self::reject("an integer"))
    }

    fn serialize_i16(self, _value: i16) -> Result<Self::Ok, Self::Error> {
        Err(Self::reject("an integer"))
    }

    fn serialize_i32(self, _value: i32) -> Result<Self::Ok, Self::Error> {
        Err(Self::reject("an integer"))
    }

    fn serialize_i64(self, _value: i64) -> Result<Self::Ok, Self::Error> {
        Err(Self::reject("an integer"))
    }

    fn serialize_u8(self, _value: u8) -> Result<Self::Ok, Self::Error> {
        Err(Self::reject("an integer"))
    }

    fn serialize_u16(self, _value: u16) -> Result<Self::Ok, Self::Error> {
        Err(Self::reject("an integer"))
    }

    fn serialize_u32(self, _value: u32) -> Result<Self::Ok, Self::Error> {
        Err(Self::reject("an integer"))
    }

    fn serialize_u64(self, _value: u64) -> Result<Self::Ok, Self::Error> {
        Err(Self::reject("an integer"))
    }

    fn serialize_f32(self, _value: f32) -> Result<Self::Ok, Self::Error> {
        Err(CanonicalError::Float)
    }

    fn serialize_f64(self, _value: f64) -> Result<Self::Ok, Self::Error> {
        Err(CanonicalError::Float)
    }

    fn serialize_bytes(self, _value: &[u8]) -> Result<Self::Ok, Self::Error> {
        Err(CanonicalError::RawBytes)
    }

    fn serialize_none(self) -> Result<Self::Ok, Self::Error> {
        Err(Self::reject("null"))
    }

    fn serialize_some<T: ?Sized + Serialize>(self, _value: &T) -> Result<Self::Ok, Self::Error> {
        Err(Self::reject("an optional value"))
    }

    fn serialize_unit(self) -> Result<Self::Ok, Self::Error> {
        Err(Self::reject("null"))
    }

    fn serialize_unit_struct(self, _name: &'static str) -> Result<Self::Ok, Self::Error> {
        Err(Self::reject("null"))
    }

    fn serialize_newtype_variant<T: ?Sized + Serialize>(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _value: &T,
    ) -> Result<Self::Ok, Self::Error> {
        Err(Self::reject("an enum variant with a payload"))
    }

    fn serialize_seq(self, _length: Option<usize>) -> Result<Self::SerializeSeq, Self::Error> {
        Err(Self::reject("a list"))
    }

    fn serialize_tuple(self, _length: usize) -> Result<Self::SerializeTuple, Self::Error> {
        Err(Self::reject("a tuple"))
    }

    fn serialize_tuple_struct(
        self,
        _name: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeTupleStruct, Self::Error> {
        Err(Self::reject("a tuple struct"))
    }

    fn serialize_tuple_variant(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeTupleVariant, Self::Error> {
        Err(Self::reject("a tuple variant"))
    }

    fn serialize_map(self, _length: Option<usize>) -> Result<Self::SerializeMap, Self::Error> {
        Err(Self::reject("a map"))
    }

    fn serialize_struct(
        self,
        _name: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeStruct, Self::Error> {
        Err(Self::reject("a struct"))
    }

    fn serialize_struct_variant(
        self,
        _name: &'static str,
        _index: u32,
        _variant: &'static str,
        _length: usize,
    ) -> Result<Self::SerializeStructVariant, Self::Error> {
        Err(Self::reject("a struct variant"))
    }
}
