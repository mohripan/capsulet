//! The M4 gate (#163): three unrelated workloads on one IR and certificate
//! protocol, end to end on a stack.
//!
//! The workloads are the reference packs: a question about a contract clause,
//! two code changes, and two infrastructure changes. Each is a definition that
//! pins its pack and declares the pack's obligations, run on the stack as an
//! ordinary IR run with its inputs, and certified from the run's log over the
//! API. Each certificate's bundle is then read back from the API and replayed
//! offline: the kernel re-decides what it can, and every replayable tool is run
//! again in the container its record pins.
//!
//! Nothing in the kernel or the runtime knows any of these packs. The worker's
//! verifier nodes are generic: one runs a pinned pack's validators, the other
//! the kernel's claim-reasoning family.
//!
//! The stack is the gate's own: the API, PostgreSQL and the object store. The
//! worker is this crate's code, driven here against the stack's database,
//! because a pack's validators run in containers and nothing on the stack holds
//! a container runtime.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use capsulet_core::{
    Evidence, EvidenceId, EvidenceSpan, MemoryContract, MemoryContractId, MemoryScope,
    SourceContent, SourceId,
};
use capsulet_graph_worker::agent::verify::{
    CLAIM_REASONING_KIND, DECIDES_PARAMETER, PACK_KIND, PACK_PARAMETER,
};
use capsulet_graph_worker::{AgentExecutor, GraphWorker, Progress, SystemClock, WorkerConfig};
use capsulet_ir::agent::KIND_PARAMETER;
use capsulet_ir::correctness::obligation::{DischargeState, RepairOwner};
use capsulet_ir::graph::{Combine, Endpoint, Graph, GraphBuilder, Hyperedge, TrustDerivation};
use capsulet_ir::node::{Node, NodeKind};
use capsulet_ir::port::{InputPort, OutputPort};
use capsulet_ir::value::ValueSchema;
use capsulet_ir::{
    AssuranceMode, AssuranceVerdict, CapabilitySet, Definition, Identifier, PackPin, ResourceBudget,
};
use capsulet_kernel::family::ClaimReasoning;
use capsulet_kernel::snapshot_document::{EvidenceDocument, RelationDocument, SourceDocument};
use capsulet_kernel::{
    Bundle, ContextPolicy, Phrasebook, Query, Question, ReplayNote, ReplayOutcome, Slot,
    SnapshotDocument, Verdict, replay,
};
use capsulet_pack::{Pack, TrustStore, generate, obligations, read, replay_tools, sign};
use capsulet_postgres::PostgresStore;
use capsulet_proposer::{EvidenceAlphabet, RawProposal, build_proposal_in, chunk_into_spans};
use capsulet_verifier::ContainerTransport;
use reqwest::Method;
use serde_json::{Value, json};

/// How long the runs may take, validators and image pulls included.
const RUNS_FINISH_WITHIN: Duration = Duration::from_mins(15);

fn id(value: &str) -> Identifier {
    Identifier::parse(value).expect("an identifier")
}

fn packs() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs")
}

fn required(name: &str) -> String {
    std::env::var(name).unwrap_or_else(|_| {
        panic!(
            "{name} is not set: run this through `cargo run -p capsulet-xtask -- verify --gate m4`"
        )
    })
}

/// The gate's stack, as the fixture file describes it.
struct Stack {
    client: reqwest::Client,
    api: String,
    token: String,
    project: String,
    tenant: String,
    trust: PathBuf,
    database: String,
}

impl Stack {
    fn from_environment() -> Self {
        let fixture: Value = serde_json::from_str(
            &fs::read_to_string(required("CAPSULET_FIXTURE_FILE")).expect("the fixture file"),
        )
        .expect("the fixture is JSON");
        let admin = &fixture["identities"]["admin"];
        Self {
            client: reqwest::Client::builder()
                .timeout(Duration::from_secs(120))
                .build()
                .expect("a client"),
            api: required("CAPSULET_API_URL"),
            token: admin["token"]
                .as_str()
                .expect("the admin's token")
                .to_string(),
            project: admin["project"]
                .as_str()
                .expect("the admin's project")
                .to_string(),
            tenant: fixture["tenant"].as_str().expect("the tenant").to_string(),
            trust: PathBuf::from(required("CAPSULET_PACK_TRUST_DIR")),
            database: required("CAPSULET_DATABASE_URL"),
        }
    }

    async fn call(&self, method: Method, path: &str, body: Option<&Value>, status: u16) -> Value {
        let mut request = self
            .client
            .request(method.clone(), format!("{}{path}", self.api))
            .bearer_auth(&self.token)
            .header("x-capsulet-project-id", &self.project);
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request.send().await.expect("the API answers");
        let found = response.status().as_u16();
        let text = response.text().await.expect("a body");
        assert_eq!(found, status, "{method} {path}: {text}");
        serde_json::from_str(&text).unwrap_or(Value::Null)
    }

    /// Has the project trust `key` for `packs`, the way an operator would: a
    /// trust store in the directory the API reads them from.
    fn trust(&self, key: &capsulet_pack::PrivateKey, packs: &[&str]) {
        let mut trust = TrustStore::new();
        trust.keys.push(
            key.trusted_for(packs.iter().map(ToString::to_string).collect())
                .expect("a public key"),
        );
        let store = self.trust.join(&self.tenant);
        fs::create_dir_all(&store).expect("the trust directory");
        fs::write(
            store.join(format!("{}.json", self.project)),
            serde_json::to_vec_pretty(&trust).expect("encodes"),
        )
        .expect("the trust store");
    }

    /// Signs a copy of the pack with `key` and installs it over the API.
    async fn install(&self, name: &str, key: &capsulet_pack::PrivateKey) {
        let directory = tempfile::tempdir().expect("a directory");
        copy(&packs().join(name), directory.path());
        sign(directory.path(), key).expect("signs");
        let pack = read(directory.path()).expect("reads");
        self.call(
            Method::POST,
            "/v1/packs",
            Some(&serde_json::to_value(pack.bundle()).expect("encodes")),
            200,
        )
        .await;
    }

    async fn register(&self, definition: &Definition) -> String {
        let registered = self
            .call(
                Method::POST,
                "/v1/ir/definitions",
                Some(&json!({ "definition": definition })),
                201,
            )
            .await;
        registered["digest"]
            .as_str()
            .expect("the definition's digest")
            .to_string()
    }

    async fn start(&self, run: &str, digest: &str, inputs: Value) {
        self.call(
            Method::POST,
            "/v1/ir/runs",
            Some(&json!({ "definition_digest": digest, "id": run, "inputs": inputs })),
            201,
        )
        .await;
    }

    async fn status(&self, run: &str) -> String {
        self.call(Method::GET, &format!("/v1/ir/runs/{run}"), None, 200)
            .await["status"]
            .as_str()
            .expect("a status")
            .to_string()
    }

    /// Seals the run's certificate over the API, then reads its bundle back.
    async fn certified(&self, run: &str) -> Bundle {
        let sealed = self
            .call(
                Method::POST,
                &format!("/v1/ir/runs/{run}/certificate"),
                None,
                200,
            )
            .await;
        let again = self
            .call(
                Method::POST,
                &format!("/v1/ir/runs/{run}/certificate"),
                None,
                200,
            )
            .await;
        assert_eq!(
            sealed["replay_digest"], again["replay_digest"],
            "sealing a run twice seals one certificate"
        );
        let id = sealed["id"].as_str().expect("the certificate's id");
        let bundle = self
            .call(
                Method::GET,
                &format!("/v1/assurance/certificates/{id}/bundle"),
                None,
                200,
            )
            .await;
        Bundle::read(bundle["bundle"].as_str().expect("a bundle").as_bytes()).expect("reads")
    }
}

fn copy(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).expect("read") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            fs::create_dir_all(&target).expect("dir");
            copy(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy");
        }
    }
}

fn opaque(reason: &str) -> ValueSchema {
    ValueSchema::Json {
        reason: reason.to_string(),
    }
}

/// A verifier node reading `ports`, each fed by the graph input of its name,
/// and leaving its decision on `decision`.
fn verifier(name: &str, parameters: &[(&str, &str)], ports: &[&str]) -> Node {
    Node {
        id: id(name),
        name: name.to_string(),
        kind: NodeKind::Verifier,
        inputs: ports
            .iter()
            .map(|port| InputPort::new(id(port), opaque("what the verifier reads")))
            .collect(),
        outputs: vec![OutputPort::new(
            id(capsulet_kernel::DECISION_PORT),
            opaque("what the verifier decided"),
        )],
        capabilities: vec![],
        effects: vec![],
        budget: ResourceBudget {
            wall_ms: 300_000,
            tokens: 0,
            cost_micro_units: 0,
            effect_count: 0,
        },
        provider: None,
        sub_workflow: None,
        parameters: parameters
            .iter()
            .map(|(key, value)| ((*key).to_string(), (*value).to_string()))
            .collect(),
    }
}

/// A definition under `enforce` that pins `pack` and declares its obligations.
fn workload(name: &str, pack: &Pack, nodes: Vec<Node>) -> Definition {
    let mut edges = Vec::new();
    let mut inputs = Vec::new();
    for node in &nodes {
        for port in &node.inputs {
            if !inputs.iter().any(|input: &OutputPort| input.id == port.id) {
                inputs.push(OutputPort::new(port.id.clone(), opaque("a workload input")));
            }
            edges.push(Hyperedge {
                id: id(&format!("{}-to-{}", port.id, node.id)),
                sources: vec![Endpoint::GraphInput {
                    name: port.id.clone(),
                }],
                targets: vec![Endpoint::Port {
                    node: node.id.clone(),
                    port: port.id.clone(),
                }],
                combine: Combine::Forward,
                trust: TrustDerivation::Weakest,
            });
        }
    }
    let graph = Graph::new(GraphBuilder {
        nodes,
        edges,
        inputs,
        ..GraphBuilder::default()
    })
    .expect("a graph");
    Definition {
        schema_version: Definition::current_schema_version(),
        id: id(name),
        version: "1".to_string(),
        name: name.to_string(),
        assurance: AssuranceMode::Enforce,
        capabilities: CapabilitySet::empty(),
        budget: ResourceBudget {
            wall_ms: 900_000,
            tokens: 0,
            cost_micro_units: 0,
            effect_count: 0,
        },
        graph,
        boundaries: vec![],
        contracts: obligations(pack).expect("the pack's obligations"),
        packs: vec![PackPin {
            name: pack.manifest.name.clone(),
            version: pack.manifest.version.clone(),
            manifest: pack.manifest.digest().expect("a digest"),
        }],
    }
}

fn pack_node(pack: &str) -> Node {
    verifier(
        "check",
        &[(KIND_PARAMETER, PACK_KIND), (PACK_PARAMETER, pack)],
        &["change"],
    )
}

/// Files under `root`, by their paths below it.
fn files(root: &Path, under: &[&str]) -> Value {
    let mut found = serde_json::Map::new();
    for prefix in under {
        let directory = root.join(prefix);
        for entry in fs::read_dir(&directory).expect("a fixture directory") {
            let path = entry.expect("entry").path();
            let name = format!(
                "{prefix}/{}",
                path.file_name().expect("a name").to_string_lossy()
            );
            found.insert(
                name,
                Value::String(fs::read_to_string(&path).expect("text")),
            );
        }
    }
    Value::Object(found)
}

/// The snapshot a question about the services agreement is decided against,
/// as the document a certificate pins: the agreement and its spans, the
/// contract's relations, cues and context policy, and the typed question.
fn snapshot_of(
    tenant: &str,
    project: &str,
    text: &str,
    content: &SourceContent,
    evidence: &[Evidence],
    ast: &capsulet_core::MemoryContractAst,
) -> SnapshotDocument {
    let policy = ast.statement_policy();
    let source = content.source_id();
    let question = Question {
        text: "Which law governs this Agreement?".to_string(),
        query: Some(Query::new(
            "governed_by",
            Slot::bound("This Agreement"),
            Slot::Wanted,
        )),
    };
    SnapshotDocument {
        sources: vec![SourceDocument {
            id: source.as_str().to_string(),
            tenant_id: tenant.to_string(),
            project_id: project.to_string(),
            kind: "inline".to_string(),
            uri: None,
            title: "Services agreement".to_string(),
            authority: "high".to_string(),
            content: text.to_string(),
        }],
        evidence: evidence
            .iter()
            .map(|item| EvidenceDocument {
                id: item.id().as_str().to_string(),
                tenant_id: tenant.to_string(),
                project_id: project.to_string(),
                source_id: source.as_str().to_string(),
                locator: item.locator().to_string(),
                excerpt: item.excerpt().to_string(),
                observed_at: item.observed_at().to_string(),
                span_start: item.span().map(EvidenceSpan::start),
                span_end: item.span().map(EvidenceSpan::end),
                source_content_hash: Some(content.content_hash().to_string()),
            })
            .collect(),
        relations: ast
            .relation_types()
            .iter()
            .filter(|relation| !relation.phrasings().is_empty())
            .map(|relation| RelationDocument {
                name: relation.name().to_string(),
                stated_by: relation
                    .phrasings()
                    .iter()
                    .map(|phrasing| phrasing.as_str().to_string())
                    .collect(),
                object_type: None,
            })
            .collect(),
        cues: policy.cues().to_vec(),
        default_currency: policy.default_currency().map(str::to_string),
        date_order: policy.date_order().map(str::to_string),
        question: Some(question),
        context: ContextPolicy {
            provisions: policy.provisions().to_vec(),
            references: policy.references().to_vec(),
            subject_to: policy.subject_to().to_vec(),
            overrides: policy.overrides().to_vec(),
            conditions: policy.conditions().to_vec(),
            definitions: policy.definitions().to_vec(),
        },
        reading_authority: ast.review_policy().reading_authority().map(str::to_string),
        ..SnapshotDocument::default()
    }
}

/// A question about the services agreement, put the way the reasoning
/// endpoint puts it: the agreement as a source cut into citable spans, the
/// pack's contract as the phrasebook and context policy, a typed query, and
/// the answer a model would give citing one clause, assembled by the proposer.
fn legal_question(tenant: &str, project: &str) -> (Value, Value, Value) {
    let pack = packs().join("contract-clauses");
    let text = fs::read_to_string(pack.join("samples/services-agreement.txt")).expect("a sample");
    let contract = MemoryContract::parse(
        MemoryContractId::new("contract-clauses").expect("an id"),
        "Contract clauses",
        fs::read_to_string(pack.join("contracts/contract-clauses.contract")).expect("the contract"),
    )
    .expect("parses");
    let ast = contract.ast();

    let scope = MemoryScope::new(tenant, project).expect("a scope");
    let source = SourceId::new("source_agreement").expect("a source id");
    let content = SourceContent::new(source.clone(), &text).expect("content");
    let spans = chunk_into_spans(&content);
    let evidence: Vec<Evidence> = spans
        .iter()
        .enumerate()
        .map(|(index, (start, end))| {
            Evidence::new(
                EvidenceId::new(format!("source_agreement_ev_{index}")).expect("an id"),
                scope.clone(),
                source.clone(),
                format!("bytes {start}..{end}"),
                &text[*start..*end],
                "inline",
            )
            .expect("evidence")
            .with_span(EvidenceSpan::new(*start, *end, content.content_hash()).expect("a span"))
        })
        .collect();

    let snapshot = snapshot_of(tenant, project, &text, &content, &evidence, ast);

    let clause = "This Agreement is governed by the laws of Ontario.";
    let alphabet = EvidenceAlphabet::from_evidence(&evidence).with_source(&content);
    let cited = alphabet
        .entries()
        .iter()
        .find(|entry| entry.excerpt == clause)
        .expect("the governing-law clause is one citable span")
        .evidence_id
        .clone();
    let proposal = build_proposal_in(
        &RawProposal {
            subject: "This Agreement".to_string(),
            predicate: "governed_by".to_string(),
            object: "Ontario".to_string(),
            evidence_id: cited,
            quote: clause.to_string(),
            needs_interpretation: false,
            rationale: String::new(),
            asks_relation: String::new(),
            asks_subject: String::new(),
            asks_object: String::new(),
            asks_rationale: String::new(),
        },
        &alphabet,
        &Phrasebook::from_contract(ast),
    )
    .expect("cited from the alphabet");

    // What the stack will decide, decided here first: a question this gate
    // asks has to be one the kernel can answer from these two documents.
    let rebuilt = snapshot.rebuild().expect("the snapshot rebuilds");
    assert_eq!(
        ClaimReasoning::decide(&proposal, &rebuilt)
            .claim
            .map(|claim| claim.verdict),
        Some(Verdict::Accepted)
    );

    let agreement = json!({ "services-agreement.txt": text });
    (
        agreement,
        serde_json::to_value(&proposal).expect("encodes"),
        serde_json::to_value(&snapshot).expect("encodes"),
    )
}

/// One run's certificate, after it was read back and replayed.
struct Certified {
    verdict: AssuranceVerdict,
    states: BTreeMap<String, DischargeState>,
}

/// Replays a bundle offline and runs its tools again from it.
fn replayed(pack: &Pack, bundle: &Bundle) -> Certified {
    let evidence = bundle.evidence().expect("its evidence");
    let certificate = &bundle.certificate;
    let notes = match replay(certificate, &evidence) {
        ReplayOutcome::Reproduced { verdict, notes } => {
            assert_eq!(verdict, certificate.verdict());
            notes
        }
        other => panic!("{} does not replay: {other:?}", certificate.body().id),
    };
    let tools = replay_tools(pack, certificate, &evidence, &ContainerTransport::new())
        .expect("the tools run again");
    assert!(
        tools.iter().all(capsulet_pack::ToolReplay::agrees),
        "{}: {tools:?}",
        certificate.body().id
    );
    let oracles = notes
        .iter()
        .filter(|note| matches!(note, ReplayNote::OracleNotReExecuted { .. }))
        .count();
    assert_eq!(oracles, tools.len(), "every tool replay notes is run again");
    Certified {
        verdict: certificate.verdict(),
        states: certificate
            .body()
            .obligations
            .iter()
            .map(|obligation| {
                (
                    obligation.statement.id.to_string(),
                    obligation.state.clone(),
                )
            })
            .collect(),
    }
}

async fn drive(stack: &Stack, runs: &[&str]) {
    let store = PostgresStore::connect(&stack.database)
        .await
        .expect("the stack's database");
    let worker = GraphWorker::new(
        store.clone(),
        Arc::new(AgentExecutor::new(store, None)),
        Arc::new(SystemClock),
        WorkerConfig {
            worker_id: "m4-gate".to_string(),
            ..WorkerConfig::default()
        },
    );
    let started = Instant::now();
    loop {
        let mut finished = 0;
        for run in runs {
            if matches!(
                stack.status(run).await.as_str(),
                "completed" | "failed" | "cancelled"
            ) {
                finished += 1;
            }
        }
        if finished == runs.len() {
            return;
        }
        assert!(
            started.elapsed() < RUNS_FINISH_WITHIN,
            "the runs did not finish within {RUNS_FINISH_WITHIN:?}"
        );
        if matches!(
            worker.advance_one().await.expect("the worker"),
            Progress::NothingToDo
        ) {
            tokio::time::sleep(Duration::from_millis(250)).await;
        }
    }
}

fn discharged_by(state: &DischargeState, basis: &str) -> bool {
    matches!(state, DischargeState::Discharged { basis: found, .. } if found.as_str() == basis)
}

/// Registers a definition per pack and starts the five runs, returning their
/// ids: the clause question, two code changes and two infrastructure changes.
async fn started(stack: &Stack, [legal, code, policy]: [&Pack; 3]) -> [String; 5] {
    let unique = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("a clock")
        .as_millis();
    let legal_definition = workload(
        "m4-contract-clauses",
        legal,
        vec![
            verifier(
                "references",
                &[
                    (KIND_PARAMETER, PACK_KIND),
                    (PACK_PARAMETER, "contract-clauses"),
                ],
                &["agreement"],
            ),
            verifier(
                "reading",
                &[
                    (KIND_PARAMETER, CLAIM_REASONING_KIND),
                    (DECIDES_PARAMETER, "answers-the-question"),
                ],
                &["proposal", "snapshot"],
            ),
        ],
    );
    let code_definition = workload("m4-code-changes", code, vec![pack_node("code-changes")]);
    let policy_definition = workload(
        "m4-infrastructure-policy",
        policy,
        vec![pack_node("infrastructure-policy")],
    );
    let legal_digest = stack.register(&legal_definition).await;
    let code_digest = stack.register(&code_definition).await;
    let policy_digest = stack.register(&policy_definition).await;

    let run = |name: &str| format!("m4-{name}-{unique}");
    let (agreement, proposal, snapshot) = legal_question(&stack.tenant, &stack.project);
    let fixtures = |pack: &str, scenario: &str| packs().join(pack).join("fixtures").join(scenario);
    let runs = [
        run("governing-law"),
        run("broken-build"),
        run("fix"),
        run("public-bucket"),
        run("compliant"),
    ];
    stack
        .start(
            &runs[0],
            &legal_digest,
            json!({ "agreement": agreement, "proposal": proposal, "snapshot": snapshot }),
        )
        .await;
    for (index, scenario) in [(1, "broken-build"), (2, "fix")] {
        stack
            .start(
                &runs[index],
                &code_digest,
                json!({ "change": files(&fixtures("code-changes", scenario), &["base", "change"]) }),
            )
            .await;
    }
    for (index, scenario) in [(3, "public-bucket"), (4, "compliant")] {
        let fixture = fixtures("infrastructure-policy", scenario);
        let change = json!({
            "change/plan.json": fs::read_to_string(fixture.join("plan.json")).expect("a plan"),
            "scope.json": fs::read_to_string(fixture.join("scope.json")).expect("a scope"),
        });
        stack
            .start(&runs[index], &policy_digest, json!({ "change": change }))
            .await;
    }

    runs
}

#[tokio::test(flavor = "multi_thread")]
async fn three_unrelated_packs_run_end_to_end_on_a_stack_and_their_certificates_replay_offline() {
    let stack = Stack::from_environment();
    let (key, _) = generate().expect("a key");
    let names = ["contract-clauses", "code-changes", "infrastructure-policy"];
    stack.trust(&key, &names);
    for name in names {
        stack.install(name, &key).await;
    }
    let legal = read(&packs().join("contract-clauses")).expect("reads");
    let code = read(&packs().join("code-changes")).expect("reads");
    let policy = read(&packs().join("infrastructure-policy")).expect("reads");

    let runs = started(&stack, [&legal, &code, &policy]).await;

    let names: Vec<&str> = runs.iter().map(String::as_str).collect();
    drive(&stack, &names).await;
    for run in &runs {
        assert_eq!(stack.status(run).await, "completed", "{run}");
    }

    // The clause question: the kernel decided the answer, a tool decided that
    // every reference resolves, and the reading waits for a legal reviewer.
    let governing_law = replayed(&legal, &stack.certified(&runs[0]).await);
    assert_eq!(governing_law.verdict, AssuranceVerdict::Conditional);
    assert!(discharged_by(
        &governing_law.states["answers-the-question"],
        "mechanical"
    ));
    assert!(discharged_by(
        &governing_law.states["references-resolve"],
        "replayable_tool"
    ));
    assert!(matches!(
        governing_law.states["clause-reading"],
        DischargeState::Residual { .. }
    ));

    // A change that does not build is rejected, owned by its proposer; a
    // sound fix waits only for its review.
    let broken = replayed(&code, &stack.certified(&runs[1]).await);
    assert_eq!(broken.verdict, AssuranceVerdict::Rejected);
    assert!(matches!(
        broken.states["builds"],
        DischargeState::Failed {
            owner: RepairOwner::Proposer,
            ..
        }
    ));
    let fix = replayed(&code, &stack.certified(&runs[2]).await);
    assert_eq!(fix.verdict, AssuranceVerdict::Conditional);
    assert!(matches!(
        fix.states["does-what-was-asked"],
        DischargeState::Residual { .. }
    ));

    // A public bucket is rejected, naming the rule; a compliant change is
    // accepted on the tools' word alone.
    let public = replayed(&policy, &stack.certified(&runs[3]).await);
    assert_eq!(public.verdict, AssuranceVerdict::Rejected);
    assert!(matches!(
        &public.states["no-public-buckets"],
        DischargeState::Failed { reason, .. }
            if reason == "no-public-buckets: aws_s3_bucket_acl.docs grants public-read"
    ));
    let compliant = replayed(&policy, &stack.certified(&runs[4]).await);
    assert_eq!(compliant.verdict, AssuranceVerdict::Accepted);
    for state in compliant.states.values() {
        assert!(discharged_by(state, "replayable_tool"), "{state:?}");
    }
}
