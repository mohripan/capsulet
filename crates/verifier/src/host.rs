//! Running a validator, and turning what it said into what a certificate says.
//!
//! The host does the bookkeeping, not the validator (ADR 0021 §4). It writes the
//! inputs and digests them, runs the validator under its limits, checks the
//! inputs are still the bytes it wrote, reads one bounded response, holds every
//! requested obligation to exactly one result, digests whatever the validator
//! left in its outputs, and builds the [`VerifierRecord`]. Every way that can go
//! wrong is a typed [`VerifierFailure`] owned by the verifier, never a verdict.

use std::collections::BTreeSet;
use std::fs;
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use capsulet_ir::correctness::Identity;
use capsulet_ir::correctness::certificate::{CheckerVerdict, VerifierRecord, VerifierTrust};
use capsulet_ir::correctness::evidence::{EvidenceRef, RecordedTime};
use capsulet_ir::correctness::obligation::{
    DischargeBasis, DischargeState, Obligation, ObligationStatement, RepairOwner,
};
use capsulet_ir::correctness::proposal::{Producer, ProducerKind};
use capsulet_ir::{Digest, Identifier};
use thiserror::Error;

use crate::protocol::{
    InputFile, MAX_RESPONSE_BYTES, ObligationResult, Outcome, PROTOCOL, RequestedObligation,
    VerifyRequest, VerifyResponse,
};
use crate::registration::{Environment, RegisteredBasis, Registration, RegistrationError};

/// A file the host hands a validator.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Input {
    pub name: String,
    pub bytes: Vec<u8>,
    pub media_type: String,
}

/// An obligation to decide, and the contract it belongs to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Requested {
    pub contract: Identifier,
    pub statement: ObligationStatement,
}

/// What a transport saw of one run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RawRun {
    /// The exit code, or `None` when it was stopped or killed by a signal.
    pub exit: Option<i32>,
    pub timed_out: bool,
    pub stdout: Vec<u8>,
    /// The end of what it wrote to standard error, for the failure message.
    pub stderr_tail: String,
}

/// How a validator is started.
pub trait Transport {
    /// Runs the validator in `workdir`, which holds `inputs/` and `outputs/`,
    /// with `request` on its standard input.
    ///
    /// # Errors
    ///
    /// Returns the reason it could not be started or waited for.
    fn run(
        &self,
        registration: &Registration,
        workdir: &Path,
        request: &[u8],
    ) -> Result<RawRun, String>;
}

/// Why a validator's run decided nothing.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum VerifierFailure {
    #[error("the registration cannot be run: {0}")]
    Registration(#[from] RegistrationError),
    #[error("the validator's environment does not match its transport: {detail}")]
    WrongTransport { detail: &'static str },
    #[error("input `{name}` is not a plain file name")]
    BadInput { name: String },
    #[error("the validator could not be run: {detail}")]
    Transport { detail: String },
    #[error("the validator ran past its {limit_ms} ms limit and was stopped")]
    TimedOut { limit_ms: u64 },
    #[error("the validator exited with {code:?}: {stderr}")]
    Crashed { code: Option<i32>, stderr: String },
    #[error("the response is larger than the {limit} bytes a host reads")]
    ResponseTooLarge { limit: usize },
    #[error("the response is not a protocol response: {detail}")]
    MalformedResponse { detail: String },
    #[error("the response speaks `{found}`, not `{PROTOCOL}`")]
    ProtocolMismatch { found: String },
    #[error("the response has no result for obligation `{obligation}`")]
    MissingResult { obligation: String },
    #[error("the response decides `{obligation}`, which it was not asked about")]
    UnexpectedResult { obligation: String },
    #[error("the response decides obligation `{obligation}` more than once")]
    DuplicateResult { obligation: String },
    #[error("input `{name}` changed while the validator ran")]
    InputsModified { name: String },
    #[error("the outputs hold {found} bytes, beyond the {limit} the registration allows")]
    OutputsTooLarge { limit: u64, found: u64 },
    #[error("the run's working directory failed: {detail}")]
    Workspace { detail: String },
}

impl VerifierFailure {
    /// Who repairs it: always the verifier, because a validator that cannot be
    /// run or cannot be believed is the verifier's problem, not the proposer's.
    #[must_use]
    pub const fn owner(&self) -> RepairOwner {
        RepairOwner::Verifier
    }

    /// A short name clients can route on.
    #[must_use]
    pub const fn code(&self) -> &'static str {
        match self {
            Self::Registration(_) => "registration_refused",
            Self::WrongTransport { .. } => "wrong_transport",
            Self::BadInput { .. } => "bad_input",
            Self::Transport { .. } => "transport_failed",
            Self::TimedOut { .. } => "timed_out",
            Self::Crashed { .. } => "crashed",
            Self::ResponseTooLarge { .. } => "response_too_large",
            Self::MalformedResponse { .. } => "malformed_response",
            Self::ProtocolMismatch { .. } => "protocol_mismatch",
            Self::MissingResult { .. } => "missing_result",
            Self::UnexpectedResult { .. } => "unexpected_result",
            Self::DuplicateResult { .. } => "duplicate_result",
            Self::InputsModified { .. } => "inputs_modified",
            Self::OutputsTooLarge { .. } => "outputs_too_large",
            Self::Workspace { .. } => "workspace_failed",
        }
    }
}

/// A file the validator left in its outputs, as the host digested it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Output {
    /// Its path under the outputs directory, with `/` separators.
    pub name: String,
    pub digest: Digest,
    pub byte_length: u64,
}

/// One validator's run, as a certificate will record it.
#[derive(Debug, Clone, PartialEq)]
pub struct Verification {
    pub record: VerifierRecord,
    /// Whether it ran isolated. An unisolated run decides nothing.
    pub isolated: bool,
    pub basis: RegisteredBasis,
    pub results: Vec<ObligationResult>,
    pub outputs: Vec<Output>,
}

impl Verification {
    /// The requested obligations, in the state this run leaves them.
    ///
    /// - A replayable tool's `holds` discharges, on basis `replayable_tool`,
    ///   resting on its outputs; its `does_not_hold` fails the obligation.
    /// - A model's `holds` endorses and its `does_not_hold` disputes. Neither
    ///   decides anything.
    /// - `cannot_decide` stays residual.
    /// - Anything an unisolated run said stays residual, saying so.
    #[must_use]
    pub fn obligations(&self, requested: &[Requested]) -> Vec<Obligation> {
        let by = &self.record.identity.name;
        let evidence: Vec<Digest> = self.outputs.iter().map(|output| output.digest).collect();
        requested
            .iter()
            .map(|wanted| {
                let result = self
                    .results
                    .iter()
                    .find(|result| result.obligation == wanted.statement.id.as_str());
                let state = match result {
                    None => DischargeState::Residual {
                        rationale: "the validator returned no result for it".to_string(),
                        evidence: evidence.clone(),
                    },
                    Some(result) if !self.isolated => DischargeState::Residual {
                        rationale: format!(
                            "{} in an unisolated development run, which decides nothing: {}",
                            outcome_words(result.outcome),
                            result.reason
                        ),
                        evidence: evidence.clone(),
                    },
                    Some(result) => self.state_of(result, wanted, by, &evidence),
                };
                Obligation {
                    statement: wanted.statement.clone(),
                    contract: wanted.contract.clone(),
                    state,
                }
            })
            .collect()
    }

    fn state_of(
        &self,
        result: &ObligationResult,
        wanted: &Requested,
        by: &Identifier,
        evidence: &[Digest],
    ) -> DischargeState {
        let model = matches!(self.basis, RegisteredBasis::ModelJudgment { .. });
        match (result.outcome, model) {
            (Outcome::Holds, false) => DischargeState::Discharged {
                by: by.clone(),
                evidence: evidence.to_vec(),
                basis: DischargeBasis::ReplayableTool,
            },
            (Outcome::Holds, true) => DischargeState::Endorsed {
                by: by.clone(),
                evidence: evidence.to_vec(),
            },
            (Outcome::DoesNotHold, false) => DischargeState::Failed {
                reason: result.reason.clone(),
                owner: wanted.statement.owner,
            },
            (Outcome::DoesNotHold, true) => DischargeState::Disputed {
                by: by.clone(),
                reason: result.reason.clone(),
                evidence: evidence.to_vec(),
            },
            (Outcome::CannotDecide, _) => DischargeState::Residual {
                rationale: result.reason.clone(),
                evidence: evidence.to_vec(),
            },
        }
    }

    /// The outputs as evidence a certificate carries, captured at `at`.
    #[must_use]
    pub fn evidence(&self, at: RecordedTime) -> Vec<EvidenceRef> {
        let kind = match self.basis {
            RegisteredBasis::ReplayableTool => ProducerKind::Tool,
            RegisteredBasis::ModelJudgment { .. } => ProducerKind::Model,
        };
        self.outputs
            .iter()
            .filter_map(|output| {
                Some(EvidenceRef {
                    id: Identifier::parse(format!(
                        "{}/{}",
                        self.record.identity.name.as_str(),
                        output.name
                    ))
                    .ok()?,
                    content: output.digest,
                    media_type: "application/octet-stream".to_string(),
                    byte_length: output.byte_length,
                    producer: Producer {
                        kind,
                        identity: self.record.identity.clone(),
                    },
                    captured_at: at,
                })
            })
            .collect()
    }
}

const fn outcome_words(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Holds => "held",
        Outcome::DoesNotHold => "did not hold",
        Outcome::CannotDecide => "could not be decided",
    }
}

/// Runs one validator over `requested`, with `inputs`.
///
/// # Errors
///
/// Returns a [`VerifierFailure`] for every way a run can decide nothing: a
/// registration that cannot run, an unsafe input name, a validator that cannot
/// start, crashes, runs too long, changes its inputs, leaves too much output, or
/// answers with something other than one result per requested obligation in this
/// protocol.
pub fn verify(
    registration: &Registration,
    transport: &impl Transport,
    requested: &[Requested],
    inputs: &[Input],
) -> Result<Verification, VerifierFailure> {
    registration.validate()?;
    let identity = Identity::new(
        Identifier::parse(registration.name.clone()).map_err(|_| {
            RegistrationError::Incomplete {
                name: registration.name.clone(),
                detail: "a name that is an identifier",
            }
        })?,
        registration.version.clone(),
    );
    let workspace = Workspace::new(inputs)?;
    let request = VerifyRequest {
        protocol: PROTOCOL.to_string(),
        obligations: requested
            .iter()
            .map(|wanted| RequestedObligation {
                id: wanted.statement.id.as_str().to_string(),
                contract: wanted.contract.as_str().to_string(),
                statement: wanted.statement.statement.clone(),
            })
            .collect(),
        parameters: registration.parameters.clone(),
        inputs: workspace.files.clone(),
    };
    let bytes = serde_json::to_vec(&request).map_err(|error| VerifierFailure::Transport {
        detail: error.to_string(),
    })?;
    let run = transport
        .run(registration, workspace.root(), &bytes)
        .map_err(|detail| VerifierFailure::Transport { detail })?;

    if run.timed_out {
        return Err(VerifierFailure::TimedOut {
            limit_ms: registration.limits.timeout_ms,
        });
    }
    workspace.check_inputs()?;
    if run.exit != Some(0) {
        return Err(VerifierFailure::Crashed {
            code: run.exit,
            stderr: run.stderr_tail,
        });
    }
    let response = read_response(&run.stdout)?;
    check_results(requested, &response.results)?;
    let outputs = workspace.outputs(registration.limits.max_output_bytes)?;

    let isolated = registration.environment.is_isolated();
    let record = VerifierRecord {
        identity,
        environment: environment_digest(&registration.environment),
        inputs: workspace.digests(),
        outputs: outputs.iter().map(|output| output.digest).collect(),
        trust: trust_of(registration, isolated),
        verdict: verdict_of(&response.results),
    };
    Ok(Verification {
        record,
        isolated,
        basis: registration.basis.clone(),
        results: response.results,
        outputs,
    })
}

fn read_response(stdout: &[u8]) -> Result<VerifyResponse, VerifierFailure> {
    #[derive(serde::Deserialize)]
    struct Header {
        protocol: String,
    }
    if stdout.len() > MAX_RESPONSE_BYTES {
        return Err(VerifierFailure::ResponseTooLarge {
            limit: MAX_RESPONSE_BYTES,
        });
    }
    let malformed = |error: serde_json::Error| VerifierFailure::MalformedResponse {
        detail: error.to_string(),
    };
    let header: Header = serde_json::from_slice(stdout).map_err(malformed)?;
    if header.protocol != PROTOCOL {
        return Err(VerifierFailure::ProtocolMismatch {
            found: header.protocol,
        });
    }
    serde_json::from_slice(stdout).map_err(malformed)
}

fn check_results(
    requested: &[Requested],
    results: &[ObligationResult],
) -> Result<(), VerifierFailure> {
    let asked: BTreeSet<&str> = requested
        .iter()
        .map(|wanted| wanted.statement.id.as_str())
        .collect();
    let mut seen = BTreeSet::new();
    for result in results {
        if !asked.contains(result.obligation.as_str()) {
            return Err(VerifierFailure::UnexpectedResult {
                obligation: result.obligation.clone(),
            });
        }
        if !seen.insert(result.obligation.as_str()) {
            return Err(VerifierFailure::DuplicateResult {
                obligation: result.obligation.clone(),
            });
        }
    }
    if let Some(missing) = asked.iter().find(|id| !seen.contains(*id)) {
        return Err(VerifierFailure::MissingResult {
            obligation: (*missing).to_string(),
        });
    }
    Ok(())
}

/// What one run concluded, over all its results.
fn verdict_of(results: &[ObligationResult]) -> CheckerVerdict {
    if results
        .iter()
        .any(|result| result.outcome == Outcome::DoesNotHold)
    {
        CheckerVerdict::Rejected
    } else if results
        .iter()
        .any(|result| result.outcome == Outcome::CannotDecide)
    {
        CheckerVerdict::Conditional
    } else {
        CheckerVerdict::Accepted
    }
}

fn trust_of(registration: &Registration, isolated: bool) -> VerifierTrust {
    match &registration.basis {
        RegisteredBasis::ModelJudgment {
            family,
            independent_of_proposer,
            calibration,
        } => VerifierTrust::ModelJudgment {
            family: family.clone(),
            independent_of_proposer: *independent_of_proposer,
            calibration: calibration
                .as_ref()
                .and_then(|calibration| Identifier::parse(calibration.clone()).ok()),
        },
        RegisteredBasis::ReplayableTool if isolated => VerifierTrust::DeclaredOracle {
            rationale: format!(
                "{} {} ran isolated, with no network and no model, in an environment pinned by \
                 digest, so a replay can run it again there",
                registration.name, registration.version
            ),
        },
        RegisteredBasis::ReplayableTool => VerifierTrust::DeclaredOracle {
            rationale: format!(
                "{} {} ran unisolated in the development harness; what it said decides nothing",
                registration.name, registration.version
            ),
        },
    }
}

/// The environment, pinned: an image's own digest, or a digest naming the
/// program a development run used.
fn environment_digest(environment: &Environment) -> Digest {
    match environment {
        Environment::Container { image } => image
            .rsplit_once('@')
            .and_then(|(_, digest)| digest.parse().ok())
            .unwrap_or_else(|| Digest::of(image.as_bytes())),
        Environment::Process { program, args } => {
            Digest::of(format!("process:{program} {}", args.join(" ")).as_bytes())
        }
    }
}

/// The directory a run happens in: the inputs the host wrote, and an empty
/// outputs directory.
struct Workspace {
    directory: tempfile::TempDir,
    files: Vec<InputFile>,
    written: Vec<(PathBuf, Digest)>,
}

impl Workspace {
    fn new(inputs: &[Input]) -> Result<Self, VerifierFailure> {
        let failed = |error: std::io::Error| VerifierFailure::Workspace {
            detail: error.to_string(),
        };
        let directory = tempfile::tempdir().map_err(failed)?;
        let input_root = directory.path().join("inputs");
        fs::create_dir(&input_root).map_err(failed)?;
        fs::create_dir(directory.path().join("outputs")).map_err(failed)?;
        let mut files = Vec::new();
        let mut written = Vec::new();
        for input in inputs {
            let plain = !input.name.is_empty()
                && input.name != "."
                && input.name != ".."
                && !input.name.contains(['/', '\\', ':'])
                && !input.name.starts_with('.');
            if !plain
                || written
                    .iter()
                    .any(|(path, _): &(PathBuf, Digest)| path.ends_with(&input.name))
            {
                return Err(VerifierFailure::BadInput {
                    name: input.name.clone(),
                });
            }
            let path = input_root.join(&input.name);
            fs::write(&path, &input.bytes).map_err(failed)?;
            let mut permissions = fs::metadata(&path).map_err(failed)?.permissions();
            permissions.set_readonly(true);
            fs::set_permissions(&path, permissions).map_err(failed)?;
            let digest = Digest::of(&input.bytes);
            files.push(InputFile {
                name: input.name.clone(),
                digest: digest.to_string(),
                media_type: input.media_type.clone(),
            });
            written.push((path, digest));
        }
        Ok(Self {
            directory,
            files,
            written,
        })
    }

    fn root(&self) -> &Path {
        self.directory.path()
    }

    fn digests(&self) -> Vec<Digest> {
        self.written.iter().map(|(_, digest)| *digest).collect()
    }

    /// Whether every input is still the bytes the host wrote, and nothing was
    /// added beside them.
    fn check_inputs(&self) -> Result<(), VerifierFailure> {
        for (path, digest) in &self.written {
            let name = || {
                path.file_name()
                    .map_or_else(String::new, |n| n.to_string_lossy().into_owned())
            };
            let bytes =
                fs::read(path).map_err(|_| VerifierFailure::InputsModified { name: name() })?;
            if Digest::of(&bytes) != *digest {
                return Err(VerifierFailure::InputsModified { name: name() });
            }
        }
        let present = fs::read_dir(self.root().join("inputs"))
            .map_err(|error| VerifierFailure::Workspace {
                detail: error.to_string(),
            })?
            .count();
        if present != self.written.len() {
            return Err(VerifierFailure::InputsModified {
                name: "a file the host did not write".to_string(),
            });
        }
        Ok(())
    }

    /// Every file under `outputs/`, digested, within `limit` bytes in all.
    fn outputs(&self, limit: u64) -> Result<Vec<Output>, VerifierFailure> {
        let root = self.root().join("outputs");
        let mut pending = vec![root.clone()];
        let mut found = Vec::new();
        let mut total: u64 = 0;
        let failed = |error: std::io::Error| VerifierFailure::Workspace {
            detail: error.to_string(),
        };
        while let Some(directory) = pending.pop() {
            for entry in fs::read_dir(&directory).map_err(failed)? {
                let entry = entry.map_err(failed)?;
                let kind = entry.file_type().map_err(failed)?;
                if kind.is_dir() {
                    pending.push(entry.path());
                    continue;
                }
                if !kind.is_file() {
                    continue;
                }
                let length = entry.metadata().map_err(failed)?.len();
                total = total.saturating_add(length);
                if total > limit {
                    return Err(VerifierFailure::OutputsTooLarge {
                        limit,
                        found: total,
                    });
                }
                let bytes = fs::read(entry.path()).map_err(failed)?;
                let name = entry
                    .path()
                    .strip_prefix(&root)
                    .map_or_else(|_| entry.path(), Path::to_path_buf)
                    .components()
                    .map(|part| part.as_os_str().to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("/");
                found.push(Output {
                    name,
                    digest: Digest::of(&bytes),
                    byte_length: length,
                });
            }
        }
        found.sort_by(|a, b| a.name.cmp(&b.name));
        Ok(found)
    }
}

impl Drop for Workspace {
    fn drop(&mut self) {
        for (path, _) in &self.written {
            if let Ok(metadata) = fs::metadata(path) {
                let mut permissions = metadata.permissions();
                #[expect(
                    clippy::permissions_set_readonly_false,
                    reason = "the run is over and the host is removing its own files"
                )]
                permissions.set_readonly(false);
                let _ = fs::set_permissions(path, permissions);
            }
        }
    }
}

/// Runs a [`Environment::Process`] validator as a local program. The development
/// harness's transport: nothing it decides counts, and its records say so.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessTransport;

impl Transport for ProcessTransport {
    fn run(
        &self,
        registration: &Registration,
        workdir: &Path,
        request: &[u8],
    ) -> Result<RawRun, String> {
        let Environment::Process { program, args } = &registration.environment else {
            return Err("a process transport runs only process environments".to_string());
        };
        let mut command = Command::new(program);
        command
            .args(args)
            .current_dir(workdir)
            .env_clear()
            .env("CAPSULET_VERIFIER_INPUTS", workdir.join("inputs"))
            .env("CAPSULET_VERIFIER_OUTPUTS", workdir.join("outputs"))
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        // Only what a program needs to start at all on this platform.
        for name in ["PATH", "SYSTEMROOT", "TEMP", "TMP"] {
            if let Some(value) = std::env::var_os(name) {
                command.env(name, value);
            }
        }
        let mut child = command.spawn().map_err(|error| error.to_string())?;
        if let Some(mut stdin) = child.stdin.take() {
            stdin
                .write_all(request)
                .map_err(|error| error.to_string())?;
        }
        let stdout = child.stdout.take().ok_or("no standard output")?;
        let stderr = child.stderr.take().ok_or("no standard error")?;
        let reader = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stdout
                .take(u64::try_from(MAX_RESPONSE_BYTES).unwrap_or(u64::MAX) + 1)
                .read_to_end(&mut bytes);
            bytes
        });
        let errors = std::thread::spawn(move || {
            let mut bytes = Vec::new();
            let _ = stderr.take(64 * 1024).read_to_end(&mut bytes);
            let text = String::from_utf8_lossy(&bytes).into_owned();
            let start = text
                .char_indices()
                .rev()
                .nth(2048)
                .map_or(0, |(index, _)| index);
            text[start..].trim().to_string()
        });

        let deadline = Instant::now() + Duration::from_millis(registration.limits.timeout_ms);
        let (exit, timed_out) = loop {
            if let Some(status) = child.try_wait().map_err(|error| error.to_string())? {
                break (status.code(), false);
            }
            if Instant::now() >= deadline {
                let _ = child.kill();
                let _ = child.wait();
                break (None, true);
            }
            std::thread::sleep(Duration::from_millis(5));
        };
        Ok(RawRun {
            exit,
            timed_out,
            stdout: reader.join().unwrap_or_default(),
            stderr_tail: errors.join().unwrap_or_default(),
        })
    }
}
