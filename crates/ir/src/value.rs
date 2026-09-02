//! Structural value schemas.
//!
//! The runtime this crate replaces described a value with a nominal tag from a
//! closed list — `RetrievedDocuments`, `Prompt`, `Json` — which says what
//! someone called a value, not what is in it. Two ports agreeing on a tag proves
//! nothing, and every tag mismatch has the same explanation: "types differ".
//!
//! Schemas here are structural. Compatibility is decided by shape, and a
//! rejection names the field and the rule that failed, because the caller has to
//! fix something specific.
//!
//! [`ValueSchema::Json`] remains, because real systems carry values nobody has
//! modelled yet. It is not a hole in the type system: a value that passes
//! through `Json` loses its structure, that loss is recorded, and
//! [`crate::trust`] refuses to let a lost-provenance value silently satisfy a
//! contract that needs structure.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Why one schema does not satisfy another.
///
/// Carries the path, so a mismatch three records deep says where it is instead
/// of making the reader diff two schema dumps.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum SchemaMismatch {
    #[error("at `{path}`: expected {expected}, found {found}")]
    Kind {
        path: String,
        expected: String,
        found: String,
    },
    #[error("at `{path}`: required field `{field}` is missing")]
    MissingField { path: String, field: String },
    #[error("at `{path}`: variant `{variant}` is not accepted by the requirement")]
    UnexpectedVariant { path: String, variant: String },
    #[error("at `{path}`: requirement has no variant for the discriminant `{discriminant}`")]
    MissingDiscriminant { path: String, discriminant: String },
    #[error("at `{path}`: value `{value}` is not one of the accepted enumeration members")]
    NotAMember { path: String, value: String },
    #[error("at `{path}`: bound {found} is outside the required range {minimum}..={maximum}")]
    OutOfRange {
        path: String,
        found: String,
        minimum: String,
        maximum: String,
    },
    #[error(
        "at `{path}`: an opaque value cannot satisfy a structural requirement; \
         declare the degradation if this is intended"
    )]
    OpaqueNotAccepted { path: String },
    #[error("at `{path}`: decimal scale {found} does not match the required scale {required}")]
    Scale {
        path: String,
        found: u8,
        required: u8,
    },
}

/// An inclusive integer range.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct IntegerRange {
    pub minimum: i128,
    pub maximum: i128,
}

impl IntegerRange {
    /// Builds a range.
    #[must_use]
    pub const fn new(minimum: i128, maximum: i128) -> Self {
        Self { minimum, maximum }
    }

    /// Whether this range fits entirely inside `required`.
    #[must_use]
    pub const fn is_within(&self, required: &Self) -> bool {
        self.minimum >= required.minimum && self.maximum <= required.maximum
    }
}

/// Bounds on the length of a list or a string.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LengthBounds {
    pub minimum: u32,
    pub maximum: u32,
}

impl LengthBounds {
    /// Builds length bounds.
    #[must_use]
    pub const fn new(minimum: u32, maximum: u32) -> Self {
        Self { minimum, maximum }
    }

    /// Whether these bounds fit entirely inside `required`.
    #[must_use]
    pub const fn is_within(&self, required: &Self) -> bool {
        self.minimum >= required.minimum && self.maximum <= required.maximum
    }
}

/// One field of a record schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Field {
    pub schema: ValueSchema,
    pub required: bool,
}

impl Field {
    /// A field that must be present.
    #[must_use]
    pub const fn required(schema: ValueSchema) -> Self {
        Self {
            schema,
            required: true,
        }
    }

    /// A field that may be absent.
    #[must_use]
    pub const fn optional(schema: ValueSchema) -> Self {
        Self {
            schema,
            required: false,
        }
    }
}

/// The structure of a value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum ValueSchema {
    /// No information. The result of a node that exists for its effect.
    Unit,
    Bool,
    /// An integer constrained to an explicit range, because an unbounded
    /// integer is a budget nobody agreed to.
    Integer {
        range: IntegerRange,
    },
    /// A fixed-point decimal at an exact scale. Never a float.
    Decimal {
        scale: u8,
    },
    Text {
        length: LengthBounds,
    },
    /// Bytes referenced by digest. The bytes themselves live in evidence or
    /// object storage; a schema never inlines them.
    BytesRef,
    /// A closed set of named members.
    Enumeration {
        members: Vec<String>,
    },
    List {
        item: Box<ValueSchema>,
        length: LengthBounds,
    },
    Record {
        fields: BTreeMap<String, Field>,
    },
    /// A tagged union. The discriminant is explicit so a reader never has to
    /// guess a variant from the shape of a payload.
    Union {
        discriminant: String,
        variants: BTreeMap<String, ValueSchema>,
    },
    /// A reference to an artifact produced elsewhere in the run.
    ArtifactRef {
        contract: Option<String>,
    },
    /// An opaque value whose structure was not modelled.
    ///
    /// Legal to produce, and legal to consume where the consumer declares that
    /// it accepts opacity. Never legal to pass off as structure.
    Json {
        reason: String,
    },
}

impl ValueSchema {
    /// A short name for this schema kind, used in mismatch messages.
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::Unit => "unit",
            Self::Bool => "bool",
            Self::Integer { .. } => "integer",
            Self::Decimal { .. } => "decimal",
            Self::Text { .. } => "text",
            Self::BytesRef => "bytes reference",
            Self::Enumeration { .. } => "enumeration",
            Self::List { .. } => "list",
            Self::Record { .. } => "record",
            Self::Union { .. } => "union",
            Self::ArtifactRef { .. } => "artifact reference",
            Self::Json { .. } => "opaque json",
        }
    }

    /// Whether this schema, or anything inside it, is opaque.
    ///
    /// Used to propagate provenance loss: a record containing one opaque field
    /// is not a fully structured record, and pretending otherwise is how a
    /// guarantee quietly becomes a habit.
    #[must_use]
    pub fn carries_opacity(&self) -> bool {
        match self {
            Self::Json { .. } => true,
            Self::List { item, .. } => item.carries_opacity(),
            Self::Record { fields } => fields.values().any(|field| field.schema.carries_opacity()),
            Self::Union { variants, .. } => variants.values().any(ValueSchema::carries_opacity),
            _ => false,
        }
    }

    /// Whether a value described by `self` may be supplied where `required` is
    /// expected.
    ///
    /// Records use width subtyping: a producer may carry extra fields, and a
    /// consumer may declare a field optional that the producer always sends.
    /// Unions are checked the other way around: every variant a producer can
    /// emit must be handled by the requirement, because an unhandled variant is
    /// a runtime surprise.
    ///
    /// # Errors
    ///
    /// Returns the first [`SchemaMismatch`] found, with its path.
    pub fn check_satisfies(&self, required: &Self) -> Result<(), SchemaMismatch> {
        self.check_at("", required)
    }

    #[expect(
        clippy::too_many_lines,
        reason = "one match over the schema kinds reads better than a scattered rule set"
    )]
    fn check_at(&self, path: &str, required: &Self) -> Result<(), SchemaMismatch> {
        let mismatch = || SchemaMismatch::Kind {
            path: display_path(path),
            expected: required.kind_name().to_string(),
            found: self.kind_name().to_string(),
        };

        // Opacity is asymmetric on purpose. A consumer that declares it accepts
        // opaque input may receive anything; an opaque producer may not satisfy
        // a structural requirement, because nothing checked that it matches.
        if matches!(required, Self::Json { .. }) {
            return Ok(());
        }
        if matches!(self, Self::Json { .. }) {
            return Err(SchemaMismatch::OpaqueNotAccepted {
                path: display_path(path),
            });
        }

        match (self, required) {
            (Self::Unit, Self::Unit)
            | (Self::Bool, Self::Bool)
            | (Self::BytesRef, Self::BytesRef) => Ok(()),
            (Self::Integer { range }, Self::Integer { range: expected }) => {
                if range.is_within(expected) {
                    Ok(())
                } else {
                    Err(SchemaMismatch::OutOfRange {
                        path: display_path(path),
                        found: format!("{}..={}", range.minimum, range.maximum),
                        minimum: expected.minimum.to_string(),
                        maximum: expected.maximum.to_string(),
                    })
                }
            }
            (Self::Decimal { scale }, Self::Decimal { scale: expected }) => {
                if scale == expected {
                    Ok(())
                } else {
                    Err(SchemaMismatch::Scale {
                        path: display_path(path),
                        found: *scale,
                        required: *expected,
                    })
                }
            }
            (Self::Text { length }, Self::Text { length: expected }) => {
                if length.is_within(expected) {
                    Ok(())
                } else {
                    Err(SchemaMismatch::OutOfRange {
                        path: display_path(path),
                        found: format!("{}..={}", length.minimum, length.maximum),
                        minimum: expected.minimum.to_string(),
                        maximum: expected.maximum.to_string(),
                    })
                }
            }
            (Self::Enumeration { members }, Self::Enumeration { members: accepted }) => members
                .iter()
                .find(|member| !accepted.contains(member))
                .map_or(Ok(()), |member| {
                    Err(SchemaMismatch::NotAMember {
                        path: display_path(path),
                        value: member.clone(),
                    })
                }),
            (
                Self::List { item, length },
                Self::List {
                    item: expected_item,
                    length: expected_length,
                },
            ) => {
                if !length.is_within(expected_length) {
                    return Err(SchemaMismatch::OutOfRange {
                        path: display_path(path),
                        found: format!("{}..={}", length.minimum, length.maximum),
                        minimum: expected_length.minimum.to_string(),
                        maximum: expected_length.maximum.to_string(),
                    });
                }
                item.check_at(&join(path, "[]"), expected_item)
            }
            (Self::Record { fields }, Self::Record { fields: expected }) => {
                for (name, expected_field) in expected {
                    match fields.get(name) {
                        Some(field) => {
                            if expected_field.required && !field.required {
                                return Err(SchemaMismatch::MissingField {
                                    path: display_path(path),
                                    field: name.clone(),
                                });
                            }
                            field
                                .schema
                                .check_at(&join(path, name), &expected_field.schema)?;
                        }
                        None if expected_field.required => {
                            return Err(SchemaMismatch::MissingField {
                                path: display_path(path),
                                field: name.clone(),
                            });
                        }
                        None => {}
                    }
                }
                Ok(())
            }
            (
                Self::Union {
                    discriminant,
                    variants,
                },
                Self::Union {
                    discriminant: expected_discriminant,
                    variants: expected_variants,
                },
            ) => {
                if discriminant != expected_discriminant {
                    return Err(SchemaMismatch::MissingDiscriminant {
                        path: display_path(path),
                        discriminant: discriminant.clone(),
                    });
                }
                for (name, variant) in variants {
                    let Some(expected_variant) = expected_variants.get(name) else {
                        return Err(SchemaMismatch::UnexpectedVariant {
                            path: display_path(path),
                            variant: name.clone(),
                        });
                    };
                    variant.check_at(&join(path, name), expected_variant)?;
                }
                Ok(())
            }
            (Self::ArtifactRef { contract }, Self::ArtifactRef { contract: expected }) => {
                match (contract, expected) {
                    (_, None) => Ok(()),
                    (Some(contract), Some(expected)) if contract == expected => Ok(()),
                    _ => Err(mismatch()),
                }
            }
            _ => Err(mismatch()),
        }
    }
}

fn join(path: &str, segment: &str) -> String {
    if path.is_empty() {
        segment.to_string()
    } else {
        format!("{path}.{segment}")
    }
}

fn display_path(path: &str) -> String {
    if path.is_empty() {
        "<root>".to_string()
    } else {
        path.to_string()
    }
}

/// The named schemas the current runtime's port tags map onto.
///
/// This table is the single definition of that mapping. The adapters translate
/// through it, so a port keeps the label an author recognises while gaining the
/// structure a checker can act on. Where today's runtime genuinely carries
/// unmodelled content, the alias resolves to [`ValueSchema::Json`] with the
/// reason recorded — visible debt rather than a silent hole.
pub mod aliases {
    use std::collections::BTreeMap;

    use super::{Field, IntegerRange, LengthBounds, ValueSchema};

    /// Text with generous bounds, for prose-shaped values.
    #[must_use]
    pub fn prose() -> ValueSchema {
        ValueSchema::Text {
            length: LengthBounds::new(0, 1_048_576),
        }
    }

    /// A retrieved or ranked document: identity, content digest, and the span
    /// that justifies citing it.
    #[must_use]
    pub fn document() -> ValueSchema {
        let mut fields = BTreeMap::new();
        fields.insert("source_id".to_string(), Field::required(prose()));
        fields.insert(
            "content_digest".to_string(),
            Field::required(ValueSchema::BytesRef),
        );
        fields.insert(
            "span_start".to_string(),
            Field::optional(ValueSchema::Integer {
                range: IntegerRange::new(0, i128::from(u32::MAX)),
            }),
        );
        fields.insert(
            "span_end".to_string(),
            Field::optional(ValueSchema::Integer {
                range: IntegerRange::new(0, i128::from(u32::MAX)),
            }),
        );
        ValueSchema::Record { fields }
    }

    /// An ordered list of documents.
    #[must_use]
    pub fn documents() -> ValueSchema {
        ValueSchema::List {
            item: Box::new(document()),
            length: LengthBounds::new(0, 4_096),
        }
    }

    /// An embedding vector, carried by reference: the numbers themselves are
    /// floating point, which has no canonical encoding, so the vector lives in
    /// object storage and the graph moves its digest.
    #[must_use]
    pub fn embedding() -> ValueSchema {
        ValueSchema::BytesRef
    }

    /// The alias every unmodelled port resolves to, with its reason recorded.
    #[must_use]
    pub fn opaque(reason: &str) -> ValueSchema {
        ValueSchema::Json {
            reason: reason.to_string(),
        }
    }

    /// The mapping from the current runtime's port tag to a structural schema.
    ///
    /// Returns `None` for a tag this table does not know, so a new tag added to
    /// the runtime fails loudly here instead of defaulting to opacity.
    #[must_use]
    pub fn for_port_value_type(tag: &str) -> Option<ValueSchema> {
        let schema = match tag {
            "user_query" | "normalized_query" | "prompt" | "model_response" | "final_answer" => {
                prose()
            }
            "conversation_context" => ValueSchema::List {
                item: Box::new(prose()),
                length: LengthBounds::new(0, 1_024),
            },
            "embedding_vector" => embedding(),
            "retrieved_documents" | "ranked_documents" => documents(),
            "validation_result" => {
                let mut fields = BTreeMap::new();
                fields.insert("passed".to_string(), Field::required(ValueSchema::Bool));
                fields.insert("detail".to_string(), Field::optional(prose()));
                ValueSchema::Record { fields }
            }
            "json" => opaque("the source graph declared an unmodelled JSON port"),
            _ => return None,
        };
        Some(schema)
    }
}
