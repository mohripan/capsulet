//! Claim, perform, finalize.
//!
//! A protected effect is written down before it is attempted and written down
//! again once it is known to have happened. The gap between those two records
//! is the only window in which a crash leaves a question, and the whole design
//! is about making that question answerable: a claim with no finalization means
//! *nobody knows*, and what happens next follows from the idempotency the IR
//! declared, never from a guess.
//!
//! The types here make the order hard to get wrong. A finalization can only be
//! built from an attempt, and an attempt can only be built by claiming, so
//! "finalize an effect that was never claimed" is not an expression this crate
//! lets you write.
//!
//! Nothing here performs anything. It produces the events a worker appends
//! around the work it does, which keeps this crate as replayable as the rest.

use capsulet_ir::definition::Definition;
use capsulet_ir::digest::Digest;
use capsulet_ir::effect::{Effect, Idempotency};
use capsulet_ir::id::Identifier;
use thiserror::Error;

use crate::event::RunEvent;
use crate::state::OutstandingEffect;

/// What a far side can deduplicate on, and this runtime can supply.
///
/// A closed set on purpose. The IR declares `key_source` as free text, because
/// what a far side keys on is its business; but a runtime that met an unknown
/// source and invented a key would be doing exactly what the declaration exists
/// to prevent. Unknown sources are refused, up front, by [`check_effect_keys`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeySource {
    /// One key for the whole run, so the far side sees at most one such effect
    /// however often the run retries or restarts. The strongest of the three,
    /// and the right default for "open the pull request once".
    Run,
    /// One key per node, for a graph that performs the same effect at more than
    /// one place and means them as different effects.
    RunAndNode,
    /// One key per attempt. Weaker: a retry under a new attempt number is a new
    /// key and therefore a new effect on the far side, so this only suits an
    /// effect where repeating is acceptable and the key is there to collapse
    /// exact duplicates.
    RunNodeAndAttempt,
}

impl KeySource {
    /// Reads the source an effect declared.
    ///
    /// # Errors
    ///
    /// Returns [`EffectError::UnknownKeySource`] for anything outside the set
    /// above.
    pub fn parse(effect: &Identifier, declared: &str) -> Result<Self, EffectError> {
        match declared.trim() {
            "run_id" => Ok(Self::Run),
            "run_id+node_id" => Ok(Self::RunAndNode),
            "run_id+node_id+attempt" => Ok(Self::RunNodeAndAttempt),
            other => Err(EffectError::UnknownKeySource {
                effect: effect.clone(),
                declared: other.to_string(),
            }),
        }
    }
}

/// Where an effect is being performed from.
#[derive(Debug, Clone, Copy)]
pub struct EffectContext<'a> {
    pub run: &'a str,
    pub node: &'a Identifier,
    pub attempt: u32,
}

/// One claimed attempt at one effect.
///
/// Holds the idempotency key that was handed to the far side, because a retry
/// has to present *that* key rather than one derived again later.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EffectAttempt {
    node: Identifier,
    effect: Identifier,
    attempt: u32,
    key: Option<String>,
}

impl EffectAttempt {
    /// Claims an attempt, deriving the key the effect declared it needs.
    ///
    /// # Errors
    ///
    /// Returns [`EffectError::UnknownKeySource`] when the effect declares a key
    /// source this runtime cannot supply. Performing it anyway would mean
    /// telling a far side "deduplicate on this" while supplying something it
    /// never asked for.
    pub fn claim(declared: &Effect, context: &EffectContext<'_>) -> Result<Self, EffectError> {
        let key = match &declared.idempotency {
            Idempotency::Idempotent | Idempotency::NonIdempotent => None,
            Idempotency::Keyed { key_source } => {
                let source = KeySource::parse(&declared.id, key_source)?;
                Some(key_for(source, declared, context))
            }
        };

        Ok(Self {
            node: context.node.clone(),
            effect: declared.id.clone(),
            attempt: context.attempt,
            key,
        })
    }

    /// Rebuilds the attempt a dead worker had made, from the claim it left.
    ///
    /// The key comes back from the log rather than being derived again. If key
    /// derivation ever changes, a retry must still present the key the far side
    /// already saw — otherwise the retry is a second effect wearing the first
    /// one's name, which is the failure this whole protocol exists to prevent.
    #[must_use]
    pub fn recovered(claim: &OutstandingEffect, attempt: u32) -> Self {
        Self {
            node: claim.node.clone(),
            effect: claim.effect.clone(),
            attempt,
            key: claim.key.clone(),
        }
    }

    #[must_use]
    pub fn node(&self) -> &Identifier {
        &self.node
    }

    #[must_use]
    pub fn effect(&self) -> &Identifier {
        &self.effect
    }

    #[must_use]
    pub const fn attempt(&self) -> u32 {
        self.attempt
    }

    /// The key handed to the far side, where the effect declared one.
    #[must_use]
    pub fn key(&self) -> Option<&str> {
        self.key.as_deref()
    }

    /// The event to append *before* performing.
    #[must_use]
    pub fn claimed(&self) -> RunEvent {
        RunEvent::EffectClaimed {
            node: self.node.clone(),
            effect: self.effect.clone(),
            attempt: self.attempt,
            key: self.key.clone(),
        }
    }

    /// The event to append once the effect is known to have happened.
    #[must_use]
    pub fn finalized(&self, receipt: Digest) -> RunEvent {
        RunEvent::EffectFinalized {
            node: self.node.clone(),
            effect: self.effect.clone(),
            attempt: self.attempt,
            receipt,
        }
    }

    /// The event to append when nobody can say whether it happened.
    ///
    /// Recording the doubt is the point. A run that stops here says which
    /// effect is in question, so a person can look and say what a machine
    /// cannot.
    #[must_use]
    pub fn uncertain(&self) -> RunEvent {
        RunEvent::EffectUncertain {
            node: self.node.clone(),
            effect: self.effect.clone(),
            attempt: self.attempt,
        }
    }
}

/// Checks every keyed effect in a definition before the run starts.
///
/// An effect that declares a key source this runtime cannot supply is a run
/// that will fail — the only question is when. Finding out before anything has
/// happened is worth more than finding out with half the graph executed.
///
/// # Errors
///
/// Returns [`EffectError::UnknownKeySource`] for the first effect this runtime
/// could not supply a key for.
pub fn check_effect_keys(definition: &Definition) -> Result<(), EffectError> {
    for node in definition.graph.nodes() {
        for effect in &node.effects {
            if let Idempotency::Keyed { key_source } = &effect.idempotency {
                KeySource::parse(&effect.id, key_source)?;
            }
        }
    }
    Ok(())
}

/// The key value itself.
///
/// Readable rather than hashed, because the person reading the far side's
/// deduplication log is trying to match it to a run, and a digest tells them
/// nothing they can act on.
fn key_for(source: KeySource, declared: &Effect, context: &EffectContext<'_>) -> String {
    let run = context.run;
    let node = context.node.as_str();
    let effect = declared.id.as_str();
    match source {
        KeySource::Run => format!("capsulet:{run}:{effect}"),
        KeySource::RunAndNode => format!("capsulet:{run}:{node}:{effect}"),
        KeySource::RunNodeAndAttempt => {
            format!("capsulet:{run}:{node}:{effect}:{}", context.attempt)
        }
    }
}

/// Why an effect could not be claimed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum EffectError {
    #[error(
        "effect {effect} deduplicates on {declared:?}, which this runtime cannot supply; \
         performing it would mean sending a key the far side did not ask for"
    )]
    UnknownKeySource {
        effect: Identifier,
        declared: String,
    },
}
