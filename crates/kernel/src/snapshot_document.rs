//! The serializable form of a snapshot.
//!
//! A certificate that says "the claim-reasoning family decided this" is only
//! checkable by someone else if they can rebuild the inputs it decided from.
//! The domain records in `capsulet-core` are constructed through validating
//! constructors and deliberately have no serde derives, so this module is the
//! wire form: exactly the fields the kernel reads, and nothing else.
//!
//! Rebuilding goes back through the same constructors the rest of the system
//! uses, so a document cannot smuggle in a record the domain would have
//! rejected — an empty excerpt, an inverted span, a claim with no evidence.

use capsulet_core::{
    Authority, Claim, ClaimId, ClaimStatus, Confidence, EntityId, Evidence, EvidenceId,
    EvidenceSpan, MemoryScope, Source, SourceContent, SourceId,
};
use serde::{Deserialize, Serialize};

use crate::snapshot::Snapshot;

/// Why a snapshot document could not be rebuilt.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RebuildError {
    pub detail: String,
}

impl RebuildError {
    fn from(detail: impl std::fmt::Display) -> Self {
        Self {
            detail: detail.to_string(),
        }
    }
}

/// A source, as pinned in a bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourceDocument {
    pub id: String,
    pub tenant_id: String,
    pub project_id: String,
    pub kind: String,
    pub uri: Option<String>,
    pub title: String,
    /// `low`, `medium`, or `high`.
    pub authority: String,
    /// The exact text spans were taken from.
    pub content: String,
}

/// A piece of evidence, as pinned in a bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceDocument {
    pub id: String,
    pub tenant_id: String,
    pub project_id: String,
    pub source_id: String,
    pub locator: String,
    pub excerpt: String,
    pub observed_at: String,
    pub span_start: Option<usize>,
    pub span_end: Option<usize>,
    pub source_content_hash: Option<String>,
}

/// A claim, as pinned in a bundle.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimDocument {
    pub id: String,
    pub tenant_id: String,
    pub project_id: String,
    pub subject_id: String,
    pub predicate: String,
    pub object: String,
    pub evidence_ids: Vec<String>,
    /// Carried as text so no float reaches a digest.
    pub confidence: String,
    pub authority: String,
    pub observed_at: String,
    pub status: String,
}

/// Everything a claim-reasoning decision was made against.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SnapshotDocument {
    pub sources: Vec<SourceDocument>,
    pub evidence: Vec<EvidenceDocument>,
    pub claims: Vec<ClaimDocument>,
}

impl SnapshotDocument {
    /// Rebuilds the snapshot, through the domain's own constructors.
    ///
    /// # Errors
    ///
    /// Returns [`RebuildError`] when any record would not have been valid in
    /// the first place, or when a field does not parse.
    pub fn rebuild(&self) -> Result<Snapshot, RebuildError> {
        let mut snapshot = Snapshot::new();

        for source in &self.sources {
            let id = SourceId::new(&source.id).map_err(RebuildError::from)?;
            let scope = scope(&source.tenant_id, &source.project_id)?;
            snapshot = snapshot.with_source(
                Source::new(
                    id.clone(),
                    scope,
                    &source.kind,
                    source.uri.clone(),
                    &source.title,
                    authority(&source.authority)?,
                )
                .map_err(RebuildError::from)?,
            );
            snapshot = snapshot.with_source_content(
                SourceContent::new(id, &source.content).map_err(RebuildError::from)?,
            );
        }

        for evidence in &self.evidence {
            let mut record = Evidence::new(
                EvidenceId::new(&evidence.id).map_err(RebuildError::from)?,
                scope(&evidence.tenant_id, &evidence.project_id)?,
                SourceId::new(&evidence.source_id).map_err(RebuildError::from)?,
                &evidence.locator,
                &evidence.excerpt,
                &evidence.observed_at,
            )
            .map_err(RebuildError::from)?;

            if let (Some(start), Some(end), Some(hash)) = (
                evidence.span_start,
                evidence.span_end,
                evidence.source_content_hash.as_ref(),
            ) {
                record = record
                    .with_span(EvidenceSpan::new(start, end, hash).map_err(RebuildError::from)?);
            }
            snapshot = snapshot.with_evidence(record);
        }

        for claim in &self.claims {
            let evidence_ids = claim
                .evidence_ids
                .iter()
                .map(|id| EvidenceId::new(id).map_err(RebuildError::from))
                .collect::<Result<Vec<_>, _>>()?;
            let confidence: f64 = claim
                .confidence
                .parse()
                .map_err(|_| RebuildError::from("confidence is not a number"))?;

            let record = Claim::new(
                ClaimId::new(&claim.id).map_err(RebuildError::from)?,
                scope(&claim.tenant_id, &claim.project_id)?,
                EntityId::new(&claim.subject_id).map_err(RebuildError::from)?,
                &claim.predicate,
                &claim.object,
                evidence_ids,
                Confidence::new(confidence).map_err(RebuildError::from)?,
                authority(&claim.authority)?,
                &claim.observed_at,
                None,
                None,
            )
            .map_err(RebuildError::from)?;

            snapshot = snapshot.with_claim(record.with_status(status(&claim.status)?));
        }

        Ok(snapshot)
    }
}

fn scope(tenant: &str, project: &str) -> Result<MemoryScope, RebuildError> {
    MemoryScope::new(tenant, project).map_err(RebuildError::from)
}

fn authority(value: &str) -> Result<Authority, RebuildError> {
    match value {
        "low" => Ok(Authority::Low),
        "medium" => Ok(Authority::Medium),
        "high" => Ok(Authority::High),
        other => Err(RebuildError::from(format!("unknown authority `{other}`"))),
    }
}

fn status(value: &str) -> Result<ClaimStatus, RebuildError> {
    match value {
        "candidate" => Ok(ClaimStatus::Candidate),
        "active" => Ok(ClaimStatus::Active),
        "rejected" => Ok(ClaimStatus::Rejected),
        "superseded" => Ok(ClaimStatus::Superseded),
        "contradicted" => Ok(ClaimStatus::Contradicted),
        "expired" => Ok(ClaimStatus::Expired),
        other => Err(RebuildError::from(format!(
            "unknown claim status `{other}`"
        ))),
    }
}
