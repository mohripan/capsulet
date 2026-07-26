use std::fmt::{self, Display};

use thiserror::Error;

use super::{ClaimId, EntityId, EventId, EvidenceId, ObservationId, RelationshipId, SourceId};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemoryScope {
    tenant_id: String,
    project_id: String,
}

impl MemoryScope {
    /// Creates a tenant/project memory scope.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError`] when either scope part is empty.
    pub fn new(
        tenant_id: impl Into<String>,
        project_id: impl Into<String>,
    ) -> Result<Self, MemoryError> {
        let tenant_id = non_empty(tenant_id.into(), "tenant id")?;
        let project_id = non_empty(project_id.into(), "project id")?;
        Ok(Self {
            tenant_id,
            project_id,
        })
    }

    #[must_use]
    pub fn tenant_id(&self) -> &str {
        &self.tenant_id
    }

    #[must_use]
    pub fn project_id(&self) -> &str {
        &self.project_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Confidence(f64);

impl Confidence {
    /// Creates a confidence score in the inclusive range `0.0..=1.0`.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError`] when the score is not finite or outside the
    /// accepted range.
    pub fn new(value: f64) -> Result<Self, MemoryError> {
        if !value.is_finite() || !(0.0..=1.0).contains(&value) {
            return Err(MemoryError::InvalidConfidence);
        }
        Ok(Self(value))
    }

    #[must_use]
    pub const fn value(self) -> f64 {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Authority {
    Low,
    Medium,
    High,
}

impl Display for Authority {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClaimStatus {
    Candidate,
    Active,
    Rejected,
    Superseded,
    Contradicted,
    Expired,
}

impl Display for ClaimStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Candidate => "candidate",
            Self::Active => "active",
            Self::Rejected => "rejected",
            Self::Superseded => "superseded",
            Self::Contradicted => "contradicted",
            Self::Expired => "expired",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Source {
    id: SourceId,
    scope: MemoryScope,
    kind: String,
    uri: Option<String>,
    title: String,
    authority: Authority,
}

impl Source {
    /// Creates a source that evidence can cite.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError`] when required text fields are empty.
    pub fn new(
        id: SourceId,
        scope: MemoryScope,
        kind: impl Into<String>,
        uri: Option<String>,
        title: impl Into<String>,
        authority: Authority,
    ) -> Result<Self, MemoryError> {
        Ok(Self {
            id,
            scope,
            kind: non_empty(kind.into(), "source kind")?,
            uri,
            title: non_empty(title.into(), "source title")?,
            authority,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &SourceId {
        &self.id
    }

    #[must_use]
    pub const fn scope(&self) -> &MemoryScope {
        &self.scope
    }

    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }

    #[must_use]
    pub fn uri(&self) -> Option<&str> {
        self.uri.as_deref()
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub const fn authority(&self) -> Authority {
        self.authority
    }
}

/// Immutable stored text of a source, addressed by the digest of its bytes.
///
/// Evidence spans are byte offsets into this text. Storing it separately from
/// [`Source`] keeps the metadata mutable while the bytes a citation resolves
/// against stay fixed: re-ingesting a changed document produces a new digest,
/// which invalidates the spans that cited the old one rather than silently
/// repointing them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceContent {
    source_id: SourceId,
    text: String,
    content_hash: String,
}

impl SourceContent {
    /// Creates stored source content and derives its digest from the bytes.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError`] when the text is empty.
    pub fn new(source_id: SourceId, text: impl Into<String>) -> Result<Self, MemoryError> {
        let text = non_empty(text.into(), "source content")?;
        let content_hash = content_digest(text.as_bytes());
        Ok(Self {
            source_id,
            text,
            content_hash,
        })
    }

    #[must_use]
    pub const fn source_id(&self) -> &SourceId {
        &self.source_id
    }

    #[must_use]
    pub fn text(&self) -> &str {
        &self.text
    }

    #[must_use]
    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }

    #[must_use]
    pub const fn byte_length(&self) -> usize {
        self.text.len()
    }
}

/// A byte range into a specific version of a source's text.
///
/// The digest is part of the span, not looked up at check time, so a span
/// always names the bytes it was taken from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceSpan {
    start: usize,
    end: usize,
    source_content_hash: String,
}

impl EvidenceSpan {
    /// Creates a byte span pinned to one version of a source's text.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError`] when the range is empty or inverted, or when the
    /// digest is missing.
    pub fn new(
        start: usize,
        end: usize,
        source_content_hash: impl Into<String>,
    ) -> Result<Self, MemoryError> {
        if end <= start {
            return Err(MemoryError::InvalidSpan { start, end });
        }
        Ok(Self {
            start,
            end,
            source_content_hash: non_empty(
                source_content_hash.into(),
                "evidence source content hash",
            )?,
        })
    }

    #[must_use]
    pub const fn start(&self) -> usize {
        self.start
    }

    #[must_use]
    pub const fn end(&self) -> usize {
        self.end
    }

    #[must_use]
    pub fn source_content_hash(&self) -> &str {
        &self.source_content_hash
    }
}

/// Why a citation could not be re-derived from its source.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ProvenanceError {
    #[error("evidence {evidence_id} carries no span, so its excerpt cannot be re-derived")]
    SpanMissing { evidence_id: String },
    #[error("evidence {evidence_id} cites source {cited} but was checked against {actual}")]
    SourceMismatch {
        evidence_id: String,
        cited: String,
        actual: String,
    },
    #[error(
        "evidence {evidence_id} was taken from source content {expected} but the stored content is {actual}"
    )]
    ContentChanged {
        evidence_id: String,
        expected: String,
        actual: String,
    },
    #[error(
        "evidence {evidence_id} spans bytes {start}..{end} but the source content is {length} bytes"
    )]
    SpanOutOfBounds {
        evidence_id: String,
        start: usize,
        end: usize,
        length: usize,
    },
    #[error("evidence {evidence_id} spans bytes {start}..{end}, which is not a character boundary")]
    SpanNotOnCharBoundary {
        evidence_id: String,
        start: usize,
        end: usize,
    },
    #[error("evidence {evidence_id} excerpt does not match the bytes at its span")]
    ExcerptMismatch {
        evidence_id: String,
        expected: String,
        found: String,
    },
}

/// Re-derives an excerpt from stored source content.
///
/// This is the check that turns provenance from an assertion into a fact. It is
/// deliberately total and free of I/O so the kernel can run it.
///
/// # Errors
///
/// Returns [`ProvenanceError`] when the evidence has no span, when the content
/// has changed since the span was taken, when the range is out of bounds or off
/// a character boundary, or when the bytes do not match the recorded excerpt.
pub fn verify_evidence_span(
    evidence: &Evidence,
    content: &SourceContent,
) -> Result<(), ProvenanceError> {
    let evidence_id = evidence.id().as_str().to_string();
    if evidence.source_id() != content.source_id() {
        return Err(ProvenanceError::SourceMismatch {
            evidence_id,
            cited: evidence.source_id().as_str().to_string(),
            actual: content.source_id().as_str().to_string(),
        });
    }
    let Some(span) = evidence.span() else {
        return Err(ProvenanceError::SpanMissing { evidence_id });
    };
    if span.source_content_hash() != content.content_hash() {
        return Err(ProvenanceError::ContentChanged {
            evidence_id,
            expected: span.source_content_hash().to_string(),
            actual: content.content_hash().to_string(),
        });
    }
    let text = content.text();
    if span.end() > text.len() {
        return Err(ProvenanceError::SpanOutOfBounds {
            evidence_id,
            start: span.start(),
            end: span.end(),
            length: text.len(),
        });
    }
    if !text.is_char_boundary(span.start()) || !text.is_char_boundary(span.end()) {
        return Err(ProvenanceError::SpanNotOnCharBoundary {
            evidence_id,
            start: span.start(),
            end: span.end(),
        });
    }
    let found = &text[span.start()..span.end()];
    if found != evidence.excerpt() {
        return Err(ProvenanceError::ExcerptMismatch {
            evidence_id,
            expected: evidence.excerpt().to_string(),
            found: found.to_string(),
        });
    }
    Ok(())
}

/// Hex-encoded SHA-256 of the given bytes.
#[must_use]
pub fn content_digest(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(digest.len() * 2);
    for byte in digest {
        use std::fmt::Write as _;
        let _ = write!(out, "{byte:02x}");
    }
    out
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    id: EvidenceId,
    scope: MemoryScope,
    source_id: SourceId,
    locator: String,
    excerpt: String,
    observed_at: String,
    span: Option<EvidenceSpan>,
}

impl Evidence {
    /// Creates an evidence record linked to a source location or excerpt.
    ///
    /// Evidence created this way carries no span and therefore cannot ground a
    /// claim in the kernel. Use [`Evidence::with_span`] for citable evidence.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError`] when required fields are empty.
    pub fn new(
        id: EvidenceId,
        scope: MemoryScope,
        source_id: SourceId,
        locator: impl Into<String>,
        excerpt: impl Into<String>,
        observed_at: impl Into<String>,
    ) -> Result<Self, MemoryError> {
        Ok(Self {
            id,
            scope,
            source_id,
            locator: non_empty(locator.into(), "evidence locator")?,
            excerpt: non_empty(excerpt.into(), "evidence excerpt")?,
            observed_at: non_empty(observed_at.into(), "observed at")?,
            span: None,
        })
    }

    /// Attaches a byte span pinning this excerpt to a version of its source.
    #[must_use]
    pub fn with_span(mut self, span: EvidenceSpan) -> Self {
        self.span = Some(span);
        self
    }

    /// Attaches an optional span, for reconstructing records from storage.
    #[must_use]
    pub fn with_optional_span(mut self, span: Option<EvidenceSpan>) -> Self {
        self.span = span;
        self
    }

    #[must_use]
    pub const fn span(&self) -> Option<&EvidenceSpan> {
        self.span.as_ref()
    }

    #[must_use]
    pub const fn id(&self) -> &EvidenceId {
        &self.id
    }

    #[must_use]
    pub const fn scope(&self) -> &MemoryScope {
        &self.scope
    }

    #[must_use]
    pub const fn source_id(&self) -> &SourceId {
        &self.source_id
    }

    #[must_use]
    pub fn locator(&self) -> &str {
        &self.locator
    }

    #[must_use]
    pub fn excerpt(&self) -> &str {
        &self.excerpt
    }

    #[must_use]
    pub fn observed_at(&self) -> &str {
        &self.observed_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entity {
    id: EntityId,
    scope: MemoryScope,
    type_name: String,
    name: String,
    aliases: Vec<String>,
}

impl Entity {
    /// Creates a typed memory entity.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError`] when required fields are empty.
    pub fn new(
        id: EntityId,
        scope: MemoryScope,
        entity_type: impl Into<String>,
        name: impl Into<String>,
        aliases: Vec<String>,
    ) -> Result<Self, MemoryError> {
        Ok(Self {
            id,
            scope,
            type_name: non_empty(entity_type.into(), "entity type")?,
            name: non_empty(name.into(), "entity name")?,
            aliases,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &EntityId {
        &self.id
    }

    #[must_use]
    pub const fn scope(&self) -> &MemoryScope {
        &self.scope
    }

    #[must_use]
    pub fn entity_type(&self) -> &str {
        &self.type_name
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Claim {
    id: ClaimId,
    scope: MemoryScope,
    subject_id: EntityId,
    predicate: String,
    object: String,
    evidence_ids: Vec<EvidenceId>,
    confidence: Confidence,
    authority: Authority,
    status: ClaimStatus,
    observed_at: String,
    valid_from: Option<String>,
    valid_until: Option<String>,
}

impl Claim {
    /// Creates an evidence-backed claim.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError`] when required fields are empty, confidence is
    /// invalid, or no evidence is supplied.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: ClaimId,
        scope: MemoryScope,
        subject_id: EntityId,
        predicate: impl Into<String>,
        object: impl Into<String>,
        evidence_ids: Vec<EvidenceId>,
        confidence: Confidence,
        authority: Authority,
        observed_at: impl Into<String>,
        valid_from: Option<&str>,
        valid_until: Option<&str>,
    ) -> Result<Self, MemoryError> {
        if evidence_ids.is_empty() {
            return Err(MemoryError::MissingEvidence);
        }
        Ok(Self {
            id,
            scope,
            subject_id,
            predicate: non_empty(predicate.into(), "claim predicate")?,
            object: non_empty(object.into(), "claim object")?,
            evidence_ids,
            confidence,
            authority,
            status: ClaimStatus::Candidate,
            observed_at: non_empty(observed_at.into(), "observed at")?,
            valid_from: valid_from.map(str::to_string),
            valid_until: valid_until.map(str::to_string),
        })
    }

    #[must_use]
    pub fn with_status(mut self, status: ClaimStatus) -> Self {
        self.status = status;
        self
    }

    #[must_use]
    pub fn conflicts_with(&self, other: &Self) -> bool {
        self.scope == other.scope
            && self.subject_id == other.subject_id
            && self.predicate == other.predicate
            && self.object != other.object
            && self.status == ClaimStatus::Candidate
            && other.status == ClaimStatus::Candidate
    }

    #[must_use]
    pub const fn id(&self) -> &ClaimId {
        &self.id
    }

    #[must_use]
    pub const fn scope(&self) -> &MemoryScope {
        &self.scope
    }

    #[must_use]
    pub const fn subject_id(&self) -> &EntityId {
        &self.subject_id
    }

    #[must_use]
    pub fn predicate(&self) -> &str {
        &self.predicate
    }

    #[must_use]
    pub fn object(&self) -> &str {
        &self.object
    }

    #[must_use]
    pub fn evidence_ids(&self) -> &[EvidenceId] {
        &self.evidence_ids
    }

    #[must_use]
    pub const fn confidence(&self) -> Confidence {
        self.confidence
    }

    #[must_use]
    pub const fn authority(&self) -> Authority {
        self.authority
    }

    #[must_use]
    pub const fn status(&self) -> ClaimStatus {
        self.status
    }

    #[must_use]
    pub fn observed_at(&self) -> &str {
        &self.observed_at
    }

    #[must_use]
    pub fn valid_from(&self) -> Option<&str> {
        self.valid_from.as_deref()
    }

    #[must_use]
    pub fn valid_until(&self) -> Option<&str> {
        self.valid_until.as_deref()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    id: EventId,
    scope: MemoryScope,
    type_name: String,
    occurred_at: String,
    entity_ids: Vec<EntityId>,
    evidence_ids: Vec<EvidenceId>,
}

impl Event {
    /// Creates a temporal memory event backed by evidence.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError`] when required fields are empty or evidence is
    /// missing.
    pub fn new(
        id: EventId,
        scope: MemoryScope,
        event_type: impl Into<String>,
        occurred_at: impl Into<String>,
        entity_ids: Vec<EntityId>,
        evidence_ids: Vec<EvidenceId>,
    ) -> Result<Self, MemoryError> {
        if evidence_ids.is_empty() {
            return Err(MemoryError::MissingEvidence);
        }
        Ok(Self {
            id,
            scope,
            type_name: non_empty(event_type.into(), "event type")?,
            occurred_at: non_empty(occurred_at.into(), "occurred at")?,
            entity_ids,
            evidence_ids,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &EventId {
        &self.id
    }

    #[must_use]
    pub const fn scope(&self) -> &MemoryScope {
        &self.scope
    }

    #[must_use]
    pub fn event_type(&self) -> &str {
        &self.type_name
    }

    #[must_use]
    pub fn occurred_at(&self) -> &str {
        &self.occurred_at
    }

    #[must_use]
    pub fn entity_ids(&self) -> &[EntityId] {
        &self.entity_ids
    }

    #[must_use]
    pub fn evidence_ids(&self) -> &[EvidenceId] {
        &self.evidence_ids
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Relationship {
    id: RelationshipId,
    scope: MemoryScope,
    type_name: String,
    from_entity_id: EntityId,
    to_entity_id: EntityId,
    evidence_ids: Vec<EvidenceId>,
}

impl Relationship {
    /// Creates an evidence-backed relationship between entities.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError`] when required fields are empty or evidence is
    /// missing.
    pub fn new(
        id: RelationshipId,
        scope: MemoryScope,
        relationship_type: impl Into<String>,
        from_entity_id: EntityId,
        to_entity_id: EntityId,
        evidence_ids: Vec<EvidenceId>,
    ) -> Result<Self, MemoryError> {
        if evidence_ids.is_empty() {
            return Err(MemoryError::MissingEvidence);
        }
        Ok(Self {
            id,
            scope,
            type_name: non_empty(relationship_type.into(), "relationship type")?,
            from_entity_id,
            to_entity_id,
            evidence_ids,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &RelationshipId {
        &self.id
    }

    #[must_use]
    pub const fn scope(&self) -> &MemoryScope {
        &self.scope
    }

    #[must_use]
    pub fn relationship_type(&self) -> &str {
        &self.type_name
    }

    #[must_use]
    pub const fn from_entity_id(&self) -> &EntityId {
        &self.from_entity_id
    }

    #[must_use]
    pub const fn to_entity_id(&self) -> &EntityId {
        &self.to_entity_id
    }

    #[must_use]
    pub fn evidence_ids(&self) -> &[EvidenceId] {
        &self.evidence_ids
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Observation {
    id: ObservationId,
    scope: MemoryScope,
    observed_at: String,
    evidence_id: EvidenceId,
    note: String,
}

impl Observation {
    /// Creates an observation that records when evidence entered memory.
    ///
    /// # Errors
    ///
    /// Returns [`MemoryError`] when required fields are empty.
    pub fn new(
        id: ObservationId,
        scope: MemoryScope,
        observed_at: impl Into<String>,
        evidence_id: EvidenceId,
        note: impl Into<String>,
    ) -> Result<Self, MemoryError> {
        Ok(Self {
            id,
            scope,
            observed_at: non_empty(observed_at.into(), "observed at")?,
            evidence_id,
            note: non_empty(note.into(), "observation note")?,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &ObservationId {
        &self.id
    }

    #[must_use]
    pub const fn scope(&self) -> &MemoryScope {
        &self.scope
    }

    #[must_use]
    pub fn observed_at(&self) -> &str {
        &self.observed_at
    }

    #[must_use]
    pub const fn evidence_id(&self) -> &EvidenceId {
        &self.evidence_id
    }

    #[must_use]
    pub fn note(&self) -> &str {
        &self.note
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MemoryError {
    #[error("{field} cannot be empty")]
    EmptyField { field: &'static str },
    #[error("confidence must be finite and between 0.0 and 1.0")]
    InvalidConfidence,
    #[error("memory claims, events, and relationships require evidence")]
    MissingEvidence,
    #[error("evidence span {start}..{end} must be a non-empty forward range")]
    InvalidSpan { start: usize, end: usize },
}

fn non_empty(value: String, field: &'static str) -> Result<String, MemoryError> {
    if value.trim().is_empty() {
        return Err(MemoryError::EmptyField { field });
    }
    Ok(value)
}

#[cfg(test)]
mod provenance_tests {
    use super::{
        Evidence, EvidenceSpan, MemoryScope, ProvenanceError, SourceContent, verify_evidence_span,
    };
    use crate::domain::{EvidenceId, SourceId};

    const DOC: &str = "Acme renewed the Contoso contract on 2026-03-01. Notice is 30 days.";

    fn scope() -> MemoryScope {
        MemoryScope::new("acme", "prod").expect("scope")
    }

    fn content() -> SourceContent {
        SourceContent::new(SourceId::new("src_1").expect("source id"), DOC).expect("content")
    }

    fn cited(excerpt: &str, start: usize, end: usize, hash: &str) -> Evidence {
        Evidence::new(
            EvidenceId::new("ev_1").expect("evidence id"),
            scope(),
            SourceId::new("src_1").expect("source id"),
            "para-1",
            excerpt,
            "2026-03-02T00:00:00Z",
        )
        .expect("evidence")
        .with_span(EvidenceSpan::new(start, end, hash).expect("span"))
    }

    #[test]
    fn accepts_an_excerpt_that_re_derives_from_its_source() {
        let content = content();
        let evidence = cited(
            "Acme renewed the Contoso contract",
            0,
            33,
            content.content_hash(),
        );

        verify_evidence_span(&evidence, &content).expect("span verifies");
    }

    #[test]
    fn rejects_an_excerpt_that_is_not_the_bytes_at_the_span() {
        let content = content();
        // The span covers real text, but the excerpt claims something else — the
        // shape a fabricated citation takes.
        let evidence = cited(
            "Acme terminated the Contoso contract",
            0,
            33,
            content.content_hash(),
        );

        let error = verify_evidence_span(&evidence, &content).expect_err("must reject");

        assert!(matches!(error, ProvenanceError::ExcerptMismatch { .. }));
    }

    #[test]
    fn rejects_a_span_taken_from_a_different_version_of_the_source() {
        let content = content();
        let evidence = cited("Acme renewed the Contoso contract", 0, 33, "stale-digest");

        let error = verify_evidence_span(&evidence, &content).expect_err("must reject");

        assert!(matches!(error, ProvenanceError::ContentChanged { .. }));
    }

    #[test]
    fn rejects_a_span_past_the_end_of_the_source() {
        let content = content();
        let evidence = cited("whatever", 0, 10_000, content.content_hash());

        let error = verify_evidence_span(&evidence, &content).expect_err("must reject");

        assert!(matches!(error, ProvenanceError::SpanOutOfBounds { .. }));
    }

    #[test]
    fn rejects_evidence_that_carries_no_span() {
        let content = content();
        let evidence = Evidence::new(
            EvidenceId::new("ev_1").expect("evidence id"),
            scope(),
            SourceId::new("src_1").expect("source id"),
            "para-1",
            "Acme renewed the Contoso contract",
            "2026-03-02T00:00:00Z",
        )
        .expect("evidence");

        let error = verify_evidence_span(&evidence, &content).expect_err("must reject");

        assert!(matches!(error, ProvenanceError::SpanMissing { .. }));
    }

    #[test]
    fn rejects_a_span_that_splits_a_character() {
        let text = "café renewed";
        let content =
            SourceContent::new(SourceId::new("src_1").expect("source id"), text).expect("content");
        // 'é' occupies bytes 3..5, so 4 lands mid-character.
        let evidence = cited("caf?", 0, 4, content.content_hash());

        let error = verify_evidence_span(&evidence, &content).expect_err("must reject");

        assert!(matches!(
            error,
            ProvenanceError::SpanNotOnCharBoundary { .. }
        ));
    }

    #[test]
    fn rejects_evidence_checked_against_another_source() {
        let other =
            SourceContent::new(SourceId::new("src_2").expect("source id"), DOC).expect("content");
        let evidence = cited(
            "Acme renewed the Contoso contract",
            0,
            33,
            other.content_hash(),
        );

        let error = verify_evidence_span(&evidence, &other).expect_err("must reject");

        assert!(matches!(error, ProvenanceError::SourceMismatch { .. }));
    }

    #[test]
    fn content_hash_changes_when_the_source_text_changes() {
        let first = content();
        let second = SourceContent::new(
            SourceId::new("src_1").expect("source id"),
            "Acme renewed the Contoso contract on 2026-03-02. Notice is 30 days.",
        )
        .expect("content");

        assert_ne!(first.content_hash(), second.content_hash());
    }
}
