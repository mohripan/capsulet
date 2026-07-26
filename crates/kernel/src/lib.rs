//! The correctness kernel.
//!
//! A proposer — a model, a retriever, anything untrusted — emits a [`Proposal`].
//! The kernel decides it and issues a [`Certificate`]. Nothing here is learned,
//! nothing here performs I/O, and every check is total: `check` always
//! terminates with a verdict.
//!
//! The design boundary is deliberate. Provenance, arithmetic, record state and
//! policy are mechanically decidable, so the kernel decides them. Whether a
//! passage *means* what a proposition says is not decidable over natural
//! language, so the kernel refuses to pretend: [`Rule::Interpret`] discharges
//! nothing and records a [`Residual`], which is what makes a verdict
//! [`Verdict::Conditional`] rather than [`Verdict::Accepted`].

pub mod certificate;
pub mod error;
pub mod ir;
pub mod snapshot;

use capsulet_core::{Authority, ClaimStatus, verify_evidence_span};

pub use certificate::{Certificate, CertificateError, DischargedStep, Residual, Verdict};
pub use error::{CheckError, RepairOwner};
pub use ir::{ArithOp, Judgment, Proposal, Proposition, Rule};
pub use snapshot::Snapshot;

/// Difference below which two computed numbers are considered equal.
///
/// Proposers emit decimal literals, so exact float equality would reject
/// arithmetic that is correct to every digit anyone wrote down.
const ARITH_EPSILON: f64 = 1e-9;

/// Decides a proposal against a snapshot.
///
/// Always terminates. A failure anywhere produces [`Verdict::Rejected`] with
/// the specific reasons; an otherwise sound derivation that required a reading
/// produces [`Verdict::Conditional`] with the readings recorded.
#[must_use]
pub fn check(proposal: &Proposal, snapshot: &Snapshot) -> Certificate {
    let mut state = CheckState::default();
    let outcome = derive(&proposal.derivation, snapshot, &mut state);

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
        replay_digest: replay_digest(proposal),
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

/// Evaluates one rule. Returns `None` when the step could not be taken at all,
/// after recording why.
fn derive(rule: &Rule, snapshot: &Snapshot, state: &mut CheckState) -> Option<Judgment> {
    match rule {
        Rule::Cite {
            evidence_id,
            proposition,
        } => derive_cite(evidence_id, proposition, snapshot, state),
        Rule::Attest { claim_id } => derive_attest(claim_id, snapshot, state),
        Rule::Trust {
            premise,
            min_authority,
        } => derive_trust(premise, min_authority, snapshot, state),
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
        } => derive_interpret(premise, proposition, rationale, snapshot, state),
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
) -> Option<Judgment> {
    let inner = derive(premise, snapshot, state)?;
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
    operands: &[f64],
    claimed: f64,
    proposition: &Proposition,
    state: &mut CheckState,
) -> Option<Judgment> {
    let Some(computed) = op.apply(operands) else {
        state
            .errors
            .push(CheckError::ArithNoOperands { op: op.as_str() });
        return None;
    };
    if (computed - claimed).abs() > ARITH_EPSILON {
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
) -> Option<Judgment> {
    let inner = derive(premise, snapshot, state)?;
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

fn normalize(value: &str) -> String {
    value
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
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
fn replay_digest(proposal: &Proposal) -> String {
    let serialized = serde_json::to_string(proposal).unwrap_or_default();
    capsulet_core::content_digest(serialized.as_bytes())
}

#[cfg(test)]
mod tests;
