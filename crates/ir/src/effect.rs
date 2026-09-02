//! Effects: what a node does that the outside world can notice.
//!
//! An effect is declared, or it is not permitted. The declaration says what kind
//! of effect it is, what it touches, which granted capability authorises it,
//! whether it can be undone, and — the part that matters most for recovery —
//! whether repeating it is safe.
//!
//! Idempotency is declared rather than assumed because the runtime cannot
//! discover it. A worker that crashed after calling an endpoint and before
//! recording the call has no way to ask the endpoint whether it counted. The
//! only honest options are to know in advance that repeating is harmless, to
//! carry a key the far side deduplicates on, or to say plainly that this effect
//! must not be repeated and let the runtime stop rather than guess.

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::id::Identifier;

/// Why an effect declaration was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EffectError {
    #[error("node `{node}` performs effect `{effect}` but declares no capability for it")]
    Undeclared {
        node: Identifier,
        effect: Identifier,
    },
    #[error("effect `{effect}` is keyed but names no idempotency key source")]
    MissingIdempotencyKey { effect: Identifier },
    #[error("effect `{effect}` claims to be reversible but names no compensation route")]
    MissingCompensation { effect: Identifier },
    #[error("effect `{effect}` is declared twice on node `{node}`")]
    Duplicate {
        node: Identifier,
        effect: Identifier,
    },
}

/// The kind of effect.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EffectKind {
    Network,
    Filesystem,
    SecretAccess,
    /// Anything that makes work visible outside the run: opening a pull
    /// request, releasing an artifact, sending a message.
    Publication,
    /// A write into governed memory, which is a trust transition as well as a
    /// side effect.
    MemoryWrite,
    /// An effect on a system this definition does not model.
    ExternalSideEffect,
}

impl EffectKind {
    /// Whether this kind of effect is, by its nature, visible outside the run.
    ///
    /// Used to catch the case where a definition marks nothing as protected
    /// even though it publishes.
    #[must_use]
    pub const fn is_outward_facing(self) -> bool {
        matches!(
            self,
            Self::Publication | Self::MemoryWrite | Self::ExternalSideEffect
        )
    }

    /// A short name for messages.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Network => "network",
            Self::Filesystem => "filesystem",
            Self::SecretAccess => "secret_access",
            Self::Publication => "publication",
            Self::MemoryWrite => "memory_write",
            Self::ExternalSideEffect => "external_side_effect",
        }
    }
}

/// Whether repeating an effect is safe.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Idempotency {
    /// Repeating changes nothing.
    Idempotent,
    /// The far side deduplicates on a key this run supplies.
    Keyed { key_source: String },
    /// Repeating is not safe. A runtime that cannot prove the effect happened
    /// exactly once must stop rather than retry.
    NonIdempotent,
}

/// Whether an effect can be undone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Reversibility {
    /// Undoable by the named compensation route.
    Reversible { compensation: Identifier },
    /// Not undoable. Saying so is more useful than a compensation route that
    /// does not really compensate.
    Irreversible,
}

/// A declared effect.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Effect {
    pub id: Identifier,
    pub kind: EffectKind,
    /// What the effect touches, in a form a policy can match against: a host, a
    /// repository, a memory space, a queue.
    pub target: String,
    /// The granted capability that authorises this effect.
    pub capability: Identifier,
    pub idempotency: Idempotency,
    pub reversibility: Reversibility,
}

impl Effect {
    /// Checks the parts of an effect that are decidable from the effect alone.
    ///
    /// # Errors
    ///
    /// Returns [`EffectError`] when a keyed effect names no key source or a
    /// reversible effect names no compensation route.
    pub fn check(&self) -> Result<(), EffectError> {
        if let Idempotency::Keyed { key_source } = &self.idempotency
            && key_source.trim().is_empty()
        {
            return Err(EffectError::MissingIdempotencyKey {
                effect: self.id.clone(),
            });
        }
        if let Reversibility::Reversible { compensation } = &self.reversibility
            && compensation.as_str().is_empty()
        {
            return Err(EffectError::MissingCompensation {
                effect: self.id.clone(),
            });
        }
        Ok(())
    }
}

/// A boundary whose crossing a policy may gate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProtectedBoundary {
    pub id: Identifier,
    /// The node that would cross it.
    pub node: Identifier,
    /// What crossing means here.
    pub crossing: Crossing,
    /// Plain text for the operator who will read the denial.
    pub description: String,
}

/// What a protected boundary protects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Crossing {
    /// Performing a declared effect.
    Effect { effect: Identifier },
    /// Handing a value to something that requires a stronger trust class than
    /// the value carries by default: a governed memory space, a protected
    /// downstream node, an external consumer.
    TrustTransition { into: Identifier },
}

/// Why a protected boundary declaration was refused.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BoundaryError {
    #[error(
        "protected boundary `{boundary}` names node `{node}`, which the definition does not declare"
    )]
    UnknownNode {
        boundary: Identifier,
        node: Identifier,
    },
    #[error(
        "protected boundary `{boundary}` protects effect `{effect}`, which node `{node}` does not \
         declare; a boundary over nothing protects nothing"
    )]
    NoSuchEffect {
        boundary: Identifier,
        node: Identifier,
        effect: Identifier,
    },
    #[error("protected boundary `{boundary}` is declared twice")]
    Duplicate { boundary: Identifier },
}
