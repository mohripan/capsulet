//! The correctness kernel.
//!
//! A proposer — a model, a retriever, anything untrusted — emits a [`Proposal`].
//! The kernel decides it and issues a [`Certificate`]. Nothing here is learned
//! and nothing here performs I/O.
//!
//! Every check is total: `check` always terminates with a verdict. Derivations
//! nest — [`Rule::Trust`] and [`Rule::Interpret`] each carry a premise, and the
//! proposer chooses how deep to go — so totality rests on
//! [`MAX_DERIVATION_DEPTH`], a bound the kernel states rather than inherits
//! from whatever stack it happens to run on.
//!
//! The design boundary is deliberate. Provenance, arithmetic, record state and
//! policy are mechanically decidable, so the kernel decides them. Whether a
//! passage *means* what a proposition says is not decidable over natural
//! language, so the kernel refuses to pretend: [`Rule::Interpret`] discharges
//! nothing and records a [`Residual`], which is what makes a verdict
//! [`Verdict::Conditional`] rather than [`Verdict::Accepted`].

pub mod bundle;
pub mod certificate;
pub mod error;
pub mod family;
pub mod ir;
pub mod quantity;
pub mod replay;
pub mod snapshot;
pub mod snapshot_document;
pub mod workflow;

use capsulet_core::{Authority, ClaimStatus, verify_evidence_span};

pub use bundle::{Bundle, BundleError};
pub use certificate::{Certificate, CertificateError, DischargedStep, Residual, Verdict};
pub use error::{CheckError, RepairOwner};
pub use family::{ClaimReasoning, ObligationFamily, from_checker_verdict, to_checker_verdict};
pub use ir::{ArithOp, Judgment, Proposal, Proposition, Rule};
pub use quantity::{MAX_SCALE, Quantity, QuantityError};
pub use replay::{EvidenceMap, EvidenceSource, ReplayFinding, ReplayNote, ReplayOutcome, replay};
pub use snapshot::Snapshot;
pub use snapshot_document::SnapshotDocument;
pub use workflow::{Assembly, KERNEL_VERSION, certify};

/// The deepest chain of nested rules the kernel will walk.
///
/// [`Rule::Trust`] and [`Rule::Interpret`] each carry a premise, so a
/// derivation is a chain and a proposer chooses its length. Without a bound the
/// kernel recurses as far as it is asked and a deep enough proposal exhausts
/// the stack — which ends the process rather than producing a verdict, so no
/// caller can catch it and nothing is recorded about why.
///
/// The value is deliberately far above any real derivation. Grounding a claim
/// takes a citation, a trust step, and sometimes a reading: three rules, not
/// sixty-four. A bound this loose refuses nothing anyone would write and still
/// keeps the walk to a depth any thread's stack holds comfortably.
///
/// A caller passing JSON is *also* bounded by `serde_json`, whose own recursion
/// limit gives up at a shallower depth than this. That is a property of the
/// format the caller happened to choose, not a decision this kernel made, so
/// the kernel does not rely on it: a format without such a limit, or a value
/// built in process, reaches [`check`] directly.
///
/// What this bound covers is the kernel's own reading of a derivation. It does
/// not make [`Rule`] itself safe at any depth: the type is recursive, so
/// dropping, cloning or encoding a value built far past this bound still
/// recurses in code the kernel does not own — measured here at tens of
/// thousands of rules. Nothing that arrives through a supported transport gets
/// near it, and [`check`] refuses such a value without reading it, but the
/// durable fix is to bound depth where a `Rule` is constructed rather than
/// where it is used.
pub const MAX_DERIVATION_DEPTH: u32 = 64;

/// The longest cited span the kernel will treat as a citation.
///
/// A citation points at a span, and containment in that span is the whole of
/// what [`Rule::Cite`] establishes. A span the size of a document points at
/// nothing in particular: almost any short phrase is "contained" in it, so
/// containment stops being evidence that the document says the proposition and
/// becomes evidence only that the words exist somewhere in it.
///
/// Four kilobytes is roughly a page — long enough for any passage worth quoting
/// and short enough that a reader can check the citation by looking at it, which
/// is the point of a citation.
pub const MAX_CITED_EXCERPT_BYTES: usize = 4096;

/// Decides a proposal against a snapshot.
///
/// Always terminates, for every proposal including a hostile one: a derivation
/// nested past [`MAX_DERIVATION_DEPTH`] is rejected rather than walked. A
/// failure anywhere produces [`Verdict::Rejected`] with the specific reasons;
/// an otherwise sound derivation that required a reading produces
/// [`Verdict::Conditional`] with the readings recorded.
#[must_use]
pub fn check(proposal: &Proposal, snapshot: &Snapshot) -> Certificate {
    // Measured before anything reads the derivation, because walking it is not
    // the only recursion over it: `replay_digest` serializes the proposal, and
    // a serializer has no depth limit of its own. A bound that only guarded the
    // walk would still hand a hostile derivation to the encoder.
    if exceeds_depth_bound(&proposal.derivation) {
        return refused_as_too_deep(proposal);
    }

    // Computed before any checking, because a proposal nobody can pin is one no
    // certificate can be about.
    let replay_digest = match replay_digest(proposal) {
        Ok(digest) => digest,
        Err(source) => return refused_as_unencodable(proposal, &source),
    };

    let mut state = CheckState::default();
    let outcome = derive(&proposal.derivation, snapshot, &mut state, 1);

    if let Some(judgment) = &outcome {
        let derived = judgment.proposition().canonical();
        let goal = proposal.goal.canonical();
        if derived != goal {
            state.errors.push(CheckError::GoalNotDerived {
                derived: judgment.canonical(),
                goal,
            });
        }
    }

    let verdict = if outcome.is_none() || !state.errors.is_empty() {
        Verdict::Rejected
    } else if state.residuals.is_empty() {
        Verdict::Accepted
    } else {
        Verdict::Conditional
    };

    Certificate {
        verdict,
        goal: proposal.goal.clone(),
        discharged: state.discharged,
        residuals: state.residuals,
        errors: state
            .errors
            .iter()
            .map(|error| CertificateError {
                code: error.code().to_string(),
                message: error.to_string(),
                repair_owner: error.repair_owner().as_str().to_string(),
                corrected_value: error.corrected_value(),
            })
            .collect(),
        replay_digest,
        derivation_depth_limit: Some(MAX_DERIVATION_DEPTH),
    }
}

/// The certificate for a proposal the kernel could not pin.
fn refused_as_unencodable(
    proposal: &Proposal,
    source: &capsulet_ir::CanonicalError,
) -> Certificate {
    let error = CheckError::ProposalNotEncodable {
        detail: source.to_string(),
    };
    refusal(proposal, "proposal-not-encodable", &error)
}

/// Whether the derivation nests past [`MAX_DERIVATION_DEPTH`].
///
/// Iterative on purpose: a recursive measurement would be the recursion it is
/// meant to detect. Each rule carries at most one premise, so the chain walks
/// with a single borrow and no stack. Stopping at the bound rather than
/// measuring the whole chain also means a hostile derivation costs work
/// proportional to the bound, not to the length the proposer chose.
fn exceeds_depth_bound(rule: &Rule) -> bool {
    let mut depth: u32 = 1;
    let mut current = rule;
    while let Rule::Trust { premise, .. } | Rule::Interpret { premise, .. } = current {
        depth += 1;
        if depth > MAX_DERIVATION_DEPTH {
            return true;
        }
        current = premise;
    }
    false
}

/// The certificate for a derivation the kernel refused to read.
///
/// The derivation is not digested. Encoding it is the recursion this refusal
/// exists to avoid, so the digest covers the goal and says plainly that the
/// derivation was never read — an honest identifier for a proposal that was
/// turned away at the door, rather than one that pretends to pin bytes nobody
/// looked at.
fn refused_as_too_deep(proposal: &Proposal) -> Certificate {
    let error = CheckError::DerivationTooDeep {
        limit: MAX_DERIVATION_DEPTH,
    };
    refusal(proposal, "derivation-too-deep", &error)
}

/// A certificate for a proposal the kernel turned away without reading.
///
/// The derivation is not digested — encoding it is the very thing that could not
/// be done, or must not be — so the digest covers the goal and the reason. That
/// is an honest identifier for a proposal refused at the door, rather than one
/// pretending to pin bytes nobody looked at.
fn refusal(proposal: &Proposal, reason: &str, error: &CheckError) -> Certificate {
    Certificate {
        verdict: Verdict::Rejected,
        goal: proposal.goal.clone(),
        discharged: Vec::new(),
        residuals: Vec::new(),
        errors: vec![CertificateError {
            code: error.code().to_string(),
            message: error.to_string(),
            repair_owner: error.repair_owner().as_str().to_string(),
            corrected_value: error.corrected_value(),
        }],
        replay_digest: capsulet_core::content_digest(
            format!("{reason}|{}", proposal.goal.canonical()).as_bytes(),
        ),
        derivation_depth_limit: Some(MAX_DERIVATION_DEPTH),
    }
}

#[derive(Default)]
struct CheckState {
    discharged: Vec<DischargedStep>,
    residuals: Vec<Residual>,
    errors: Vec<CheckError>,
}

impl CheckState {
    fn discharge(&mut self, rule: &str, concluded: &Judgment, detail: String) {
        self.discharged.push(DischargedStep {
            rule: rule.to_string(),
            concluded: concluded.canonical(),
            detail,
        });
    }
}

/// Evaluates one rule, `depth` rules into the derivation. Returns `None` when
/// the step could not be taken at all, after recording why.
///
/// The bound is checked on the way in rather than before the walk, so a
/// derivation is refused for being too deep only if the kernel actually reaches
/// that depth — and refusing costs one frame, not a traversal of whatever is
/// left below it.
fn derive(
    rule: &Rule,
    snapshot: &Snapshot,
    state: &mut CheckState,
    depth: u32,
) -> Option<Judgment> {
    if depth > MAX_DERIVATION_DEPTH {
        state.errors.push(CheckError::DerivationTooDeep {
            limit: MAX_DERIVATION_DEPTH,
        });
        return None;
    }
    match rule {
        Rule::Cite {
            evidence_id,
            proposition,
        } => derive_cite(evidence_id, proposition, snapshot, state),
        Rule::Attest { claim_id } => derive_attest(claim_id, snapshot, state),
        Rule::Trust {
            premise,
            min_authority,
        } => derive_trust(premise, min_authority, snapshot, state, depth),
        Rule::Arith {
            op,
            operands,
            claimed,
            proposition,
        } => derive_arith(*op, operands, *claimed, proposition, state),
        Rule::Interpret {
            premise,
            proposition,
            rationale,
        } => derive_interpret(premise, proposition, rationale, snapshot, state, depth),
    }
}

/// `Cite` is where fabrication is caught.
///
/// It establishes only that a source *said* something, and only when the
/// evidence re-derives from the stored bytes and the proposition's object
/// appears literally within the cited span. That is containment, not entailment
/// — reaching the proposition's meaning still requires [`Rule::Interpret`].
fn derive_cite(
    evidence_id: &str,
    proposition: &Proposition,
    snapshot: &Snapshot,
    state: &mut CheckState,
) -> Option<Judgment> {
    let Some(evidence) = snapshot.evidence(evidence_id) else {
        state.errors.push(CheckError::DanglingEvidence {
            evidence_id: evidence_id.to_string(),
        });
        return None;
    };
    let source_id = evidence.source_id().as_str().to_string();

    let Some(span) = evidence.span() else {
        state.errors.push(CheckError::Provenance {
            evidence_id: evidence_id.to_string(),
            source: capsulet_core::ProvenanceError::SpanMissing {
                evidence_id: evidence_id.to_string(),
            },
        });
        return None;
    };
    let Some(content) = snapshot.source_content(&source_id, span.source_content_hash()) else {
        state.errors.push(CheckError::SourceContentMissing {
            source_id,
            content_hash: span.source_content_hash().to_string(),
        });
        return None;
    };
    if let Err(error) = verify_evidence_span(evidence, content) {
        state.errors.push(CheckError::Provenance {
            evidence_id: evidence_id.to_string(),
            source: error,
        });
        return None;
    }

    // Checked after provenance, so a span that does not re-derive is reported as
    // the fabrication it is rather than as a size complaint.
    let excerpt = evidence.excerpt();
    if excerpt.len() > MAX_CITED_EXCERPT_BYTES {
        state.errors.push(CheckError::CitedSpanTooLong {
            evidence_id: evidence_id.to_string(),
            limit: MAX_CITED_EXCERPT_BYTES,
            found: excerpt.len(),
        });
        return None;
    }

    // Both endpoints of the relation must be present, not just the object.
    // Checking only the object lets a proposer attach any subject it likes to a
    // span that happens to contain a matching string — "Contoso acquired-by X"
    // grounded on a sentence about anniversary dates. The predicate is exempt:
    // it is an ontology label, not a quotation.
    for (role, value) in [
        ("subject", &proposition.subject),
        ("object", &proposition.object),
    ] {
        if !contains_normalized(evidence.excerpt(), value) {
            state.errors.push(CheckError::TermNotInSpan {
                evidence_id: evidence_id.to_string(),
                role,
                term: value.clone(),
                excerpt: evidence.excerpt().to_string(),
            });
            return None;
        }
    }

    let judgment = Judgment::Says {
        source_id: source_id.clone(),
        proposition: proposition.clone(),
    };
    state.discharge(
        "cite",
        &judgment,
        format!(
            "excerpt re-derived from {source_id} bytes {}..{} and contains the object",
            span.start(),
            span.end()
        ),
    );
    Some(judgment)
}

fn derive_attest(claim_id: &str, snapshot: &Snapshot, state: &mut CheckState) -> Option<Judgment> {
    let Some(claim) = snapshot.claim(claim_id) else {
        state.errors.push(CheckError::DanglingClaim {
            claim_id: claim_id.to_string(),
        });
        return None;
    };
    if claim.status() != ClaimStatus::Active {
        state.errors.push(CheckError::ClaimNotActive {
            claim_id: claim_id.to_string(),
            status: claim.status().to_string(),
        });
        return None;
    }
    let judgment = Judgment::Holds {
        proposition: Proposition::new(
            claim.subject_id().as_str(),
            claim.predicate(),
            claim.object(),
        ),
    };
    state.discharge(
        "attest",
        &judgment,
        format!(
            "claim {claim_id} is active with authority {}",
            claim.authority()
        ),
    );
    Some(judgment)
}

/// The only rule that turns attribution into assertion.
fn derive_trust(
    premise: &Rule,
    min_authority: &str,
    snapshot: &Snapshot,
    state: &mut CheckState,
    depth: u32,
) -> Option<Judgment> {
    let inner = derive(premise, snapshot, state, depth + 1)?;
    let Judgment::Says {
        source_id,
        proposition,
    } = inner
    else {
        state.errors.push(CheckError::TrustPremiseNotAttributed {
            found: inner.canonical(),
        });
        return None;
    };
    let Some(required) = parse_authority(min_authority) else {
        state.errors.push(CheckError::UnknownAuthority {
            value: min_authority.to_string(),
        });
        return None;
    };
    let Some(actual) = snapshot.authority_of(&source_id) else {
        state.errors.push(CheckError::DanglingSource { source_id });
        return None;
    };
    if authority_rank(actual) < authority_rank(required) {
        state.errors.push(CheckError::AuthorityBelowFloor {
            source_id,
            actual: actual.to_string(),
            required: required.to_string(),
        });
        return None;
    }
    let judgment = Judgment::Holds { proposition };
    state.discharge(
        "trust",
        &judgment,
        format!("source {source_id} authority {actual} meets the {required} floor"),
    );
    Some(judgment)
}

/// The kernel computes the value itself, so a wrong number is caught and the
/// right one is already known.
fn derive_arith(
    op: ArithOp,
    operands: &[Quantity],
    claimed: Quantity,
    proposition: &Proposition,
    state: &mut CheckState,
) -> Option<Judgment> {
    if operands.is_empty() {
        state
            .errors
            .push(CheckError::ArithNoOperands { op: op.as_str() });
        return None;
    }
    let Some(computed) = op.apply(operands) else {
        // Operands there were, but no exact answer to be had. Reporting that as
        // a mismatch would name a "computed" value the kernel does not have.
        state.errors.push(CheckError::ArithNotExact {
            op: op.as_str(),
            operands: operands.to_vec(),
        });
        return None;
    };
    // Exact equality, because the values are exact. There is no tolerance left
    // to tune and nothing for a rounding difference to hide behind.
    if computed != claimed {
        state.errors.push(CheckError::ArithMismatch {
            op: op.as_str(),
            operands: operands.to_vec(),
            claimed,
            computed,
        });
        return None;
    }
    let judgment = Judgment::Holds {
        proposition: proposition.clone(),
    };
    state.discharge(
        "arith",
        &judgment,
        format!("{} of {operands:?} recomputed as {computed}", op.as_str()),
    );
    Some(judgment)
}

/// Discharges nothing on purpose.
///
/// The premise is still checked — a reading of a fabricated citation is
/// rejected, not merely flagged — but the reading itself is recorded as an
/// obligation for a human or a stronger model.
fn derive_interpret(
    premise: &Rule,
    proposition: &Proposition,
    rationale: &str,
    snapshot: &Snapshot,
    state: &mut CheckState,
    depth: u32,
) -> Option<Judgment> {
    let inner = derive(premise, snapshot, state, depth + 1)?;

    // A residual is a question put to a person, and one that does not say what
    // was read or why is not a question — it is a blank the reviewer is asked to
    // sign. The rule that discharges nothing has to at least say what it did.
    if rationale.trim().is_empty() {
        state
            .errors
            .push(CheckError::InterpretationWithoutARationale {
                from: inner.canonical(),
                to: proposition.canonical(),
            });
        return None;
    }

    state.residuals.push(Residual {
        from: inner.canonical(),
        to: proposition.clone(),
        rationale: rationale.to_string(),
        evidence_ids: cited_evidence_ids(premise),
    });
    Some(Judgment::Holds {
        proposition: proposition.clone(),
    })
}

/// Evidence a derivation cites, so a residual points at what to re-read.
fn cited_evidence_ids(rule: &Rule) -> Vec<String> {
    let mut ids = Vec::new();
    collect_evidence_ids(rule, &mut ids);
    ids
}

fn collect_evidence_ids(rule: &Rule, out: &mut Vec<String>) {
    match rule {
        Rule::Cite { evidence_id, .. } => out.push(evidence_id.clone()),
        Rule::Trust { premise, .. } | Rule::Interpret { premise, .. } => {
            collect_evidence_ids(premise, out);
        }
        Rule::Attest { .. } | Rule::Arith { .. } => {}
    }
}

/// Whether the excerpt contains the object, ignoring case and run-length of
/// whitespace.
///
/// Models reflow whitespace and change case when quoting; neither changes what
/// the source says, so neither should reject a citation. Anything beyond that —
/// paraphrase, synonymy, inference — deliberately does not pass here.
fn contains_normalized(excerpt: &str, object: &str) -> bool {
    let needle = normalize(object);
    if needle.is_empty() {
        return false;
    }
    normalize(excerpt).contains(&needle)
}

/// The form two pieces of text are compared in.
///
/// Three forgivenesses, and no more. Whitespace run-length, because models
/// reflow quotations. Case, because they change it — `str::to_lowercase` is a
/// full Unicode mapping, not an ASCII one, so Turkish dotted I and Greek final
/// sigma fold correctly without help. And Unicode composition, because "é"
/// written as one codepoint and as "e" plus a combining acute are the same text
/// by any reading a person would give it, and a citation that turned on which
/// encoding the quoter happened to use would be rejecting a true statement
/// about the document.
///
/// NFC rather than NFD: it is the form the IR's canonical encoding already
/// uses, and two layers that normalise differently would disagree about whether
/// the same document says the same thing.
fn normalize(value: &str) -> String {
    use unicode_normalization::UnicodeNormalization;

    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .nfc()
        .collect()
}

fn parse_authority(value: &str) -> Option<Authority> {
    match value.trim().to_lowercase().as_str() {
        "low" => Some(Authority::Low),
        "medium" => Some(Authority::Medium),
        "high" => Some(Authority::High),
        _ => None,
    }
}

const fn authority_rank(authority: Authority) -> u8 {
    match authority {
        Authority::Low => 0,
        Authority::Medium => 1,
        Authority::High => 2,
    }
}

/// Digest over the proposal, so a certificate names the exact input it decided.
/// The digest that ties a certificate to the exact proposal it decided.
///
/// Over the IR's canonical bytes, not `serde_json`. Two encoders that disagree
/// about byte order or number formatting produce two digests for one proposal,
/// and the digest is the thing that says which proposal this was.
///
/// It used to be `serde_json::to_string(proposal).unwrap_or_default()`, which
/// had a sharper edge than the encoder choice: on failure it digested the empty
/// string, so *every* proposal that would not serialize shared one digest. The
/// reachable case was a float — `serde_json` refuses `NaN` — which the exact
/// quantities have since removed. The `Result` is handled either way, because a
/// proposal that cannot be encoded cannot be pinned, and pretending otherwise is
/// how the collision existed in the first place.
fn replay_digest(proposal: &Proposal) -> Result<String, capsulet_ir::CanonicalError> {
    capsulet_ir::digest_of(proposal).map(|digest| digest.to_string())
}

#[cfg(test)]
mod tests;
