//! A certificate assembled from what actually happened.
//!
//! The inputs to M2's `certify` come from the run's log and nowhere else. That
//! is the whole reason this function exists rather than the worker building an
//! assembly as it goes: a worker that accumulates certificate inputs in memory
//! produces a different certificate after a crash than before one, and the
//! difference is invisible until somebody replays it.
//!
//! Everything here is therefore a fold. A run killed at any point and resumed
//! produces the same log as one that ran straight through — modulo the
//! recovery events that genuinely happened — and the same log produces the same
//! certificate.
//!
//! What this does *not* do is invent anything. Loop outcomes come from the
//! iteration records the log carries; obligations and verifier records come
//! from the caller, because a verifier protocol arrives in M4 and pretending to
//! have one now would put a claim in a certificate that nothing checked.

use capsulet_ir::admission::AdmissionRecord;
use capsulet_ir::correctness::certificate::{Subject, VerifierRecord};
use capsulet_ir::correctness::evidence::EvidenceRef;
use capsulet_ir::correctness::obligation::Obligation;
use capsulet_ir::definition::Definition;
use capsulet_ir::digest::Digest;
use capsulet_ir::id::Identifier;
use capsulet_ir::loop_region::{IterationRecord, LoopOutcome, StopReason};

use crate::event::{RecordedEvent, RunEvent};
use crate::state::RunState;

/// Everything the log can say about a run, in the shape a certificate wants.
///
/// Deliberately not a `Certificate`: sealing one lives in `capsulet-kernel`,
/// and this crate stays a pure fold with no opinion about kernel versions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RunEvidence {
    pub subject: Subject,
    pub loops: Vec<LoopOutcome>,
    /// The definition's own admission record, carried through unchanged.
    pub admission: AdmissionRecord,
}

/// Reads a run's log into the inputs a certificate is assembled from.
///
/// `outputs` are taken from the fold rather than from whatever the caller
/// remembers producing, so a resumed run and an uninterrupted one agree.
#[must_use]
pub fn evidence_of(
    definition: &Definition,
    admission: AdmissionRecord,
    run: &Identifier,
    state: &RunState,
    events: &[RecordedEvent],
    inputs: Vec<Digest>,
) -> RunEvidence {
    RunEvidence {
        subject: Subject {
            definition: *state.definition(),
            definition_version: definition.version.clone(),
            run: Some(run.clone()),
            inputs,
            // Sorted and de-duplicated by the fold's own ordering, so two runs
            // that produced the same values produce the same subject.
            outputs: state.outputs().into_values().collect(),
        },
        loops: loop_outcomes(events, state),
        admission,
    }
}

/// Every loop that stopped, with its iterations and the reason it stopped.
///
/// A loop still running is left out. [`LoopOutcome`] requires a stop reason and
/// there is not one yet, and every value that could be put there would be a
/// false statement about the run — `ConditionFalse` says it finished its work,
/// `BudgetExhausted` says it ran out. A certificate assembled mid-loop simply
/// does not describe that loop, which is the truthful thing for it to do.
#[must_use]
pub fn loop_outcomes(events: &[RecordedEvent], state: &RunState) -> Vec<LoopOutcome> {
    let mut regions: Vec<Identifier> = Vec::new();
    let mut iterations: Vec<(Identifier, IterationRecord)> = Vec::new();

    for recorded in events {
        match &recorded.event {
            RunEvent::IterationStarted { region, .. } | RunEvent::LoopStopped { region, .. } => {
                if !regions.contains(region) {
                    regions.push(region.clone());
                }
            }
            RunEvent::IterationFinished { region, record } => {
                if !regions.contains(region) {
                    regions.push(region.clone());
                }
                iterations.push((region.clone(), (**record).clone()));
            }
            _ => {}
        }
    }

    regions
        .into_iter()
        .filter_map(|region| {
            let stopped = state.loop_progress(&region).stopped?;
            Some(LoopOutcome {
                iterations: iterations
                    .iter()
                    .filter(|(each, _)| each == &region)
                    .map(|(_, record)| record.clone())
                    .collect(),
                region,
                stopped,
            })
        })
        .collect()
}

/// The stop reasons a run recorded, in the order it recorded them.
///
/// Useful on its own: "why did the loops stop" is the question a reviewer asks
/// first, and reading it out of the log is cheaper than opening a certificate.
#[must_use]
pub fn stop_reasons(events: &[RecordedEvent]) -> Vec<(Identifier, StopReason)> {
    events
        .iter()
        .filter_map(|recorded| match &recorded.event {
            RunEvent::LoopStopped { region, reason } => Some((region.clone(), reason.clone())),
            _ => None,
        })
        .collect()
}

/// Obligations and evidence the caller has gathered, checked against the run.
///
/// This crate does not produce either — a verifier protocol arrives in M4 —
/// but it can say when a caller has handed over an obligation that rests on
/// evidence the run never carried, which is the mistake that would otherwise
/// reach a sealed certificate.
///
/// # Errors
///
/// Returns [`CertifyError::UnknownEvidence`] naming the first obligation whose
/// evidence is not among the evidence supplied.
pub fn check_obligations(
    obligations: &[Obligation],
    evidence: &[EvidenceRef],
    verifiers: &[VerifierRecord],
) -> Result<(), CertifyError> {
    let carried: Vec<Digest> = evidence
        .iter()
        .map(|each| each.content)
        .chain(verifiers.iter().flat_map(|each| each.outputs.clone()))
        .collect();

    for obligation in obligations {
        for digest in obligation.state.evidence() {
            if !carried.contains(digest) {
                return Err(CertifyError::UnknownEvidence {
                    obligation: obligation.statement.id.clone(),
                    digest: digest.to_string(),
                });
            }
        }
    }
    Ok(())
}

/// Why a certificate could not be assembled from a run.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CertifyError {
    #[error(
        "obligation `{obligation}` rests on evidence `{digest}`, which this run does not carry"
    )]
    UnknownEvidence {
        obligation: Identifier,
        digest: String,
    },
}
