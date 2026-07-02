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

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Evidence {
    id: EvidenceId,
    scope: MemoryScope,
    source_id: SourceId,
    locator: String,
    excerpt: String,
    observed_at: String,
}

impl Evidence {
    /// Creates an evidence record linked to a source location or excerpt.
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
        })
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
}

fn non_empty(value: String, field: &'static str) -> Result<String, MemoryError> {
    if value.trim().is_empty() {
        return Err(MemoryError::EmptyField { field });
    }
    Ok(value)
}
