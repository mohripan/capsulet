//! Capabilities: what a definition is allowed to reach.
//!
//! A capability is granted once, at the definition level, and referenced by
//! identifier from the nodes that use it. Nothing a node says at runtime can
//! create one. This is the structural answer to the failure mode where a
//! planner emits the name of a tool it was never given and the runtime, seeing
//! a plausible string, goes and calls it: here that name resolves to no grant,
//! and the definition does not admit.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::digest::Digest;
use crate::id::Identifier;

/// Why a capability reference or grant was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CapabilityError {
    #[error(
        "node `{node}` references capability `{capability}`, which the definition never granted"
    )]
    NotGranted {
        node: Identifier,
        capability: Identifier,
    },
    #[error(
        "node `{node}` is a {kind} and may not hold capability `{capability}`, which grants {grant}"
    )]
    NotPermittedForKind {
        node: Identifier,
        kind: &'static str,
        capability: Identifier,
        grant: &'static str,
    },
    #[error("capability `{capability}` is granted twice")]
    Duplicate { capability: Identifier },
    #[error("region capability `{capability}` is not granted by the enclosing scope")]
    WidensParent { capability: Identifier },
}

/// What a capability permits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Grant {
    /// A model provider and the exact models allowed from it.
    ModelProvider {
        provider: Identifier,
        models: Vec<String>,
    },
    /// A named tool.
    Tool { tool: Identifier },
    /// A container image, pinned by digest. An image reference without a digest
    /// names whatever the registry serves today, which is not a capability, it
    /// is a hope.
    ContainerImage { image: String, digest: Digest },
    /// A verifier identity and version, for declared oracles.
    Verifier {
        identity: Identifier,
        version: String,
    },
    /// Outbound network access to named hosts.
    Network { hosts: Vec<String> },
    /// Filesystem access to named paths.
    Filesystem { paths: Vec<String>, write: bool },
    /// Access to named secrets. The values never appear in the IR.
    Secret { names: Vec<Identifier> },
    /// Access to a governed-memory space.
    MemorySpace { space: Identifier, write: bool },
    /// Where data may be processed.
    DataResidency { regions: Vec<String> },
}

impl Grant {
    /// A short name for this grant, used in messages.
    #[must_use]
    pub const fn kind_name(&self) -> &'static str {
        match self {
            Self::ModelProvider { .. } => "a model provider",
            Self::Tool { .. } => "a tool",
            Self::ContainerImage { .. } => "a container image",
            Self::Verifier { .. } => "a verifier identity",
            Self::Network { .. } => "network access",
            Self::Filesystem { .. } => "filesystem access",
            Self::Secret { .. } => "secret access",
            Self::MemorySpace { .. } => "a memory space",
            Self::DataResidency { .. } => "a data residency region",
        }
    }
}

/// One granted capability.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capability {
    pub id: Identifier,
    pub grant: Grant,
}

/// The capabilities a definition or region grants.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct CapabilitySet {
    capabilities: BTreeMap<Identifier, Grant>,
}

impl CapabilitySet {
    /// Builds a capability set, refusing duplicate grants.
    ///
    /// # Errors
    ///
    /// Returns [`CapabilityError::Duplicate`] when an identifier is granted
    /// twice, because the second grant would silently shadow the first.
    pub fn new(capabilities: Vec<Capability>) -> Result<Self, CapabilityError> {
        let mut map = BTreeMap::new();
        for capability in capabilities {
            if map.contains_key(&capability.id) {
                return Err(CapabilityError::Duplicate {
                    capability: capability.id,
                });
            }
            map.insert(capability.id, capability.grant);
        }
        Ok(Self { capabilities: map })
    }

    /// An empty set: the definition grants nothing.
    #[must_use]
    pub fn empty() -> Self {
        Self::default()
    }

    /// The grant behind an identifier, if it was granted.
    #[must_use]
    pub fn grant(&self, id: &Identifier) -> Option<&Grant> {
        self.capabilities.get(id)
    }

    /// Whether the identifier was granted.
    #[must_use]
    pub fn contains(&self, id: &Identifier) -> bool {
        self.capabilities.contains_key(id)
    }

    /// The granted identifiers, in canonical order.
    pub fn ids(&self) -> impl Iterator<Item = &Identifier> {
        self.capabilities.keys()
    }

    /// Checks that this set only narrows `parent`.
    ///
    /// A region may hold fewer capabilities than its parent, never more, and
    /// never a different grant behind the same name. Widening in a nested scope
    /// is how a sandbox becomes decorative.
    ///
    /// # Errors
    ///
    /// Returns [`CapabilityError::WidensParent`] for the first capability this
    /// set holds that the parent does not grant identically.
    pub fn check_narrows(&self, parent: &Self) -> Result<(), CapabilityError> {
        for (id, grant) in &self.capabilities {
            if parent.grant(id) != Some(grant) {
                return Err(CapabilityError::WidensParent {
                    capability: id.clone(),
                });
            }
        }
        Ok(())
    }
}
