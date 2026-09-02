//! Nodes, and what each kind of node is allowed to declare.
//!
//! Kind is not decoration for a diagram. A proposer may call a model and may
//! not publish; a verifier may not call a model at all, because a checker that
//! can ask a model to grade its own input is not an independent check; a pure
//! computation may do neither. These rules are enforced when a definition is
//! admitted, which is what makes "models propose, checkers decide" a property
//! of the representation rather than a convention people are asked to follow.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::capability::{CapabilityError, CapabilitySet, Grant};
use crate::effect::{Effect, EffectError, EffectKind};
use crate::id::Identifier;

/// Why a node declaration was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum NodeError {
    #[error("node `{node}` is a {kind} and may not declare effects")]
    EffectNotAllowed {
        node: Identifier,
        kind: &'static str,
    },
    #[error("node `{node}` is a {kind} and must declare the effect it exists to perform")]
    EffectRequired {
        node: Identifier,
        kind: &'static str,
    },
    #[error("node `{node}` is a {kind} and may not declare a `{effect}` effect")]
    EffectKindNotAllowed {
        node: Identifier,
        kind: &'static str,
        effect: &'static str,
    },
    #[error("node `{node}` is a sub-workflow but names no definition to run")]
    MissingSubWorkflow { node: Identifier },
    #[error("node `{node}` declares a budget of zero {resource}, which can never make progress")]
    EmptyBudget {
        node: Identifier,
        resource: &'static str,
    },
    #[error(transparent)]
    Capability(#[from] CapabilityError),
    #[error(transparent)]
    Effect(#[from] EffectError),
}

/// What a node is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NodeKind {
    /// A deterministic function of its inputs. No model, no effect, no network.
    PureComputation,
    /// Anything that proposes: a model, a tool, a retriever, a planner. Its
    /// output is a candidate, never an accepted value.
    Proposer,
    /// A checker. Decides obligations and produces evidence.
    Verifier,
    /// A node that exists to perform a declared effect.
    Effect,
    /// A durable wait for a person. M3 executes it; M2 represents it.
    HumanGate,
    MemoryRead,
    MemoryWrite,
    /// Runs another definition.
    SubWorkflow,
    RegionEntry,
    RegionExit,
}

impl NodeKind {
    /// A short name for messages.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::PureComputation => "pure computation",
            Self::Proposer => "proposer",
            Self::Verifier => "verifier",
            Self::Effect => "effect node",
            Self::HumanGate => "human gate",
            Self::MemoryRead => "memory read",
            Self::MemoryWrite => "memory write",
            Self::SubWorkflow => "sub-workflow",
            Self::RegionEntry => "region entry",
            Self::RegionExit => "region exit",
        }
    }

    /// Whether this kind may hold the given grant.
    #[must_use]
    pub const fn may_hold(self, grant: &Grant) -> bool {
        match grant {
            // Only a proposer may reach a model. A verifier that can call a
            // model is not independent of the thing it checks.
            Grant::ModelProvider { .. } => matches!(self, Self::Proposer),
            Grant::Tool { .. } => matches!(self, Self::Proposer | Self::Effect),
            Grant::ContainerImage { .. } => {
                matches!(self, Self::Proposer | Self::Verifier | Self::Effect)
            }
            Grant::Verifier { .. } => matches!(self, Self::Verifier),
            Grant::Network { .. } => {
                matches!(self, Self::Proposer | Self::Verifier | Self::Effect)
            }
            Grant::Filesystem { .. } => {
                matches!(self, Self::Proposer | Self::Verifier | Self::Effect)
            }
            Grant::Secret { .. } => matches!(self, Self::Proposer | Self::Effect),
            Grant::MemorySpace { .. } => {
                matches!(self, Self::MemoryRead | Self::MemoryWrite)
            }
            Grant::DataResidency { .. } => true,
        }
    }

    /// Whether this kind may declare the given effect kind.
    #[must_use]
    pub const fn may_perform(self, effect: EffectKind) -> bool {
        match self {
            Self::Effect => true,
            Self::MemoryWrite => matches!(effect, EffectKind::MemoryWrite),
            // A proposer may read the world; it may not change it. Publication
            // belongs to an effect node behind a protected boundary.
            Self::Proposer | Self::Verifier => matches!(
                effect,
                EffectKind::Network | EffectKind::Filesystem | EffectKind::SecretAccess
            ),
            _ => false,
        }
    }

    /// Whether this kind may declare any effect at all.
    #[must_use]
    pub const fn permits_any_effect(self) -> bool {
        matches!(
            self,
            Self::Effect | Self::MemoryWrite | Self::Proposer | Self::Verifier
        )
    }

    /// Whether a node of this kind must declare at least one effect.
    #[must_use]
    pub const fn requires_effect(self) -> bool {
        matches!(self, Self::Effect | Self::MemoryWrite)
    }
}

/// What a node may consume, expressed in integers so a digest is stable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResourceBudget {
    pub wall_ms: u64,
    pub tokens: u64,
    /// Cost in millionths of the accounting unit. Integers, because a float
    /// budget cannot be digested reproducibly.
    pub cost_micro_units: u64,
    pub effect_count: u32,
}

impl ResourceBudget {
    /// A budget that permits deterministic local work and nothing else.
    #[must_use]
    pub const fn deterministic(wall_ms: u64) -> Self {
        Self {
            wall_ms,
            tokens: 0,
            cost_micro_units: 0,
            effect_count: 0,
        }
    }

    /// Whether this budget fits inside `parent`.
    #[must_use]
    pub const fn is_within(&self, parent: &Self) -> bool {
        self.wall_ms <= parent.wall_ms
            && self.tokens <= parent.tokens
            && self.cost_micro_units <= parent.cost_micro_units
            && self.effect_count <= parent.effect_count
    }
}

/// How a proposer node is bound to whatever proposes.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderBinding {
    /// The granted capability this binding spends.
    pub capability: Identifier,
    /// The exact model, tool, or image selected from that grant.
    pub selection: String,
}

/// A node in the graph.
///
/// Ports are added by the graph module; this is the node's own declaration of
/// what it is and what it may reach.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Node {
    pub id: Identifier,
    pub name: String,
    pub kind: NodeKind,
    /// Capabilities this node spends. Every one must be granted by the
    /// enclosing definition.
    pub capabilities: Vec<Identifier>,
    pub effects: Vec<Effect>,
    pub budget: ResourceBudget,
    pub provider: Option<ProviderBinding>,
    /// For a sub-workflow node, the definition it runs.
    pub sub_workflow: Option<Identifier>,
}

impl Node {
    /// Checks this node against the capabilities the definition granted.
    ///
    /// # Errors
    ///
    /// Returns [`NodeError`] for the first violated rule: an ungranted
    /// capability, a capability this kind may not hold, an effect this kind may
    /// not perform, an effect with no authorising capability, a missing
    /// sub-workflow reference, or a malformed effect declaration.
    pub fn check(&self, granted: &CapabilitySet) -> Result<(), NodeError> {
        for capability in &self.capabilities {
            let Some(grant) = granted.grant(capability) else {
                return Err(CapabilityError::NotGranted {
                    node: self.id.clone(),
                    capability: capability.clone(),
                }
                .into());
            };
            if !self.kind.may_hold(grant) {
                return Err(CapabilityError::NotPermittedForKind {
                    node: self.id.clone(),
                    kind: self.kind.as_str(),
                    capability: capability.clone(),
                    grant: grant.kind_name(),
                }
                .into());
            }
        }

        if let Some(provider) = &self.provider
            && !self.capabilities.contains(&provider.capability)
        {
            return Err(CapabilityError::NotGranted {
                node: self.id.clone(),
                capability: provider.capability.clone(),
            }
            .into());
        }

        if self.kind.requires_effect() && self.effects.is_empty() {
            return Err(NodeError::EffectRequired {
                node: self.id.clone(),
                kind: self.kind.as_str(),
            });
        }

        let mut seen: Vec<&Identifier> = Vec::new();
        for effect in &self.effects {
            effect.check()?;
            if seen.contains(&&effect.id) {
                return Err(EffectError::Duplicate {
                    node: self.id.clone(),
                    effect: effect.id.clone(),
                }
                .into());
            }
            seen.push(&effect.id);

            if !self.kind.may_perform(effect.kind) {
                return Err(if self.kind.permits_any_effect() {
                    NodeError::EffectKindNotAllowed {
                        node: self.id.clone(),
                        kind: self.kind.as_str(),
                        effect: effect.kind.as_str(),
                    }
                } else {
                    NodeError::EffectNotAllowed {
                        node: self.id.clone(),
                        kind: self.kind.as_str(),
                    }
                });
            }

            // The capability the effect names must be one this node holds, not
            // merely one the definition granted to somebody.
            if !self.capabilities.contains(&effect.capability) {
                return Err(EffectError::Undeclared {
                    node: self.id.clone(),
                    effect: effect.id.clone(),
                }
                .into());
            }
        }

        if matches!(self.kind, NodeKind::SubWorkflow) && self.sub_workflow.is_none() {
            return Err(NodeError::MissingSubWorkflow {
                node: self.id.clone(),
            });
        }

        if !self.effects.is_empty() && self.budget.effect_count == 0 {
            return Err(NodeError::EmptyBudget {
                node: self.id.clone(),
                resource: "effects",
            });
        }
        if matches!(self.kind, NodeKind::Proposer) && self.budget.tokens == 0 {
            return Err(NodeError::EmptyBudget {
                node: self.id.clone(),
                resource: "tokens",
            });
        }

        Ok(())
    }
}
