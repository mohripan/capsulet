//! Typed ports.
//!
//! A port carries two things, and both of them are load-bearing: the structure
//! of the value, and its assurance. Keeping trust on the port rather than in a
//! side table is what makes an unearned trust transition a structural error
//! instead of a policy someone has to remember to apply.
//!
//! Inputs state a requirement; outputs state a claim. A claim has to be
//! justified, and [`OutputPort`] cannot be built with a strengthened claim
//! except through a verification record.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::id::Identifier;
use crate::trust::{TrustClass, VerificationRecord};
use crate::value::ValueSchema;

/// How much assurance a consumer insists on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TrustLevel {
    Unverified,
    Conditional,
    Verified,
}

impl TrustLevel {
    /// The level of a class.
    #[must_use]
    pub const fn of(class: &TrustClass) -> Self {
        match class {
            TrustClass::Unverified => Self::Unverified,
            TrustClass::Conditional { .. } => Self::Conditional,
            TrustClass::Verified { .. } => Self::Verified,
        }
    }

    /// A short name for messages.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Unverified => "unverified",
            Self::Conditional => "conditional",
            Self::Verified => "verified",
        }
    }
}

/// What a consumer requires of the values it receives.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrustRequirement {
    pub minimum: TrustLevel,
    /// The contract the assurance must have been established under. `None`
    /// accepts any contract, which is only appropriate where the consumer truly
    /// does not care which property was checked.
    pub contract: Option<Identifier>,
}

impl TrustRequirement {
    /// The requirement that accepts anything.
    #[must_use]
    pub const fn none() -> Self {
        Self {
            minimum: TrustLevel::Unverified,
            contract: None,
        }
    }

    /// Requires a verdict of at least `minimum` under the named contract.
    #[must_use]
    pub const fn at_least(minimum: TrustLevel, contract: Identifier) -> Self {
        Self {
            minimum,
            contract: Some(contract),
        }
    }

    /// Whether a value of this class may be supplied here.
    #[must_use]
    pub fn is_satisfied_by(&self, class: &TrustClass) -> bool {
        if TrustLevel::of(class) < self.minimum {
            return false;
        }
        match (&self.contract, class.contract()) {
            (None, _) => true,
            (Some(required), Some(established)) => required.as_str() == established,
            (Some(_), None) => false,
        }
    }
}

/// Why an output port claim was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PortError {
    #[error(
        "output port `{port}` claims `{claimed}` trust without a verification record; assurance is \
         established by a checker, not by a declaration"
    )]
    UnjustifiedClaim {
        port: Identifier,
        claimed: &'static str,
    },
}

/// A value a node requires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InputPort {
    pub id: Identifier,
    pub schema: ValueSchema,
    pub requires: TrustRequirement,
}

impl InputPort {
    /// An input with no assurance requirement.
    #[must_use]
    pub const fn new(id: Identifier, schema: ValueSchema) -> Self {
        Self {
            id,
            schema,
            requires: TrustRequirement::none(),
        }
    }

    /// An input that will not accept a value below `requires`.
    #[must_use]
    pub const fn guarded(id: Identifier, schema: ValueSchema, requires: TrustRequirement) -> Self {
        Self {
            id,
            schema,
            requires,
        }
    }
}

/// A value a node produces.
///
/// `produces` is deliberately not public: an output that claims to be verified
/// has to have been built from a record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OutputPort {
    pub id: Identifier,
    pub schema: ValueSchema,
    produces: TrustClass,
}

impl OutputPort {
    /// An output that carries no assurance, which is what almost every node
    /// produces.
    #[must_use]
    pub const fn new(id: Identifier, schema: ValueSchema) -> Self {
        Self {
            id,
            schema,
            produces: TrustClass::Unverified,
        }
    }

    /// An output whose assurance is established by a verification record.
    ///
    /// The class is whatever the record justifies, never what the caller hoped
    /// for, so this constructor cannot be used to strengthen anything.
    #[must_use]
    pub fn established(id: Identifier, schema: ValueSchema, record: &VerificationRecord) -> Self {
        Self {
            id,
            schema,
            produces: TrustClass::from_record(record),
        }
    }

    /// What this port produces.
    #[must_use]
    pub const fn produces(&self) -> &TrustClass {
        &self.produces
    }
}
