//! A workflow definition: the whole unit that gets admitted, stored, and
//! pinned by a certificate.
//!
//! Definitions are immutable. A change produces a new version with a new
//! digest, because a run that says it executed version 3 has to be able to
//! point at the exact bytes of version 3 forever.

use serde::{Deserialize, Serialize};

use crate::capability::CapabilitySet;
use crate::correctness::obligation::Contract;
use crate::effect::ProtectedBoundary;
use crate::graph::Graph;
use crate::id::Identifier;
use crate::node::ResourceBudget;
use crate::version::{IR_SCHEMA_VERSION, SchemaVersion};

/// How much the correctness plane does for a definition or a subgraph.
///
/// The three modes differ only in what happens to domain obligations and
/// protected boundaries. None of them turns off structural admission: `Observe`
/// means nobody checked the domain properties, not that a malformed or
/// unbounded definition may run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssuranceMode {
    /// Run, capture evidence, and label the output `unverified`.
    Observe,
    /// Evaluate the declared obligations and report a verdict. Callers decide
    /// what to do with it.
    Verify,
    /// Evaluate the obligations, and refuse to cross a protected boundary
    /// unless the policy's minimum verdict is met.
    Enforce,
}

impl AssuranceMode {
    /// A short name for messages and storage.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Observe => "observe",
            Self::Verify => "verify",
            Self::Enforce => "enforce",
        }
    }

    /// Whether this mode evaluates domain obligations at all.
    #[must_use]
    pub const fn evaluates_obligations(self) -> bool {
        matches!(self, Self::Verify | Self::Enforce)
    }

    /// Whether this mode can stop a boundary from being crossed.
    #[must_use]
    pub const fn enforces_boundaries(self) -> bool {
        matches!(self, Self::Enforce)
    }

    /// The stricter of two modes.
    ///
    /// Used when a subgraph declares its own mode: the enclosing scope can only
    /// be tightened by what it contains, never loosened, so a region cannot opt
    /// out of enforcement its parent applied.
    #[must_use]
    pub fn strictest(self, other: Self) -> Self {
        if self >= other { self } else { other }
    }
}

/// A versioned workflow definition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Definition {
    pub schema_version: SchemaVersion,
    pub id: Identifier,
    /// The version of this definition. Immutable once written.
    pub version: String,
    pub name: String,
    /// The mode the whole definition runs under, before any region tightens it.
    pub assurance: AssuranceMode,
    /// Everything this definition may reach.
    pub capabilities: CapabilitySet,
    /// The ceiling every node and region sits under.
    pub budget: ResourceBudget,
    pub graph: Graph,
    /// The boundaries a policy may gate.
    pub boundaries: Vec<ProtectedBoundary>,
    /// The contracts this definition's obligations come from.
    pub contracts: Vec<Contract>,
}

impl Definition {
    /// The schema version this build writes.
    #[must_use]
    pub fn current_schema_version() -> SchemaVersion {
        IR_SCHEMA_VERSION
    }

    /// The contract behind an identifier.
    #[must_use]
    pub fn contract(&self, id: &Identifier) -> Option<&Contract> {
        self.contracts.iter().find(|contract| &contract.id == id)
    }

    /// The boundary behind an identifier.
    #[must_use]
    pub fn boundary(&self, id: &Identifier) -> Option<&ProtectedBoundary> {
        self.boundaries.iter().find(|boundary| &boundary.id == id)
    }
}
