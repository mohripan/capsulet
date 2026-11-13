//! The contract-clauses reference pack (#159), held to what it claims.
//!
//! - It is a pack: every file is listed as it is, and once signed it installs,
//!   so its contract compiles and its obligations hold together.
//! - A typed question about a termination right, asked of the services
//!   agreement under the pack's contract, is answered, cited, checked for
//!   responsiveness, and left `conditional` on the clause that qualifies it
//!   until a holder of `review:legal` decides it.
//! - Every case in the misreading corpus (#162) that the pack's mechanisms claim
//!   to catch is caught under the pack's own contract, and the case it does not
//!   claim is still the recorded limitation.
//! - Its obligations are what ADR 0021 says a legal pack's are under `enforce`:
//!   a reading is discharged by a review under `review:legal` and by nothing
//!   else, and a model only endorses or disputes one.
//!
//! Answers are built the way the ask endpoint builds them: the text chunked into
//! citable spans, an alphabet over them with the text beside it, and the
//! proposer's own assembly of what a model emitted.

#[path = "../../ir/tests/fixtures/mod.rs"]
mod fixtures;

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use capsulet_core::{
    Authority, Evidence, EvidenceId, EvidenceSpan, MemoryContract, MemoryContractId, MemoryScope,
    Source, SourceContent, SourceId,
};
use capsulet_ir::assurance::{BoundaryDecision, BoundaryPolicy, DecisionContext};
use capsulet_ir::correctness::certificate::{Subject, VerifierRecord, VerifierTrust};
use capsulet_ir::correctness::evidence::EvidenceRef;
use capsulet_ir::correctness::obligation::{BasisKind, DischargeBasis, DischargeState};
use capsulet_ir::correctness::proposal::{Producer, ProducerKind};
use capsulet_ir::correctness::{Certificate as Sealed, CertificateBody};
use capsulet_ir::{
    AssuranceMode, AssurancePolicy, AssuranceVerdict, CertificateError, CheckerVerdict, Contract,
    Digest, Identity, Obligation, PackPin, RecordedTime, decide_boundary,
};
use capsulet_kernel::{
    Certificate, Phrasebook, Proposal, Proposition, Query, Question, READING_AUTHORITY,
    ReadingDecision, ReadingReview, Rule, Slot, Snapshot, Verdict, check, reading_digest, reviewed,
};
use capsulet_pack::{TrustStore, admit, files_of, generate, obligations, read, sign};
use capsulet_proposer::{EvidenceAlphabet, RawProposal, build_proposal_in, chunk_into_spans};
use fixtures::{definition_in, id};
use serde::Deserialize;

fn pack_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/contract-clauses")
}

fn contract() -> MemoryContract {
    let source = fs::read_to_string(pack_dir().join("contracts/contract-clauses.contract"))
        .expect("the pack's contract reads");
    let contract = MemoryContract::parse(
        MemoryContractId::new("contract-clauses").expect("contract id"),
        "Contract clauses",
        source,
    )
    .expect("the pack's contract parses");
    contract.compile().expect("the pack's contract compiles");
    contract
}

fn phrasebook() -> Phrasebook {
    Phrasebook::from_contract(contract().ast())
}

fn sample(name: &str) -> String {
    fs::read_to_string(pack_dir().join("samples").join(name)).expect("a sample reads")
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

#[test]
fn the_pack_lists_its_files_and_once_signed_installs() {
    let committed = read(&pack_dir()).expect("the pack reads");
    assert_eq!(
        committed.manifest.files,
        files_of(&pack_dir()).expect("its files"),
        "packs/contract-clauses/pack.json lists every file as it is: run `capsulet pack refresh`"
    );
    assert!(committed.signature.is_none(), "committed unsigned");

    let directory = tempfile::tempdir().expect("dir");
    copy(&pack_dir(), directory.path());
    let (key, _) = generate().expect("a key");
    sign(directory.path(), &key).expect("signs");
    let mut trust = TrustStore::new();
    trust.keys.push(
        key.trusted_for(vec!["contract-clauses".to_string()])
            .expect("a public key"),
    );
    let signed = read(directory.path()).expect("reads");
    let admitted = admit(&signed, &trust, &[]).expect("installs");
    assert_eq!(
        (admitted.name.as_str(), admitted.version.as_str()),
        ("contract-clauses", "1.1.0")
    );

    let contracts = obligations(&signed).expect("its obligations hold together");
    assert_eq!(contracts.len(), 1);
    let ids: Vec<&str> = contracts[0]
        .obligations
        .iter()
        .map(|obligation| obligation.id.as_str())
        .collect();
    assert_eq!(
        ids,
        [
            "clause-reading",
            "answers-the-question",
            "references-resolve"
        ]
    );
    assert_eq!(
        contract().ast().review_policy().reading_authority(),
        Some("review:legal"),
        "the contract and its obligations name the same authority"
    );
}

/// A document asked about, as the ask endpoint stores it.
struct Asked {
    snapshot: Snapshot,
    alphabet: EvidenceAlphabet,
}

fn asked(text: &str, question: &str, query: Query) -> Asked {
    let scope = MemoryScope::new("acme", "legal").expect("scope");
    let source_id = SourceId::new("source_agreement").expect("source id");
    let content = SourceContent::new(source_id.clone(), text).expect("content");
    let evidence: Vec<Evidence> = chunk_into_spans(&content)
        .into_iter()
        .enumerate()
        .map(|(index, (start, end))| {
            Evidence::new(
                EvidenceId::new(format!("source_agreement_ev_{index}")).expect("evidence id"),
                scope.clone(),
                source_id.clone(),
                format!("bytes {start}..{end}"),
                &text[start..end],
                "inline",
            )
            .expect("evidence")
            .with_span(EvidenceSpan::new(start, end, content.content_hash()).expect("span"))
        })
        .collect();
    let alphabet = EvidenceAlphabet::from_evidence(&evidence).with_source(&content);
    let mut snapshot = Snapshot::new()
        .with_reading_authority(contract().ast().review_policy().reading_authority())
        .with_question(Question {
            text: question.to_string(),
            query: Some(query),
        })
        .with_phrasebook(phrasebook())
        .with_source(
            Source::new(
                source_id,
                scope,
                "inline",
                None,
                "Services agreement",
                Authority::High,
            )
            .expect("source"),
        )
        .with_source_content(content);
    for item in evidence {
        snapshot = snapshot.with_evidence(item);
    }
    Asked { snapshot, alphabet }
}

impl Asked {
    fn id_of(&self, excerpt: &str) -> String {
        self.alphabet
            .entries()
            .iter()
            .find(|entry| entry.excerpt == excerpt)
            .unwrap_or_else(|| panic!("{excerpt:?} is not one citable span"))
            .evidence_id
            .clone()
    }

    /// What a model emits citing `clause` for `answer`, assembled by the proposer.
    fn answer(&self, clause: &str, answer: (&str, &str, &str)) -> Proposal {
        let raw = RawProposal {
            subject: answer.0.to_string(),
            predicate: answer.1.to_string(),
            object: answer.2.to_string(),
            evidence_id: self.id_of(clause),
            quote: clause.to_string(),
            needs_interpretation: false,
            rationale: String::new(),
            asks_relation: String::new(),
            asks_subject: String::new(),
            asks_object: String::new(),
            asks_rationale: String::new(),
        };
        build_proposal_in(&raw, &self.alphabet, &phrasebook()).expect("cited from the alphabet")
    }
}

const TERMINATION: &str =
    "Subject to Section 8.3, either party may terminate this Agreement on 60 days' written notice.";
const MINIMUM_TERM: &str = "Notwithstanding Section 8.1, neither party may terminate this \
                            Agreement for convenience during the Initial Term.";

fn review(certificate: &Certificate, authority: &str, decision: ReadingDecision) -> ReadingReview {
    ReadingReview {
        reading: reading_digest(certificate, &certificate.residuals[0]),
        decision,
        reviewer: "counsel".to_string(),
        authority: authority.to_string(),
        rationale: "read with Section 8.3, either party may terminate after the Initial Term"
            .to_string(),
    }
}

#[test]
fn a_typed_question_about_a_termination_right_waits_on_a_legal_reviewer() {
    let asked = asked(
        &sample("services-agreement.txt"),
        "May either party terminate this Agreement?",
        Query::new(
            "may_terminate",
            Slot::bound("either party"),
            Slot::bound("this Agreement"),
        ),
    );
    let proposal = asked.answer(
        TERMINATION,
        ("either party", "may_terminate", "this Agreement"),
    );
    let certificate = check(&proposal, &asked.snapshot);

    // Answered, cited, and checked against the question it was asked.
    assert!(certificate.errors.is_empty(), "{:?}", certificate.errors);
    let rules: Vec<&str> = certificate
        .discharged
        .iter()
        .map(|step| step.rule.as_str())
        .collect();
    assert!(rules.contains(&"cite"), "{rules:?}");
    assert!(rules.contains(&"answer"), "{rules:?}");

    // Conditional on the clause that qualifies it, and on nothing else: the
    // definitions it uses were read with it.
    assert_eq!(certificate.verdict, Verdict::Conditional);
    assert_eq!(
        certificate.residuals.len(),
        1,
        "{:?}",
        certificate.residuals
    );
    let reading = &certificate.residuals[0];
    assert!(
        reading.rationale.contains("\"subject to\" provision 8.3"),
        "{}",
        reading.rationale
    );
    assert!(
        reading.evidence_ids.contains(&asked.id_of(MINIMUM_TERM)),
        "the reviewer is shown the clause that qualifies it: {:?}",
        reading.evidence_ids
    );
    assert_eq!(
        certificate.reading_authority.as_deref(),
        Some("review:legal")
    );

    // Only a holder of review:legal decides it.
    let general = [review(
        &certificate,
        READING_AUTHORITY,
        ReadingDecision::Holds,
    )];
    assert_eq!(
        reviewed(&certificate, &general).verdict,
        Verdict::Conditional
    );
    let legal = [review(&certificate, "review:legal", ReadingDecision::Holds)];
    let outcome = reviewed(&certificate, &legal);
    assert_eq!(outcome.verdict, Verdict::Accepted);
    assert!(
        outcome.by_review,
        "accepted on review, never by the kernel alone"
    );
    let refuted = [review(
        &certificate,
        "review:legal",
        ReadingDecision::DoesNotHold,
    )];
    assert_eq!(reviewed(&certificate, &refuted).verdict, Verdict::Rejected);
}

#[test]
fn an_answer_to_another_question_or_a_bare_quote_is_never_accepted() {
    let text = sample("services-agreement.txt");
    // The Supplier's right is not the Customer's.
    let asked_customer = asked(
        &text,
        "May the Customer terminate this Agreement?",
        Query::new(
            "may_terminate",
            Slot::bound("the Customer"),
            Slot::bound("this Agreement"),
        ),
    );
    let supplier = asked_customer.answer(
        "The Supplier may terminate this Agreement at once if an invoice stays unpaid for 45 days.",
        ("The Supplier", "may_terminate", "this Agreement"),
    );
    let certificate = check(&supplier, &asked_customer.snapshot);
    assert_eq!(certificate.verdict, Verdict::Rejected);
    assert!(
        certificate
            .errors
            .iter()
            .any(|error| error.code == "goal_not_responsive"),
        "{:?}",
        certificate.errors
    );

    // Quoted bare, with no document around it, the qualified clause is refused
    // for being qualified.
    let asked_either = asked(
        &text,
        "May either party terminate this Agreement?",
        Query::new(
            "may_terminate",
            Slot::bound("either party"),
            Slot::bound("this Agreement"),
        ),
    );
    let goal = Proposition::new("either party", "may_terminate", "this Agreement");
    let bare = Proposal {
        schema_version: Proposal::current_schema_version(),
        question_reading: None,
        goal: goal.clone(),
        derivation: Rule::Trust {
            premise: Box::new(Rule::Cite {
                evidence_id: asked_either.id_of(TERMINATION),
                proposition: goal,
                read_with: Vec::new(),
            }),
            min_authority: "low".to_string(),
        },
    };
    let certificate = check(&bare, &asked_either.snapshot);
    assert!(
        certificate
            .errors
            .iter()
            .any(|error| error.code == "relation_qualified"),
        "{certificate:?}"
    );
}

#[test]
fn a_clause_with_defined_terms_and_no_qualifier_is_accepted_read_with_them() {
    let asked = asked(
        &sample("services-agreement.txt"),
        "Which law governs this Agreement?",
        Query::new("governed_by", Slot::bound("This Agreement"), Slot::Wanted),
    );
    let proposal = asked.answer(
        "This Agreement is governed by the laws of Ontario.",
        ("This Agreement", "governed_by", "Ontario"),
    );
    let certificate = check(&proposal, &asked.snapshot);
    assert_eq!(certificate.verdict, Verdict::Accepted, "{certificate:?}");
}

/// The misreading corpus, as `crates/kernel/tests/misreading_corpus.rs` reads it.
#[derive(Deserialize)]
struct Corpus {
    agreement: String,
    cases: Vec<Case>,
    limitations: Vec<Limitation>,
}

#[derive(Deserialize)]
struct Case {
    clause: String,
    question: String,
    query: Wanted,
    answer: Answer,
    reader: String,
}

#[derive(Deserialize)]
struct Wanted {
    relation: String,
    subject: Option<String>,
    object: Option<String>,
}

#[derive(Deserialize)]
struct Answer {
    subject: String,
    predicate: String,
    object: String,
}

#[derive(Deserialize)]
struct Limitation {
    clause: String,
}

/// What stopped a corpus answer under this pack, read off its certificate.
fn caught(certificate: &Certificate) -> &'static str {
    let has = |code: &str| certificate.errors.iter().any(|error| error.code == code);
    if has("goal_not_responsive") {
        "query"
    } else if has("relation_qualified") {
        "qualifier"
    } else if has("relation_not_stated") {
        "unstated"
    } else if certificate
        .residuals
        .iter()
        .any(|residual| residual.rationale.contains("defined term"))
    {
        "definition"
    } else if certificate.verdict == Verdict::Accepted {
        "nothing"
    } else {
        panic!("stopped by something the corpus does not name: {certificate:?}")
    }
}

/// A corpus case decided under the pack's contract, in the direct-quote shape
/// the corpus is defined by.
fn under_the_pack(corpus: &Corpus, case: &Case) -> Certificate {
    let slot = |term: &Option<String>| term.as_deref().map_or(Slot::Wanted, Slot::bound);
    let asked = asked(
        &corpus.agreement,
        &case.question,
        Query::new(
            &case.query.relation,
            slot(&case.query.subject),
            slot(&case.query.object),
        ),
    );
    let goal = Proposition::new(
        &case.answer.subject,
        &case.answer.predicate,
        &case.answer.object,
    );
    let proposal = Proposal {
        schema_version: Proposal::current_schema_version(),
        question_reading: None,
        goal: goal.clone(),
        derivation: Rule::Trust {
            premise: Box::new(Rule::Cite {
                evidence_id: asked.id_of(&case.clause),
                proposition: goal,
                read_with: Vec::new(),
            }),
            min_authority: "low".to_string(),
        },
    };
    check(&proposal, &asked.snapshot)
}

/// What catches each corpus case under this pack: the clause, the reader's
/// label, and the mechanism. The kernel's own fixture names a mechanism per case
/// too; here "unless" is one of the pack's declared conditions, so the renewal
/// is caught as qualified rather than unstated.
#[rustfmt::skip]
const CAUGHT_UNDER_THE_PACK: &[(&str, &str, &str)] = &[
    ("Fees are non-refundable, except as provided in Section 11.", "misread", "qualifier"),
    ("Subject to Section 8, the Supplier is liable for all damages.", "misread", "qualifier"),
    ("Either party may terminate this agreement, provided that all fees are paid.", "misread", "qualifier"),
    ("The Supplier may assign this agreement to an Affiliate.", "misread", "definition"),
    ("This agreement renews for one year unless either party objects.", "misread", "qualifier"),
    ("The Supplier may raise prices once a year.", "misread", "nothing"),
    ("The Customer shall pay the fees within 30 days of each invoice.", "non_responsive", "query"),
    ("This agreement is governed by the laws of Ontario.", "non_responsive", "query"),
    ("This agreement is governed by the laws of Ontario.", "sound", "nothing"),
    ("The Customer shall pay the fees within 30 days of each invoice.", "sound", "nothing"),
];

#[test]
fn every_misreading_the_packs_mechanisms_claim_is_caught_under_its_contract() {
    let corpus: Corpus = serde_json::from_str(include_str!(
        "../../kernel/tests/fixtures/misreading-corpus.json"
    ))
    .expect("the corpus reads");
    let expected: BTreeMap<(&str, &str), &str> = CAUGHT_UNDER_THE_PACK
        .iter()
        .map(|&(clause, reader, mechanism)| ((clause, reader), mechanism))
        .collect();
    assert_eq!(
        corpus.cases.len(),
        expected.len(),
        "a case was added: pin it here"
    );

    for case in &corpus.cases {
        let certificate = under_the_pack(&corpus, case);
        let mechanism = caught(&certificate);
        assert_eq!(
            Some(&mechanism),
            expected.get(&(case.clause.as_str(), case.reader.as_str())),
            "{:?} ({})",
            case.clause,
            case.reader
        );
        let limitation = corpus
            .limitations
            .iter()
            .any(|limitation| limitation.clause == case.clause);
        match case.reader.as_str() {
            "sound" => assert_eq!(certificate.verdict, Verdict::Accepted, "{:?}", case.clause),
            // The one the pack does not claim: a qualification in a sentence of
            // its own. Still accepted, and still on the corpus's list.
            _ if limitation => assert_eq!(certificate.verdict, Verdict::Accepted),
            _ => assert_ne!(
                certificate.verdict,
                Verdict::Accepted,
                "{} answer from {:?} reached accepted",
                case.reader,
                case.clause
            ),
        }
    }
}

const CAPTURED_AT: i64 = 1_794_000_000_000;

fn evidence() -> EvidenceRef {
    let content = sample("services-agreement.txt").into_bytes();
    EvidenceRef {
        id: id("services-agreement"),
        content: Digest::of(&content),
        media_type: "text/plain".to_string(),
        byte_length: content.len() as u64,
        producer: Producer {
            kind: ProducerKind::Deterministic,
            identity: Identity::new(id("upload"), "1"),
        },
        captured_at: RecordedTime(CAPTURED_AT),
    }
}

fn verifier(name: &str, trust: VerifierTrust) -> VerifierRecord {
    VerifierRecord {
        identity: Identity::new(id(name), "1"),
        environment: Digest::of(name.as_bytes()),
        inputs: vec![],
        outputs: vec![],
        trust,
        verdict: CheckerVerdict::Accepted,
    }
}

/// The pack's obligations contract, and a definition under `enforce` that pins
/// the pack and declares it.
fn enforced() -> (Contract, capsulet_ir::Definition) {
    let pack = read(&pack_dir()).expect("the pack reads");
    let contract = obligations(&pack).expect("its obligations").remove(0);
    let mut definition = definition_in(AssuranceMode::Enforce);
    definition.contracts.push(contract.clone());
    definition.packs = vec![PackPin {
        name: "contract-clauses".to_string(),
        version: "1.1.0".to_string(),
        manifest: pack.manifest.digest().expect("a digest"),
    }];
    (contract, definition)
}

fn sealed(clause_reading: DischargeState) -> Result<Sealed, CertificateError> {
    let (contract, definition) = enforced();
    let admission = capsulet_ir::admit(&definition).expect("the definition is admitted");
    let statement = |name: &str| {
        contract
            .obligations
            .iter()
            .find(|obligation| obligation.id.as_str() == name)
            .expect("the pack states it")
            .clone()
    };
    let obligations = vec![
        Obligation {
            statement: statement("clause-reading"),
            contract: contract.id.clone(),
            state: clause_reading,
        },
        // A typed question: the kernel checked the answer against the query it
        // was pinned to, so this one is mechanical.
        Obligation {
            statement: statement("answers-the-question"),
            contract: contract.id.clone(),
            state: DischargeState::Discharged {
                by: id("mechanical"),
                evidence: vec![evidence().content],
                basis: DischargeBasis::Mechanical,
            },
        },
        Obligation {
            statement: statement("references-resolve"),
            contract: contract.id.clone(),
            state: DischargeState::Discharged {
                by: id("references-resolve"),
                evidence: vec![evidence().content],
                basis: DischargeBasis::ReplayableTool,
            },
        },
    ];
    let verdict = AssuranceVerdict::under_mode(AssuranceMode::Enforce, &obligations);
    Sealed::seal(CertificateBody {
        schema_version: Sealed::current_schema_version(),
        id: id("cert-termination"),
        admission: admission.clone(),
        mode: AssuranceMode::Enforce,
        subject: Subject {
            definition: *admission.definition(),
            definition_version: "1".to_string(),
            run: Some(id("run-termination")),
            inputs: vec![],
            outputs: vec![],
        },
        policy_version: "contract-clauses/1".to_string(),
        kernel_version: "capsulet-kernel 0.1.0".to_string(),
        contracts: vec![contract.id.clone()],
        verifiers: vec![
            verifier(
                "references-resolve",
                VerifierTrust::DeclaredOracle {
                    rationale: "busybox in a pinned image, with no network".to_string(),
                },
            ),
            verifier("mechanical", VerifierTrust::Deterministic),
            verifier(
                "second-reader",
                VerifierTrust::ModelJudgment {
                    family: "other".to_string(),
                    independent_of_proposer: true,
                    calibration: Some(id("calibration-2026-11")),
                },
            ),
        ],
        evidence: vec![evidence()],
        obligations,
        loops: vec![],
        verdict,
    })
}

fn by_review(authority: &str) -> DischargeState {
    DischargeState::Discharged {
        by: id("counsel"),
        evidence: vec![evidence().content],
        basis: DischargeBasis::HumanReview {
            authority: id(authority),
        },
    }
}

fn crossing(certificate: &Sealed) -> BoundaryDecision {
    let (contract, definition) = enforced();
    let policy = AssurancePolicy {
        id: id("legal-release"),
        version: "1".to_string(),
        mode: AssuranceMode::Enforce,
        required_contracts: vec![contract.id.clone()],
        required_verifiers: vec![],
        boundaries: BTreeMap::from([(
            id("publish-boundary"),
            BoundaryPolicy {
                minimum: AssuranceVerdict::Accepted,
                contract: Some(contract.id),
                requires_approval: None,
                max_age_ms: None,
                refuse_disputed: true,
                endorsement: None,
            },
        )]),
        waiver_authorities: vec![],
        trust_routes: vec![],
    };
    decide_boundary(
        &policy,
        AssuranceMode::Enforce,
        &definition,
        Some(certificate),
        &id("publish-boundary"),
        DecisionContext::at(RecordedTime(CAPTURED_AT)),
    )
}

#[test]
fn under_enforce_a_reading_is_discharged_by_a_legal_review_and_nothing_else() {
    let (contract, _) = enforced();
    let reading = contract
        .obligations
        .iter()
        .find(|obligation| obligation.id.as_str() == "clause-reading")
        .expect("the pack states it");
    assert_eq!(reading.accepts, [BasisKind::HumanReview]);
    assert_eq!(
        reading
            .authority
            .as_ref()
            .map(capsulet_ir::Identifier::as_str),
        Some("review:legal")
    );

    // A review under review:legal discharges it, and the answer crosses.
    let reviewed = sealed(by_review("review:legal")).expect("seals");
    assert_eq!(reviewed.verdict(), AssuranceVerdict::Accepted);
    let decision = crossing(&reviewed);
    assert!(decision.permits_crossing(), "{decision:?}");

    // A review under another authority, a mechanical check, and a model each
    // fail to discharge it, and the certificate says so at the seal.
    for (state, what) in [
        (by_review("review:write"), "another authority"),
        (
            DischargeState::Discharged {
                by: id("mechanical"),
                evidence: vec![evidence().content],
                basis: DischargeBasis::Mechanical,
            },
            "a mechanical check",
        ),
        (
            DischargeState::Discharged {
                by: id("second-reader"),
                evidence: vec![],
                basis: DischargeBasis::Mechanical,
            },
            "a model",
        ),
    ] {
        assert!(sealed(state).is_err(), "{what} discharged a reading");
    }

    // A model may endorse or dispute it. Either way it stays outstanding, and
    // the answer does not cross.
    for state in [
        DischargeState::Endorsed {
            by: id("second-reader"),
            evidence: vec![],
        },
        DischargeState::Disputed {
            by: id("second-reader"),
            reason: "Section 8.3 bars termination in the Initial Term".to_string(),
            evidence: vec![],
        },
    ] {
        let certificate = sealed(state).expect("seals");
        assert_eq!(certificate.verdict(), AssuranceVerdict::Conditional);
        assert!(!crossing(&certificate).permits_crossing());
    }
}

/// The corpus's verdicts under this pack's contract: the figures the M4
/// completion report states beside the kernel's own
/// (`docs/contracts/m4-completion.md`).
#[test]
fn the_verdicts_the_completion_report_states_under_the_pack_are_what_the_corpus_measures() {
    let corpus: Corpus = serde_json::from_str(include_str!(
        "../../kernel/tests/fixtures/misreading-corpus.json"
    ))
    .expect("the corpus reads");
    let mut verdicts = (0, 0, 0);
    for case in &corpus.cases {
        match under_the_pack(&corpus, case).verdict {
            Verdict::Accepted => verdicts.0 += 1,
            Verdict::Conditional => verdicts.1 += 1,
            Verdict::Rejected => verdicts.2 += 1,
        }
    }
    // (accepted, conditional, rejected)
    assert_eq!(verdicts, (3, 1, 6));
}
