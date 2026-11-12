//! A person's decision on a reading the kernel could not make.
//!
//! A residual is a question put to a person: does this passage mean this, does
//! this question ask that. Until someone answers it, a certificate carrying one
//! is `conditional` and says exactly what to review. This module is what an
//! answer does to it.
//!
//! A review is bound to the exact reading it decided by a digest over the
//! residual's canonical bytes, so a decision about one reading can never be
//! applied to another, or to the same words in another certificate. It is made
//! under an authority, and only [`READING_AUTHORITY`] counts. The outcome is a
//! pure function of the certificate and the reviews recorded against it, so it
//! is re-derived, never stored, and a replay of the same records reaches it
//! again.
//!
//! A review changes what is known about a reading, not what the kernel checked.
//! A `rejected` certificate stays rejected, because a reviewer's word cannot
//! repair a premise that failed: a citation that did not re-derive, a sum that
//! did not add up. An `accepted` outcome reached by review says so: its basis is
//! `human_review`, never `mechanical` (ADR 0021).

use serde::{Deserialize, Serialize};

use crate::certificate::{Certificate, Residual, Verdict};

/// The authority a reading review is made under.
///
/// The scope a principal needs to record one. Recorded on every review, so an
/// outcome says whose judgement it rests on, and a review under any other
/// authority decides nothing.
pub const READING_AUTHORITY: &str = "review:write";

/// What a reviewer decided about one reading.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReadingDecision {
    /// The passage means what the reading says, or the question asks it.
    Holds,
    /// It does not.
    DoesNotHold,
}

impl ReadingDecision {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Holds => "holds",
            Self::DoesNotHold => "does_not_hold",
        }
    }

    /// Reads a stored decision.
    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "holds" => Some(Self::Holds),
            "does_not_hold" => Some(Self::DoesNotHold),
            _ => None,
        }
    }
}

/// A reviewer's decision, bound to the reading it decided.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReadingReview {
    /// [`reading_digest`] of the residual decided.
    pub reading: String,
    pub decision: ReadingDecision,
    /// Who decided.
    pub reviewer: String,
    /// The authority they decided under.
    pub authority: String,
    /// Why, for whoever reads the outcome.
    pub rationale: String,
}

/// The digest that names one reading.
///
/// Over the residual's canonical bytes, together with the certificate's replay
/// digest: the same words read from another proposal are another reading.
#[must_use]
pub fn reading_digest(certificate: &Certificate, residual: &Residual) -> String {
    #[derive(Serialize)]
    struct Named<'a> {
        certificate: &'a str,
        residual: &'a Residual,
    }
    capsulet_ir::digest_of(&Named {
        certificate: &certificate.replay_digest,
        residual,
    })
    .map_or_else(|_| String::new(), |digest| digest.to_string())
}

/// Where one reading stands.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReadingState {
    /// Nobody with the authority has decided it.
    Open,
    /// Decided, by the review recorded.
    Decided(ReadingReview),
}

/// A certificate's outcome once the reviews recorded against it are read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reviewed {
    /// What the certificate and its reviews together support.
    pub verdict: Verdict,
    /// Whether `accepted` rests on review, rather than on the kernel alone.
    pub by_review: bool,
    /// Each residual, its digest, and where it stands, in the certificate's order.
    pub readings: Vec<(String, ReadingState)>,
}

/// The outcome the certificate and its reviews support.
///
/// - `rejected` stays `rejected`.
/// - Any reading decided `does_not_hold` rejects: the answer rests on a reading
///   a person with the authority says is wrong.
/// - Every reading decided `holds` accepts, by review.
/// - Anything else stays `conditional`, with the open readings named.
///
/// A review of a reading the certificate does not carry, or under another
/// authority, is ignored: it decides nothing here.
#[must_use]
pub fn reviewed(certificate: &Certificate, reviews: &[ReadingReview]) -> Reviewed {
    let readings: Vec<(String, ReadingState)> = certificate
        .residuals
        .iter()
        .map(|residual| {
            let digest = reading_digest(certificate, residual);
            let state = reviews
                .iter()
                .find(|review| review.reading == digest && review.authority == READING_AUTHORITY)
                .map_or(ReadingState::Open, |review| {
                    ReadingState::Decided(review.clone())
                });
            (digest, state)
        })
        .collect();

    let decided = |wanted: ReadingDecision| {
        readings.iter().filter(move |(_, state)| {
            matches!(state, ReadingState::Decided(review) if review.decision == wanted)
        })
    };
    let refuted = decided(ReadingDecision::DoesNotHold).next().is_some();
    let all_hold =
        !readings.is_empty() && decided(ReadingDecision::Holds).count() == readings.len();

    let (verdict, by_review) = match certificate.verdict {
        Verdict::Rejected => (Verdict::Rejected, false),
        Verdict::Accepted => (Verdict::Accepted, false),
        Verdict::Conditional if refuted => (Verdict::Rejected, true),
        Verdict::Conditional if all_hold => (Verdict::Accepted, true),
        Verdict::Conditional => (Verdict::Conditional, false),
    };
    Reviewed {
        verdict,
        by_review,
        readings,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::certificate::ResidualKind;
    use crate::ir::Proposition;

    fn certificate(verdict: Verdict, readings: usize) -> Certificate {
        Certificate {
            verdict,
            goal: Proposition::new("either party", "may_terminate", "this agreement"),
            discharged: Vec::new(),
            residuals: (0..readings)
                .map(|index| Residual {
                    kind: ResidualKind::Source,
                    from: "mentions(contract,either party|may_terminate|this agreement)"
                        .to_string(),
                    to: Proposition::new("either party", "may_terminate", "this agreement"),
                    rationale: format!("reading {index}"),
                    evidence_ids: vec!["ev_termination".to_string()],
                })
                .collect(),
            errors: Vec::new(),
            replay_digest: "sha256:proposal".to_string(),
            derivation_depth_limit: None,
        }
    }

    fn review(certificate: &Certificate, index: usize, decision: ReadingDecision) -> ReadingReview {
        ReadingReview {
            reading: reading_digest(certificate, &certificate.residuals[index]),
            decision,
            reviewer: "dana".to_string(),
            authority: READING_AUTHORITY.to_string(),
            rationale: "read with section 12".to_string(),
        }
    }

    #[test]
    fn every_reading_held_accepts_by_review_and_one_refuted_rejects() {
        let certificate = certificate(Verdict::Conditional, 2);
        let open = reviewed(&certificate, &[]);
        assert_eq!(open.verdict, Verdict::Conditional);
        assert!(!open.by_review);

        let one = [review(&certificate, 0, ReadingDecision::Holds)];
        assert_eq!(reviewed(&certificate, &one).verdict, Verdict::Conditional);

        let both = [
            review(&certificate, 0, ReadingDecision::Holds),
            review(&certificate, 1, ReadingDecision::Holds),
        ];
        let accepted = reviewed(&certificate, &both);
        assert_eq!(accepted.verdict, Verdict::Accepted);
        assert!(accepted.by_review);

        let refuted = [
            review(&certificate, 0, ReadingDecision::Holds),
            review(&certificate, 1, ReadingDecision::DoesNotHold),
        ];
        assert_eq!(reviewed(&certificate, &refuted).verdict, Verdict::Rejected);
    }

    #[test]
    fn a_review_decides_only_its_own_reading_under_the_reading_authority() {
        let certificate = certificate(Verdict::Conditional, 1);
        let elsewhere = {
            let mut other = certificate.clone();
            other.replay_digest = "sha256:another-proposal".to_string();
            review(&other, 0, ReadingDecision::Holds)
        };
        assert_eq!(
            reviewed(&certificate, &[elsewhere]).verdict,
            Verdict::Conditional
        );

        let mut unauthorised = review(&certificate, 0, ReadingDecision::Holds);
        unauthorised.authority = "reasoning:operate".to_string();
        assert_eq!(
            reviewed(&certificate, &[unauthorised]).verdict,
            Verdict::Conditional
        );
    }

    #[test]
    fn a_review_cannot_repair_a_rejected_certificate() {
        let mut rejected = certificate(Verdict::Rejected, 1);
        rejected.errors = Vec::new();
        let holds = [review(&rejected, 0, ReadingDecision::Holds)];
        let outcome = reviewed(&rejected, &holds);
        assert_eq!(outcome.verdict, Verdict::Rejected);
        assert!(!outcome.by_review);
    }
}
