//! Nested regions.
//!
//! A region is a scope: a set of nodes that share a budget, a set of
//! capabilities, and a single way in and a single way out. Values cross the
//! boundary only through the region's entry and exit nodes, which is what makes
//! "what did this subgraph consume, and what did it hand back" a question with
//! an answer.
//!
//! Nesting narrows. A region may hold fewer capabilities and a smaller budget
//! than its parent, never more. A scope that could widen its parent would make
//! the parent's limits advisory, and an advisory limit is not a limit.

use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::capability::{CapabilityError, CapabilitySet};
use crate::id::Identifier;
use crate::loop_region::{LoopError, LoopSpec};
use crate::node::ResourceBudget;

/// Why a region declaration was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RegionError {
    #[error("region `{region}` lists node `{node}`, which the graph does not declare")]
    UnknownNode {
        region: Identifier,
        node: Identifier,
    },
    #[error("region `{region}` names `{node}` as its {role}, but that node is not a {expected}")]
    WrongBoundaryKind {
        region: Identifier,
        node: Identifier,
        role: &'static str,
        expected: &'static str,
    },
    #[error("region `{region}` does not contain its own {role} node `{node}`")]
    BoundaryOutsideRegion {
        region: Identifier,
        node: Identifier,
        role: &'static str,
    },
    #[error("region `{region}` names parent `{parent}`, which does not exist")]
    UnknownParent {
        region: Identifier,
        parent: Identifier,
    },
    #[error("regions `{region}` and `{other}` both contain node `{node}`")]
    OverlappingMembership {
        region: Identifier,
        other: Identifier,
        node: Identifier,
    },
    #[error("region `{region}` declares a budget larger than the scope that contains it")]
    BudgetWidensParent { region: Identifier },
    #[error("region `{region}` is its own ancestor")]
    CyclicNesting { region: Identifier },
    #[error("in region `{region}`: {source}")]
    Capability {
        region: Identifier,
        #[source]
        source: CapabilityError,
    },
    #[error(transparent)]
    Loop(#[from] LoopError),
}

/// What kind of scope a region is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RegionKind {
    /// A scope that runs its body once.
    Plain,
    /// A scope that may repeat its body, under the bounds it declares.
    Loop { spec: Box<LoopSpec> },
}

impl RegionKind {
    /// Whether this region may contain a cycle.
    #[must_use]
    pub const fn permits_cycles(&self) -> bool {
        matches!(self, Self::Loop { .. })
    }

    /// A short name for messages.
    #[must_use]
    pub const fn as_str(&self) -> &'static str {
        match self {
            Self::Plain => "plain region",
            Self::Loop { .. } => "loop region",
        }
    }
}

/// A nested scope.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Region {
    pub id: Identifier,
    pub kind: RegionKind,
    /// The enclosing region, or `None` for a region directly inside the graph.
    pub parent: Option<Identifier>,
    /// The node values enter through. Must be a `RegionEntry` node in `nodes`.
    pub entry: Identifier,
    /// The node values leave through. Must be a `RegionExit` node in `nodes`.
    pub exit: Identifier,
    /// The nodes directly inside this region, including its entry and exit.
    pub nodes: BTreeSet<Identifier>,
    pub capabilities: CapabilitySet,
    pub budget: ResourceBudget,
}

impl Region {
    /// Whether this region directly contains the node.
    #[must_use]
    pub fn contains(&self, node: &Identifier) -> bool {
        self.nodes.contains(node)
    }

    /// Checks this region against the scope that encloses it.
    ///
    /// # Errors
    ///
    /// Returns [`RegionError`] when the region widens its parent's capabilities
    /// or budget.
    pub fn check_against_parent(
        &self,
        parent_capabilities: &CapabilitySet,
        parent_budget: &ResourceBudget,
    ) -> Result<(), RegionError> {
        self.capabilities
            .check_narrows(parent_capabilities)
            .map_err(|source| RegionError::Capability {
                region: self.id.clone(),
                source,
            })?;
        if !self.budget.is_within(parent_budget) {
            return Err(RegionError::BudgetWidensParent {
                region: self.id.clone(),
            });
        }
        Ok(())
    }
}
