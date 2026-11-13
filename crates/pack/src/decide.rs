//! Running a pack's validators over a change, and re-running them from a bundle.
//!
//! [`decide`] hands the inputs to every validator that `decides` an obligation,
//! and returns what a certificate needs: every obligation the pack states, in
//! the state the runs left it; the record of each run; and the inputs and each
//! run's outputs as evidence, with their bytes, so a bundle carries the failing
//! compiler output or test log a `does_not_hold` rests on. An obligation no
//! validator decides, a review, stays residual and says whose review it waits
//! for. A validator that could not run decides nothing, and says why.
//!
//! [`replay_tools`] is the other half (ADR 0021 §4): for a certificate and the
//! bundle of its evidence, it runs each replayable tool again in the
//! environment its record pins, with the inputs the certificate cites, and
//! compares what it produced with what the record says.

use capsulet_ir::Identifier;
use capsulet_ir::correctness::certificate::{Certificate, VerifierRecord, VerifierTrust};
use capsulet_ir::correctness::evidence::{EvidenceRef, RecordedTime};
use capsulet_ir::correctness::obligation::{DischargeState, Obligation};
use capsulet_ir::correctness::proposal::Producer;
use capsulet_kernel::{EvidenceMap, EvidenceSource};
use capsulet_verifier::{Input, ReRun, Requested, Transport, VerifierFailure, rerun, verify};

use crate::{Pack, PackError, obligations, registration_of};

/// The evidence id an input is cited under: `inputs/` and its path.
pub const INPUT_PREFIX: &str = "inputs/";

/// What one validator's run came to.
#[derive(Debug, Clone)]
pub struct Run {
    /// The registration, by its path in the pack.
    pub registration: String,
    /// The obligations it was asked to decide.
    pub decides: Vec<String>,
    /// Its record, or why it decided nothing.
    pub outcome: Result<VerifierRecord, VerifierFailure>,
}

/// Every obligation a pack states, decided as far as its validators can.
#[derive(Debug, Clone)]
pub struct Decision {
    /// In the order the pack's obligations files state them.
    pub obligations: Vec<Obligation>,
    /// One record per validator that ran.
    pub verifiers: Vec<VerifierRecord>,
    /// The inputs, then each run's outputs, as a certificate cites them.
    pub evidence: Vec<EvidenceRef>,
    /// The bytes of that evidence.
    pub blobs: EvidenceMap,
    pub runs: Vec<Run>,
}

/// Runs each validator of `pack` that decides something over `inputs`.
///
/// `producer` is who supplied the inputs, as their evidence records it, and `at`
/// is when they and the outputs were captured.
///
/// # Errors
///
/// Returns [`PackError`] when the pack's obligations or a registration do not
/// read, or an input's path is not an evidence id. A validator that fails to
/// run is not an error: its obligations stay residual, saying why.
pub fn decide(
    pack: &Pack,
    inputs: &[Input],
    transport: &impl Transport,
    producer: &Producer,
    at: RecordedTime,
) -> Result<Decision, PackError> {
    let contracts = obligations(pack)?;
    let mut blobs = EvidenceMap::new();
    let mut evidence = Vec::new();
    for input in inputs {
        let id = Identifier::parse(format!("{INPUT_PREFIX}{}", input.name)).map_err(|_| {
            PackError::Unreadable(format!("input `{}` cannot be cited", input.name))
        })?;
        let content = blobs.insert(input.bytes.clone());
        evidence.push(EvidenceRef {
            id,
            content,
            media_type: input.media_type.clone(),
            byte_length: input.bytes.len() as u64,
            producer: producer.clone(),
            captured_at: at,
        });
    }

    let mut decided: Vec<Obligation> = Vec::new();
    let mut verifiers = Vec::new();
    let mut runs = Vec::new();
    for entry in &pack.manifest.validators {
        if entry.decides.is_empty() {
            continue;
        }
        let registration = registration_of(pack, entry)?;
        let requested = requested(&contracts, &entry.decides);
        let outcome = verify(&registration, transport, &requested, inputs);
        match &outcome {
            Ok(run) => {
                decided.extend(run.obligations(&requested));
                verifiers.push(run.record.clone());
                for output in &run.outputs {
                    blobs.insert(output.bytes.clone());
                }
                evidence.extend(run.evidence(at));
            }
            Err(failure) => {
                for wanted in &requested {
                    decided.push(Obligation {
                        statement: wanted.statement.clone(),
                        contract: wanted.contract.clone(),
                        state: DischargeState::Residual {
                            rationale: format!(
                                "{} could not decide it: {failure} ({})",
                                registration.name,
                                failure.code()
                            ),
                            evidence: Vec::new(),
                        },
                    });
                }
            }
        }
        runs.push(Run {
            registration: entry.registration.clone(),
            decides: entry.decides.clone(),
            outcome: outcome.map(|run| run.record),
        });
    }

    let mut ordered = Vec::new();
    for contract in &contracts {
        for statement in &contract.obligations {
            let found = decided.iter().position(|obligation| {
                obligation.contract == contract.id && obligation.statement.id == statement.id
            });
            ordered.push(found.map_or_else(
                || Obligation {
                    statement: statement.clone(),
                    contract: contract.id.clone(),
                    state: DischargeState::Residual {
                        rationale: statement.authority.as_ref().map_or_else(
                            || "no validator in the pack decides it".to_string(),
                            |authority| {
                                format!(
                                    "no validator decides it; it waits for a review under \
                                     {authority}"
                                )
                            },
                        ),
                        evidence: Vec::new(),
                    },
                },
                |at| decided.remove(at),
            ));
        }
    }
    Ok(Decision {
        obligations: ordered,
        verifiers,
        evidence,
        blobs,
        runs,
    })
}

/// The pack's statements of `ids`, in that order.
fn requested(contracts: &[capsulet_ir::Contract], ids: &[String]) -> Vec<Requested> {
    ids.iter()
        .filter_map(|id| {
            contracts.iter().find_map(|contract| {
                contract
                    .obligations
                    .iter()
                    .find(|statement| statement.id.as_str() == id)
                    .map(|statement| Requested {
                        contract: contract.id.clone(),
                        statement: statement.clone(),
                    })
            })
        })
        .collect()
}

/// What running one recorded tool again found.
#[derive(Debug, Clone)]
pub struct ToolReplay {
    /// The tool, as its record names it.
    pub validator: String,
    /// How the second run compares with the record, or why it could not run.
    pub outcome: Result<ReRun, VerifierFailure>,
}

impl ToolReplay {
    /// Whether the tool ran again and reached its record exactly.
    #[must_use]
    pub fn agrees(&self) -> bool {
        self.outcome.as_ref().is_ok_and(ReRun::agrees)
    }
}

/// Runs every replayable tool `certificate` records again, from `evidence`.
///
/// The inputs are the evidence cited under [`INPUT_PREFIX`], in the order the
/// certificate cites them, which is the order the tools read them in. A tool's
/// registration is the one in `pack` with the record's name and version; what
/// it is asked is what it decides, as the certificate states each obligation.
/// A model's record is not re-run: a model's word is not re-derivable.
///
/// # Errors
///
/// Returns [`PackError::Unreadable`] when an input the certificate cites is
/// not in `evidence`, or a recorded tool is not in `pack`.
pub fn replay_tools(
    pack: &Pack,
    certificate: &Certificate,
    evidence: &impl EvidenceSource,
    transport: &impl Transport,
) -> Result<Vec<ToolReplay>, PackError> {
    let body = certificate.body();
    let mut inputs = Vec::new();
    for reference in &body.evidence {
        let Some(name) = reference.id.as_str().strip_prefix(INPUT_PREFIX) else {
            continue;
        };
        let bytes = evidence.bytes(&reference.content).ok_or_else(|| {
            PackError::Unreadable(format!(
                "input `{name}` ({}) is not in the bundle",
                reference.content
            ))
        })?;
        inputs.push(Input {
            name: name.to_string(),
            bytes: bytes.to_vec(),
            media_type: reference.media_type.clone(),
        });
    }
    let mut replays = Vec::new();
    for record in &body.verifiers {
        if !matches!(record.trust, VerifierTrust::DeclaredOracle { .. }) {
            continue;
        }
        let mut found = None;
        for entry in &pack.manifest.validators {
            let registration = registration_of(pack, entry)?;
            if registration.name == record.identity.name.as_str()
                && registration.version == record.identity.version
            {
                found = Some((entry, registration));
                break;
            }
        }
        let (entry, registration) = found.ok_or_else(|| {
            PackError::Unreadable(format!(
                "the certificate records {} {}, which this pack does not register",
                record.identity.name, record.identity.version
            ))
        })?;
        let requested: Vec<Requested> = entry
            .decides
            .iter()
            .filter_map(|id| {
                body.obligations
                    .iter()
                    .find(|obligation| obligation.statement.id.as_str() == id)
                    .map(|obligation| Requested {
                        contract: obligation.contract.clone(),
                        statement: obligation.statement.clone(),
                    })
            })
            .collect();
        replays.push(ToolReplay {
            validator: format!("{} {}", record.identity.name, record.identity.version),
            outcome: rerun(&registration, transport, &requested, &inputs, record),
        });
    }
    Ok(replays)
}
