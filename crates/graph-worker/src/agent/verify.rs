//! Verifier nodes: a run's obligations decided by an installed pack's
//! validators, or by one of the kernel's deterministic families.
//!
//! Both are generic. A `pack` node names a pack the definition pins, and hands
//! its validators the files its inputs hold: each input is a JSON object from a
//! relative path to that file's text. A `claim_reasoning` node hands the kernel
//! the proposal on its `proposal` input and the snapshot document on its
//! `snapshot` input, and decides the obligations its `capsulet.decides`
//! parameter names. Neither knows what any obligation means.
//!
//! Each leaves a [`NodeDecision`] on its `decision` output: the obligations it
//! decided, and only those, the record of every checker it ran, and the
//! evidence those rest on with its bytes. The run's certificate is sealed from those decisions and the
//! log, never from what a worker held in memory.
//!
//! A pack's validators run in containers through the verifier host, so a
//! worker with no container runtime runs the node and decides nothing: each
//! obligation stays residual, saying the validator could not be started.

use capsulet_ir::correctness::Identity;
use capsulet_ir::correctness::certificate::{VerifierRecord, VerifierTrust};
use capsulet_ir::correctness::evidence::EvidenceRef;
use capsulet_ir::correctness::obligation::{DischargeBasis, DischargeState, Obligation};
use capsulet_ir::correctness::proposal::{Producer, ProducerKind};
use capsulet_ir::id::Identifier;
use capsulet_kernel::family::{CLAIM_REASONING, CLAIM_REASONING_RULES, ClaimReasoning};
use capsulet_kernel::{
    DECISION_PORT, EvidenceMap, KERNEL_VERSION, NodeDecision, Proposal, SnapshotDocument, Verdict,
};
use capsulet_verifier::{ContainerTransport, Input};

use super::{AgentExecutor, Failure};
use crate::clock::{Clock, SystemClock};
use crate::execute::{NodeOutcome, NodeRequest};
use capsulet_ir::loop_region::FailureKind;
use serde_json::Value;

/// A node whose obligations an installed pack's validators decide.
pub const PACK_KIND: &str = "pack";
/// A node whose obligations the kernel's claim-reasoning family decides.
pub const CLAIM_REASONING_KIND: &str = "claim_reasoning";
/// The pack a `pack` node runs, by name: one the definition pins.
pub const PACK_PARAMETER: &str = "capsulet.pack";
/// The obligations a `claim_reasoning` node decides, by id, comma-separated.
pub const DECIDES_PARAMETER: &str = "capsulet.decides";

fn malformed(detail: impl Into<String>) -> Failure {
    Failure::new(FailureKind::SchemaMismatch, detail)
}

impl AgentExecutor {
    /// Runs the validators of the pack the node names over the files its
    /// inputs hold.
    pub(super) async fn decide_with_pack(
        &self,
        request: &NodeRequest<'_>,
    ) -> Result<NodeOutcome, Failure> {
        let name = request.node.parameters.get(PACK_PARAMETER).ok_or_else(|| {
            malformed(format!("a pack node names its pack in `{PACK_PARAMETER}`"))
        })?;
        let pin = request
            .definition
            .packs
            .iter()
            .find(|pin| &pin.name == name)
            .ok_or_else(|| malformed(format!("the definition pins no pack `{name}`")))?;
        let unavailable = |detail: String| Failure::new(FailureKind::VerifierUnavailable, detail);
        let record = self
            .store
            .find_domain_pack(request.tenant, request.project, &pin.name, &pin.version)
            .await
            .map_err(|error| unavailable(format!("the pack could not be read: {error}")))?
            .ok_or_else(|| {
                unavailable(format!(
                    "the project has not installed {} {}",
                    pin.name, pin.version
                ))
            })?;
        let bundle: capsulet_pack::Bundle = serde_json::from_value(record.bundle)
            .map_err(|error| unavailable(format!("the installed pack does not read: {error}")))?;
        let pack = capsulet_pack::Pack::from_bundle(bundle)
            .map_err(|error| unavailable(format!("the installed pack does not read: {error}")))?;
        let manifest = pack.manifest.digest().map_err(unavailable)?;
        if manifest != pin.manifest {
            return Err(unavailable(format!(
                "the project holds {} {} as manifest {manifest}, and the definition pins {}",
                pin.name, pin.version, pin.manifest
            )));
        }

        let inputs = files(self.load_inputs(request).await?)?;
        let producer = supplied_by(request);
        let at = SystemClock.now();
        // What a validator of the pack decides. The pack's other obligations,
        // reviews, are not this node's to report: the run's certificate holds
        // them residual unless another node of the run decides them.
        let deciding: std::collections::BTreeSet<String> = pack
            .manifest
            .validators
            .iter()
            .flat_map(|entry| entry.decides.iter().cloned())
            .collect();
        let decision = tokio::task::spawn_blocking(move || {
            capsulet_pack::decide(&pack, &inputs, &ContainerTransport::new(), &producer, at)
        })
        .await
        .map_err(|error| unavailable(format!("the validators did not finish: {error}")))?
        .map_err(|error| malformed(error.to_string()))?;
        let decision = NodeDecision::new(
            decision
                .obligations
                .into_iter()
                .filter(|obligation| deciding.contains(obligation.statement.id.as_str()))
                .collect(),
            decision.verifiers,
            decision.evidence,
            &decision.blobs,
        )
        .map_err(|error| malformed(error.to_string()))?;
        self.leave(request, &decision).await
    }

    /// Decides a proposal against a snapshot with the kernel's claim-reasoning
    /// family, which a replay re-decides from the two documents it pins.
    pub(super) async fn decide_claims(
        &self,
        request: &NodeRequest<'_>,
    ) -> Result<NodeOutcome, Failure> {
        let statements = claimed(request)?;
        let inputs = self.load_inputs(request).await?;
        let document = |port: &str| {
            inputs
                .get(port)
                .ok_or_else(|| malformed(format!("a claim-reasoning node reads `{port}`")))
                .and_then(|value| {
                    serde_json::to_vec(value).map_err(|error| malformed(error.to_string()))
                })
        };
        let proposal_bytes = document("proposal")?;
        let snapshot_bytes = document("snapshot")?;
        let proposal = Proposal::read(&proposal_bytes)
            .map_err(|error| malformed(format!("the proposal does not read: {error}")))?;
        let snapshot = serde_json::from_slice::<SnapshotDocument>(&snapshot_bytes)
            .map_err(|error| malformed(format!("the snapshot does not read: {error}")))?
            .rebuild()
            .map_err(|error| {
                malformed(format!("the snapshot does not rebuild: {}", error.detail))
            })?;
        let reasoned = ClaimReasoning::decide(&proposal, &snapshot);

        let at = SystemClock.now();
        let producer = supplied_by(request);
        let mut bytes = EvidenceMap::new();
        let mut cite = |name: &str, content: Vec<u8>| -> Result<EvidenceRef, Failure> {
            let byte_length = content.len() as u64;
            Ok(EvidenceRef {
                id: Identifier::parse(format!("{CLAIM_REASONING_KIND}/{name}"))
                    .map_err(|error| malformed(error.to_string()))?,
                content: bytes.insert(content),
                media_type: "application/json".to_string(),
                byte_length,
                producer: producer.clone(),
                captured_at: at,
            })
        };
        let evidence = vec![
            cite("proposal", proposal_bytes)?,
            cite("snapshot", snapshot_bytes)?,
        ];
        let pinned: Vec<_> = evidence.iter().map(|reference| reference.content).collect();
        let family =
            Identifier::parse(CLAIM_REASONING).map_err(|error| malformed(error.to_string()))?;
        let record = VerifierRecord {
            identity: Identity::new(family.clone(), CLAIM_REASONING_RULES),
            environment: capsulet_ir::Digest::of(KERNEL_VERSION.as_bytes()),
            inputs: pinned.clone(),
            outputs: Vec::new(),
            trust: VerifierTrust::Deterministic,
            verdict: reasoned.verdict,
        };
        let obligations = statements
            .into_iter()
            .map(|(contract, statement)| Obligation {
                state: state_of(reasoned.claim.as_ref(), &family, &pinned, statement.owner),
                statement,
                contract,
            })
            .collect();
        let decision = NodeDecision::new(obligations, vec![record], evidence, &bytes)
            .map_err(|error| malformed(error.to_string()))?;
        self.leave(request, &decision).await
    }

    /// Finishes the node with its decision on its `decision` output.
    async fn leave(
        &self,
        request: &NodeRequest<'_>,
        decision: &NodeDecision,
    ) -> Result<NodeOutcome, Failure> {
        if !request
            .node
            .outputs
            .iter()
            .any(|port| port.id.as_str() == DECISION_PORT)
        {
            return Err(malformed(format!(
                "a verifier node leaves its decision on a `{DECISION_PORT}` output, which `{}` \
                 does not declare",
                request.node.id
            )));
        }
        let value = serde_json::to_value(decision).map_err(|error| malformed(error.to_string()))?;
        self.finish(request, &value, None).await
    }
}

/// The statements a claim-reasoning node decides, with their contracts.
fn claimed(
    request: &NodeRequest<'_>,
) -> Result<
    Vec<(
        Identifier,
        capsulet_ir::correctness::obligation::ObligationStatement,
    )>,
    Failure,
> {
    let ids: Vec<&str> = request
        .node
        .parameters
        .get(DECIDES_PARAMETER)
        .map(|value| {
            value
                .split(',')
                .map(str::trim)
                .filter(|id| !id.is_empty())
                .collect()
        })
        .unwrap_or_default();
    if ids.is_empty() {
        return Err(malformed(format!(
            "a claim-reasoning node names what it decides in `{DECIDES_PARAMETER}`"
        )));
    }
    ids.into_iter()
        .map(|id| {
            request
                .definition
                .contracts
                .iter()
                .find_map(|contract| {
                    contract
                        .obligations
                        .iter()
                        .find(|statement| statement.id.as_str() == id)
                        .map(|statement| (contract.id.clone(), statement.clone()))
                })
                .ok_or_else(|| malformed(format!("no contract of the definition states `{id}`")))
        })
        .collect()
}

/// What the kernel's verdict does to an obligation it decides: accepted
/// discharges it, mechanically; rejected fails it, owned as it says;
/// conditional leaves it residual with the kernel's reasons.
fn state_of(
    claim: Option<&capsulet_kernel::Certificate>,
    family: &Identifier,
    pinned: &[capsulet_ir::Digest],
    owner: capsulet_ir::correctness::obligation::RepairOwner,
) -> DischargeState {
    match claim.map(|claim| claim.verdict) {
        Some(Verdict::Accepted) => DischargeState::Discharged {
            by: family.clone(),
            evidence: pinned.to_vec(),
            basis: DischargeBasis::Mechanical,
        },
        Some(Verdict::Rejected) => DischargeState::Failed {
            reason: claim.map_or_else(String::new, |claim| {
                claim
                    .errors
                    .iter()
                    .map(|error| format!("{}: {}", error.code, error.message))
                    .collect::<Vec<_>>()
                    .join("; ")
            }),
            owner,
        },
        Some(Verdict::Conditional) | None => DischargeState::Residual {
            rationale: claim
                .map(|claim| {
                    claim
                        .residuals
                        .iter()
                        .map(|residual| residual.rationale.clone())
                        .collect::<Vec<_>>()
                        .join("; ")
                })
                .filter(|rationale| !rationale.is_empty())
                .unwrap_or_else(|| "the kernel left it conditional".to_string()),
            evidence: pinned.to_vec(),
        },
    }
}

/// Who supplied what a verifier node was handed: the run's own values, read
/// for the node.
fn supplied_by(request: &NodeRequest<'_>) -> Producer {
    Producer {
        kind: ProducerKind::Retrieval,
        identity: Identity::new(request.node.id.clone(), request.definition.version.clone()),
    }
}

/// A node's inputs as files: each a JSON object from a relative path to the
/// file's text.
fn files(inputs: std::collections::BTreeMap<String, Value>) -> Result<Vec<Input>, Failure> {
    let mut files: Vec<Input> = Vec::new();
    for (port, value) in inputs {
        let Value::Object(entries) = value else {
            return Err(malformed(format!(
                "input `{port}` of a pack node is an object from each file's path to its text"
            )));
        };
        for (path, text) in entries {
            let Value::String(text) = text else {
                return Err(malformed(format!(
                    "`{path}` on input `{port}` is not a file's text"
                )));
            };
            if files.iter().any(|file| file.name == path) {
                return Err(malformed(format!("two inputs hold `{path}`")));
            }
            files.push(Input {
                media_type: capsulet_pack::media_type_of(&path).to_string(),
                name: path,
                bytes: text.into_bytes(),
            });
        }
    }
    Ok(files)
}
