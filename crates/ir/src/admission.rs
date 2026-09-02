//! Structural admission: the checks that run whatever the assurance mode is.
//!
//! This is the floor. `Observe` means the domain obligations were not
//! evaluated; it has never meant that a malformed graph, an undeclared effect,
//! an unbounded loop, or an ungranted capability is acceptable. Those are
//! refused in every mode, because they are not domain questions — nobody's
//! policy makes an unbounded loop safe.
//!
//! Two properties matter as much as the rules themselves.
//!
//! Admission is **total**: it returns a decision for every definition, never
//! panics, and never runs forever. A checker that can crash on hostile input is
//! a checker that can be made to skip a check.
//!
//! Admission is **recorded**: passing produces an [`AdmissionRecord`], which is
//! the only way to build one, and a certificate cannot be assembled without it.
//! "This definition was structurally admitted" is therefore provable after the
//! fact rather than assumed.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::canonical::CanonicalError;
use crate::correctness::obligation::RepairOwner;
use crate::definition::Definition;
use crate::digest::Digest;
use crate::effect::{BoundaryError, Crossing};
use crate::graph::GraphError;
use crate::id::Identifier;
use crate::region::RegionError;

/// What kind of structural rule was broken.
///
/// Codes are stable identifiers a caller can route on, and each one has an
/// owning subsystem so a rejection says whose problem it is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AdmissionCode {
    /// The graph is not a graph: dangling references, duplicate identifiers,
    /// endpoints pointing the wrong way.
    GraphInvalid,
    /// A value cannot flow where it was wired to flow.
    PortIncompatible,
    /// A node performs an effect it did not declare, or the boundary over it
    /// protects nothing.
    EffectUndeclared,
    /// A node reaches for a capability the definition never granted, or one its
    /// kind may not hold.
    CapabilityUngranted,
    /// A loop without finite bounds, or a cycle nobody declared as a loop.
    RepetitionUnbounded,
    /// A budget that is missing, empty, or wider than the scope above it.
    BudgetInvalid,
    /// A combination whose result cannot be traced to its sources.
    ProvenanceMissing,
    /// Trust that would strengthen without a checker establishing it.
    TrustEdgeIllegal,
    /// A schema, digest, or version that does not parse or does not agree with
    /// what it claims.
    SchemaInvalid,
    /// A protected boundary that names something the definition does not
    /// declare.
    BoundaryInvalid,
    /// A contract that is declared twice or references an unknown obligation.
    ContractInvalid,
}

impl AdmissionCode {
    /// The stable name used in messages, storage, and the published rule table.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::GraphInvalid => "graph_invalid",
            Self::PortIncompatible => "port_incompatible",
            Self::EffectUndeclared => "effect_undeclared",
            Self::CapabilityUngranted => "capability_ungranted",
            Self::RepetitionUnbounded => "repetition_unbounded",
            Self::BudgetInvalid => "budget_invalid",
            Self::ProvenanceMissing => "provenance_missing",
            Self::TrustEdgeIllegal => "trust_edge_illegal",
            Self::SchemaInvalid => "schema_invalid",
            Self::BoundaryInvalid => "boundary_invalid",
            Self::ContractInvalid => "contract_invalid",
        }
    }

    /// Who has to act on a rejection of this kind.
    #[must_use]
    pub const fn owner(self) -> RepairOwner {
        match self {
            // Everything structural is the author's problem to fix in the
            // definition, which the runtime cannot do on anyone's behalf.
            Self::GraphInvalid
            | Self::PortIncompatible
            | Self::EffectUndeclared
            | Self::RepetitionUnbounded
            | Self::BudgetInvalid
            | Self::ProvenanceMissing
            | Self::BoundaryInvalid
            | Self::ContractInvalid
            | Self::SchemaInvalid => RepairOwner::Runtime,
            // A missing grant is a policy decision, not an authoring slip.
            Self::CapabilityUngranted => RepairOwner::Policy,
            // Trust that would strengthen without a checker needs a checker.
            Self::TrustEdgeIllegal => RepairOwner::Verifier,
        }
    }

    /// What this rule refuses, for the published rule table.
    #[must_use]
    pub const fn description(self) -> &'static str {
        match self {
            Self::GraphInvalid => {
                "The graph references nodes, ports, or identifiers that do not exist, or declares \
                 the same identifier twice."
            }
            Self::PortIncompatible => {
                "An edge delivers a value that does not satisfy the schema of the port it feeds."
            }
            Self::EffectUndeclared => {
                "A node performs an effect it did not declare, an effect its kind may not perform, \
                 or a protected boundary is declared over an effect that does not exist."
            }
            Self::CapabilityUngranted => {
                "A node reaches for a capability the definition never granted, holds one its kind \
                 may not hold, or a nested scope widens what it was given."
            }
            Self::RepetitionUnbounded => {
                "A loop lacks a finite bound, or a cycle exists outside any loop region."
            }
            Self::BudgetInvalid => {
                "A budget is empty where work must happen, or a scope claims more than the scope \
                 that contains it."
            }
            Self::ProvenanceMissing => {
                "A combination produces a value that cannot be traced back to the sources that \
                 contributed to it."
            }
            Self::TrustEdgeIllegal => {
                "An edge delivers less assurance than a port requires, or claims a contract from a \
                 node that is not a verifier."
            }
            Self::SchemaInvalid => {
                "A schema version, digest, or encoding does not parse or does not match what the \
                 document claims."
            }
            Self::BoundaryInvalid => {
                "A protected boundary names a node or effect the definition does not declare."
            }
            Self::ContractInvalid => {
                "A contract is declared twice, or its obligations are not distinct."
            }
        }
    }

    /// Every code, in a stable order, for the published rule table.
    #[must_use]
    pub const fn all() -> [Self; 11] {
        [
            Self::GraphInvalid,
            Self::PortIncompatible,
            Self::EffectUndeclared,
            Self::CapabilityUngranted,
            Self::RepetitionUnbounded,
            Self::BudgetInvalid,
            Self::ProvenanceMissing,
            Self::TrustEdgeIllegal,
            Self::SchemaInvalid,
            Self::BoundaryInvalid,
            Self::ContractInvalid,
        ]
    }
}

/// A refusal: what was broken, whose problem it is, and what exactly went
/// wrong.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("{code} ({owner}): {detail}", code = self.code.as_str(), owner = self.owner.as_str())]
pub struct AdmissionRefusal {
    pub code: AdmissionCode,
    pub owner: RepairOwner,
    pub detail: String,
}

impl AdmissionRefusal {
    fn new(code: AdmissionCode, detail: impl Into<String>) -> Self {
        Self {
            code,
            owner: code.owner(),
            detail: detail.into(),
        }
    }
}

/// Proof that a definition passed structural admission.
///
/// Constructible only by [`admit`], and required to assemble a certificate.
/// That is the whole point: a definition that did not pass cannot produce a
/// verdict at all — not `unverified`, not anything. There is nothing to say
/// about a graph nobody could read.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AdmissionRecord {
    /// The definition this record is about, pinned by digest.
    definition: Digest,
    /// The codes that were evaluated, so a later reader knows which rules this
    /// build applied rather than assuming it applied today's set.
    rules_applied: Vec<AdmissionCode>,
}

impl AdmissionRecord {
    /// The definition this admission was for.
    #[must_use]
    pub const fn definition(&self) -> &Digest {
        &self.definition
    }

    /// The rules that were applied.
    #[must_use]
    pub fn rules_applied(&self) -> &[AdmissionCode] {
        &self.rules_applied
    }
}

/// Runs structural admission.
///
/// Total: every definition gets a decision. The rules are applied in a fixed
/// order so the first refusal is deterministic, which matters when the result
/// is quoted back to whoever has to fix it.
///
/// # Errors
///
/// Returns [`AdmissionRefusal`] describing the first rule broken.
pub fn admit(definition: &Definition) -> Result<AdmissionRecord, AdmissionRefusal> {
    let digest = crate::digest_of(definition).map_err(|source| schema_refusal(&source))?;

    definition
        .graph
        .check(&definition.capabilities, &definition.budget)
        .map_err(|error| refusal_for_graph(&error))?;

    check_boundaries(definition)?;
    check_contracts(definition)?;

    Ok(AdmissionRecord {
        definition: digest,
        rules_applied: AdmissionCode::all().to_vec(),
    })
}

fn schema_refusal(source: &CanonicalError) -> AdmissionRefusal {
    AdmissionRefusal::new(
        AdmissionCode::SchemaInvalid,
        format!("the definition has no canonical encoding: {source}"),
    )
}

fn check_boundaries(definition: &Definition) -> Result<(), AdmissionRefusal> {
    let mut seen: Vec<&Identifier> = Vec::new();
    for boundary in &definition.boundaries {
        if seen.contains(&&boundary.id) {
            return Err(boundary_refusal(&BoundaryError::Duplicate {
                boundary: boundary.id.clone(),
            }));
        }
        seen.push(&boundary.id);

        let Some(node) = definition.graph.node(&boundary.node) else {
            return Err(boundary_refusal(&BoundaryError::UnknownNode {
                boundary: boundary.id.clone(),
                node: boundary.node.clone(),
            }));
        };

        if let Crossing::Effect { effect } = &boundary.crossing
            && !node.effects.iter().any(|declared| &declared.id == effect)
        {
            return Err(AdmissionRefusal::new(
                AdmissionCode::EffectUndeclared,
                BoundaryError::NoSuchEffect {
                    boundary: boundary.id.clone(),
                    node: boundary.node.clone(),
                    effect: effect.clone(),
                }
                .to_string(),
            ));
        }
    }
    Ok(())
}

fn boundary_refusal(error: &BoundaryError) -> AdmissionRefusal {
    AdmissionRefusal::new(AdmissionCode::BoundaryInvalid, error.to_string())
}

fn check_contracts(definition: &Definition) -> Result<(), AdmissionRefusal> {
    let mut seen: Vec<&Identifier> = Vec::new();
    for contract in &definition.contracts {
        if seen.contains(&&contract.id) {
            return Err(AdmissionRefusal::new(
                AdmissionCode::ContractInvalid,
                format!("contract `{}` is declared twice", contract.id),
            ));
        }
        seen.push(&contract.id);

        let mut obligations: Vec<&Identifier> = Vec::new();
        for obligation in &contract.obligations {
            if obligations.contains(&&obligation.id) {
                return Err(AdmissionRefusal::new(
                    AdmissionCode::ContractInvalid,
                    format!(
                        "contract `{}` states obligation `{}` twice",
                        contract.id, obligation.id
                    ),
                ));
            }
            obligations.push(&obligation.id);
        }
    }
    Ok(())
}

/// Maps a graph failure onto the structural rule it broke.
///
/// The graph reports what is wrong in its own vocabulary; admission reports
/// which rule that violates and who owns the repair. Keeping the mapping here,
/// exhaustively, means a new graph error cannot quietly arrive without a code.
fn refusal_for_graph(error: &GraphError) -> AdmissionRefusal {
    let code = match error {
        GraphError::Duplicate { .. }
        | GraphError::UnknownNode { .. }
        | GraphError::UnknownPort { .. }
        | GraphError::UnknownGraphPort { .. }
        | GraphError::OutputUsedAsSource { .. }
        | GraphError::InputUsedAsTarget { .. }
        | GraphError::Dangling { .. }
        | GraphError::ForwardsMany { .. }
        | GraphError::BranchNotSelectable { .. }
        | GraphError::BranchNotExhaustive { .. }
        | GraphError::BranchArmUnreachable { .. }
        | GraphError::ValueEscapesRegion { .. }
        | GraphError::ValueBypassesRegionEntry { .. } => AdmissionCode::GraphInvalid,
        GraphError::Schema { .. }
        | GraphError::ConcatNotLists { .. }
        | GraphError::SourceIndexOutOfRange { .. } => AdmissionCode::PortIncompatible,
        GraphError::ProvenanceLost { .. } | GraphError::SelectionNotRecorded { .. } => {
            AdmissionCode::ProvenanceMissing
        }
        GraphError::TrustTooWeak { .. } | GraphError::NotAVerifier { .. } => {
            AdmissionCode::TrustEdgeIllegal
        }
        GraphError::UndeclaredCycle { .. } => AdmissionCode::RepetitionUnbounded,
        GraphError::Node(node) => return refusal_for_node(node),
        GraphError::Region(region) => return refusal_for_region(region),
    };
    AdmissionRefusal::new(code, error.to_string())
}

fn refusal_for_node(error: &crate::node::NodeError) -> AdmissionRefusal {
    use crate::node::NodeError;

    let code = match error {
        NodeError::EffectNotAllowed { .. }
        | NodeError::EffectRequired { .. }
        | NodeError::EffectKindNotAllowed { .. }
        | NodeError::Effect(_) => AdmissionCode::EffectUndeclared,
        NodeError::Capability(_) => AdmissionCode::CapabilityUngranted,
        NodeError::EmptyBudget { .. } => AdmissionCode::BudgetInvalid,
        NodeError::MissingSubWorkflow { .. } => AdmissionCode::GraphInvalid,
    };
    AdmissionRefusal::new(code, error.to_string())
}

fn refusal_for_region(error: &RegionError) -> AdmissionRefusal {
    let code = match error {
        RegionError::UnknownNode { .. }
        | RegionError::WrongBoundaryKind { .. }
        | RegionError::BoundaryOutsideRegion { .. }
        | RegionError::UnknownParent { .. }
        | RegionError::OverlappingMembership { .. }
        | RegionError::CyclicNesting { .. } => AdmissionCode::GraphInvalid,
        RegionError::BudgetWidensParent { .. } => AdmissionCode::BudgetInvalid,
        RegionError::Capability { .. } => AdmissionCode::CapabilityUngranted,
        RegionError::Loop(_) => AdmissionCode::RepetitionUnbounded,
    };
    AdmissionRefusal::new(code, error.to_string())
}
