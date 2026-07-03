#![expect(
    clippy::missing_errors_doc,
    reason = "nested memory graph constructors all return MemoryGraphError for invariant violations"
)]

use std::fmt::{self, Display};

use thiserror::Error;

use super::{
    CanonicalEntityId, ClaimId, Confidence, EntityGraphAttachmentId, EntityId, EntityResolutionId,
    EvidenceId, MemoryContractId, MemoryMemberId, MemoryScope, MemorySubgraphId,
    MemorySubgraphMemberId, SubgraphEdgeId, SummaryTraceId,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemorySubgraphStatus {
    Draft,
    Active,
    Archived,
}

impl Display for MemorySubgraphStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Draft => "draft",
            Self::Active => "active",
            Self::Archived => "archived",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemorySubgraphOwnerKind {
    User,
    Team,
    Service,
    Organization,
}

impl Display for MemorySubgraphOwnerKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::User => "user",
            Self::Team => "team",
            Self::Service => "service",
            Self::Organization => "organization",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemorySubgraphOwner {
    kind: MemorySubgraphOwnerKind,
    id: String,
}

impl MemorySubgraphOwner {
    pub fn new(
        kind: MemorySubgraphOwnerKind,
        id: impl Into<String>,
    ) -> Result<Self, MemoryGraphError> {
        Ok(Self {
            kind,
            id: non_empty(id.into(), "owner id")?,
        })
    }

    #[must_use]
    pub const fn kind(&self) -> MemorySubgraphOwnerKind {
        self.kind
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemorySubgraphPermissions {
    policy_json: String,
}

impl MemorySubgraphPermissions {
    pub fn new(policy_json: impl Into<String>) -> Result<Self, MemoryGraphError> {
        let policy_json = non_empty(policy_json.into(), "permissions")?;
        let trimmed = policy_json.trim();
        if !(trimmed.starts_with('{') && trimmed.ends_with('}')) {
            return Err(MemoryGraphError::InvalidPermissions);
        }
        Ok(Self { policy_json })
    }

    #[must_use]
    pub fn as_json(&self) -> &str {
        &self.policy_json
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemorySubgraph {
    id: MemorySubgraphId,
    scope: MemoryScope,
    parent_subgraph_id: Option<MemorySubgraphId>,
    name: String,
    description: Option<String>,
    owner: Option<MemorySubgraphOwner>,
    contract_id: Option<MemoryContractId>,
    summary_claim_id: Option<ClaimId>,
    permissions: Option<MemorySubgraphPermissions>,
    status: MemorySubgraphStatus,
}

impl MemorySubgraph {
    pub fn draft(
        id: MemorySubgraphId,
        scope: MemoryScope,
        parent_subgraph_id: Option<MemorySubgraphId>,
        name: impl Into<String>,
        description: Option<&str>,
    ) -> Result<Self, MemoryGraphError> {
        Ok(Self {
            id,
            scope,
            parent_subgraph_id,
            name: non_empty(name.into(), "subgraph name")?,
            description: description.map(str::to_string),
            owner: None,
            contract_id: None,
            summary_claim_id: None,
            permissions: None,
            status: MemorySubgraphStatus::Draft,
        })
    }

    pub fn active(
        draft: Self,
        activation: MemorySubgraphActivation,
    ) -> Result<Self, MemoryGraphError> {
        draft.activate(activation)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn from_record(
        id: MemorySubgraphId,
        scope: MemoryScope,
        parent_subgraph_id: Option<MemorySubgraphId>,
        name: impl Into<String>,
        description: Option<&str>,
        owner: Option<MemorySubgraphOwner>,
        contract_id: Option<MemoryContractId>,
        summary_claim_id: Option<ClaimId>,
        permissions: Option<MemorySubgraphPermissions>,
        status: MemorySubgraphStatus,
    ) -> Result<Self, MemoryGraphError> {
        if status == MemorySubgraphStatus::Active {
            if owner.is_none() {
                return Err(MemoryGraphError::MissingOwner);
            }
            if contract_id.is_none() {
                return Err(MemoryGraphError::MissingContract);
            }
            if summary_claim_id.is_none() {
                return Err(MemoryGraphError::MissingSummary);
            }
            if permissions.is_none() {
                return Err(MemoryGraphError::MissingPermissions);
            }
        }
        Ok(Self {
            id,
            scope,
            parent_subgraph_id,
            name: non_empty(name.into(), "subgraph name")?,
            description: description.map(str::to_string),
            owner,
            contract_id,
            summary_claim_id,
            permissions,
            status,
        })
    }

    pub fn activate(
        mut self,
        activation: MemorySubgraphActivation,
    ) -> Result<Self, MemoryGraphError> {
        let owner = activation.owner.ok_or(MemoryGraphError::MissingOwner)?;
        let contract_id = activation
            .contract_id
            .ok_or(MemoryGraphError::MissingContract)?;
        let permissions = activation
            .permissions
            .ok_or(MemoryGraphError::MissingPermissions)?;
        let summary_claim_id = activation
            .summary_claim_id
            .ok_or(MemoryGraphError::MissingSummary)?;
        if !activation.summary_traces.iter().any(|trace| {
            trace.subgraph_id() == &self.id && trace.summary_claim_id() == &summary_claim_id
        }) {
            return Err(MemoryGraphError::MissingSummaryTrace);
        }

        self.owner = Some(owner);
        self.contract_id = Some(contract_id);
        self.permissions = Some(permissions);
        self.summary_claim_id = Some(summary_claim_id);
        self.status = MemorySubgraphStatus::Active;
        Ok(self)
    }

    #[must_use]
    pub const fn id(&self) -> &MemorySubgraphId {
        &self.id
    }

    #[must_use]
    pub const fn scope(&self) -> &MemoryScope {
        &self.scope
    }

    #[must_use]
    pub const fn parent_subgraph_id(&self) -> Option<&MemorySubgraphId> {
        self.parent_subgraph_id.as_ref()
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn description(&self) -> Option<&str> {
        self.description.as_deref()
    }

    #[must_use]
    pub const fn owner(&self) -> Option<&MemorySubgraphOwner> {
        self.owner.as_ref()
    }

    #[must_use]
    pub const fn contract_id(&self) -> Option<&MemoryContractId> {
        self.contract_id.as_ref()
    }

    #[must_use]
    pub const fn summary_claim_id(&self) -> Option<&ClaimId> {
        self.summary_claim_id.as_ref()
    }

    #[must_use]
    pub const fn permissions(&self) -> Option<&MemorySubgraphPermissions> {
        self.permissions.as_ref()
    }

    #[must_use]
    pub const fn status(&self) -> MemorySubgraphStatus {
        self.status
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemorySubgraphActivation {
    owner: Option<MemorySubgraphOwner>,
    contract_id: Option<MemoryContractId>,
    permissions: Option<MemorySubgraphPermissions>,
    summary_claim_id: Option<ClaimId>,
    summary_traces: Vec<SummaryTrace>,
}

impl MemorySubgraphActivation {
    #[must_use]
    pub fn new(
        owner: Option<MemorySubgraphOwner>,
        contract_id: Option<MemoryContractId>,
        permissions: Option<MemorySubgraphPermissions>,
        summary_claim_id: Option<ClaimId>,
        summary_traces: Vec<SummaryTrace>,
    ) -> Self {
        Self {
            owner,
            contract_id,
            permissions,
            summary_claim_id,
            summary_traces,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemoryMemberKind {
    Source,
    Evidence,
    Entity,
    CanonicalEntity,
    Claim,
    Event,
    Relationship,
    Subgraph,
}

impl Display for MemoryMemberKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Source => "source",
            Self::Evidence => "evidence",
            Self::Entity => "entity",
            Self::CanonicalEntity => "canonical_entity",
            Self::Claim => "claim",
            Self::Event => "event",
            Self::Relationship => "relationship",
            Self::Subgraph => "subgraph",
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MemorySubgraphMemberRole {
    Member,
    Summary,
    InnerClaim,
    Evidence,
    CanonicalIdentity,
    ChildContext,
}

impl Display for MemorySubgraphMemberRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Member => "member",
            Self::Summary => "summary",
            Self::InnerClaim => "inner_claim",
            Self::Evidence => "evidence",
            Self::CanonicalIdentity => "canonical_identity",
            Self::ChildContext => "child_context",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MemorySubgraphMember {
    id: MemorySubgraphMemberId,
    scope: MemoryScope,
    subgraph_id: MemorySubgraphId,
    member_kind: MemoryMemberKind,
    member_id: MemoryMemberId,
    role: MemorySubgraphMemberRole,
}

impl MemorySubgraphMember {
    pub fn new(
        id: MemorySubgraphMemberId,
        scope: MemoryScope,
        subgraph_id: MemorySubgraphId,
        member_kind: MemoryMemberKind,
        member_id: MemoryMemberId,
        role: MemorySubgraphMemberRole,
    ) -> Result<Self, MemoryGraphError> {
        Ok(Self {
            id,
            scope,
            subgraph_id,
            member_kind,
            member_id,
            role,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &MemorySubgraphMemberId {
        &self.id
    }

    #[must_use]
    pub const fn scope(&self) -> &MemoryScope {
        &self.scope
    }

    #[must_use]
    pub const fn subgraph_id(&self) -> &MemorySubgraphId {
        &self.subgraph_id
    }

    #[must_use]
    pub const fn member_kind(&self) -> MemoryMemberKind {
        self.member_kind
    }

    #[must_use]
    pub const fn member_id(&self) -> &MemoryMemberId {
        &self.member_id
    }

    #[must_use]
    pub const fn role(&self) -> MemorySubgraphMemberRole {
        self.role
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CanonicalEntity {
    id: CanonicalEntityId,
    scope: MemoryScope,
    entity_type: String,
    display_name: String,
    aliases: Vec<String>,
}

impl CanonicalEntity {
    pub fn new(
        id: CanonicalEntityId,
        scope: MemoryScope,
        entity_type: impl Into<String>,
        display_name: impl Into<String>,
        aliases: Vec<String>,
    ) -> Result<Self, MemoryGraphError> {
        Ok(Self {
            id,
            scope,
            entity_type: non_empty(entity_type.into(), "canonical entity type")?,
            display_name: non_empty(display_name.into(), "canonical entity display name")?,
            aliases,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &CanonicalEntityId {
        &self.id
    }

    #[must_use]
    pub const fn scope(&self) -> &MemoryScope {
        &self.scope
    }

    #[must_use]
    pub fn entity_type(&self) -> &str {
        &self.entity_type
    }

    #[must_use]
    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    #[must_use]
    pub fn aliases(&self) -> &[String] {
        &self.aliases
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityResolutionStatus {
    Candidate,
    Confirmed,
    Rejected,
}

impl Display for EntityResolutionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Candidate => "candidate",
            Self::Confirmed => "confirmed",
            Self::Rejected => "rejected",
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct EntityResolution {
    id: EntityResolutionId,
    scope: MemoryScope,
    subgraph_id: MemorySubgraphId,
    entity_id: EntityId,
    canonical_entity_id: CanonicalEntityId,
    confidence: Confidence,
    status: EntityResolutionStatus,
    evidence_ids: Vec<EvidenceId>,
}

impl EntityResolution {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: EntityResolutionId,
        scope: MemoryScope,
        subgraph_id: MemorySubgraphId,
        entity_id: EntityId,
        canonical_entity_id: CanonicalEntityId,
        confidence: Confidence,
        status: EntityResolutionStatus,
        evidence_ids: Vec<EvidenceId>,
    ) -> Result<Self, MemoryGraphError> {
        if status == EntityResolutionStatus::Confirmed && evidence_ids.is_empty() {
            return Err(MemoryGraphError::MissingEvidence);
        }
        Ok(Self {
            id,
            scope,
            subgraph_id,
            entity_id,
            canonical_entity_id,
            confidence,
            status,
            evidence_ids,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &EntityResolutionId {
        &self.id
    }

    #[must_use]
    pub const fn scope(&self) -> &MemoryScope {
        &self.scope
    }

    #[must_use]
    pub const fn subgraph_id(&self) -> &MemorySubgraphId {
        &self.subgraph_id
    }

    #[must_use]
    pub const fn entity_id(&self) -> &EntityId {
        &self.entity_id
    }

    #[must_use]
    pub const fn canonical_entity_id(&self) -> &CanonicalEntityId {
        &self.canonical_entity_id
    }

    #[must_use]
    pub const fn confidence(&self) -> Confidence {
        self.confidence
    }

    #[must_use]
    pub const fn status(&self) -> EntityResolutionStatus {
        self.status
    }

    #[must_use]
    pub fn evidence_ids(&self) -> &[EvidenceId] {
        &self.evidence_ids
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubgraphEdge {
    id: SubgraphEdgeId,
    scope: MemoryScope,
    edge_type: String,
    from_subgraph_id: MemorySubgraphId,
    to_subgraph_id: MemorySubgraphId,
    from_member_kind: MemoryMemberKind,
    from_member_id: MemoryMemberId,
    to_member_kind: MemoryMemberKind,
    to_member_id: MemoryMemberId,
    claim_ids: Vec<ClaimId>,
    evidence_ids: Vec<EvidenceId>,
}

impl SubgraphEdge {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: SubgraphEdgeId,
        scope: MemoryScope,
        edge_type: impl Into<String>,
        from_subgraph_id: MemorySubgraphId,
        to_subgraph_id: MemorySubgraphId,
        from_member_kind: MemoryMemberKind,
        from_member_id: MemoryMemberId,
        to_member_kind: MemoryMemberKind,
        to_member_id: MemoryMemberId,
        claim_ids: Vec<ClaimId>,
        evidence_ids: Vec<EvidenceId>,
    ) -> Result<Self, MemoryGraphError> {
        if from_subgraph_id == to_subgraph_id {
            return Err(MemoryGraphError::SameSubgraphBoundary);
        }
        if claim_ids.is_empty() && evidence_ids.is_empty() {
            return Err(MemoryGraphError::MissingTraceSupport);
        }
        Ok(Self {
            id,
            scope,
            edge_type: non_empty(edge_type.into(), "subgraph edge type")?,
            from_subgraph_id,
            to_subgraph_id,
            from_member_kind,
            from_member_id,
            to_member_kind,
            to_member_id,
            claim_ids,
            evidence_ids,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &SubgraphEdgeId {
        &self.id
    }

    #[must_use]
    pub const fn scope(&self) -> &MemoryScope {
        &self.scope
    }

    #[must_use]
    pub fn edge_type(&self) -> &str {
        &self.edge_type
    }

    #[must_use]
    pub const fn from_subgraph_id(&self) -> &MemorySubgraphId {
        &self.from_subgraph_id
    }

    #[must_use]
    pub const fn to_subgraph_id(&self) -> &MemorySubgraphId {
        &self.to_subgraph_id
    }

    #[must_use]
    pub const fn from_member_kind(&self) -> MemoryMemberKind {
        self.from_member_kind
    }

    #[must_use]
    pub const fn from_member_id(&self) -> &MemoryMemberId {
        &self.from_member_id
    }

    #[must_use]
    pub const fn to_member_kind(&self) -> MemoryMemberKind {
        self.to_member_kind
    }

    #[must_use]
    pub const fn to_member_id(&self) -> &MemoryMemberId {
        &self.to_member_id
    }

    #[must_use]
    pub fn claim_ids(&self) -> &[ClaimId] {
        &self.claim_ids
    }

    #[must_use]
    pub fn evidence_ids(&self) -> &[EvidenceId] {
        &self.evidence_ids
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SummaryTrace {
    id: SummaryTraceId,
    scope: MemoryScope,
    subgraph_id: MemorySubgraphId,
    summary_claim_id: ClaimId,
    inner_claim_ids: Vec<ClaimId>,
    evidence_ids: Vec<EvidenceId>,
}

impl SummaryTrace {
    pub fn new(
        id: SummaryTraceId,
        scope: MemoryScope,
        subgraph_id: MemorySubgraphId,
        summary_claim_id: ClaimId,
        inner_claim_ids: Vec<ClaimId>,
        evidence_ids: Vec<EvidenceId>,
    ) -> Result<Self, MemoryGraphError> {
        if inner_claim_ids.is_empty() && evidence_ids.is_empty() {
            return Err(MemoryGraphError::MissingTraceSupport);
        }
        Ok(Self {
            id,
            scope,
            subgraph_id,
            summary_claim_id,
            inner_claim_ids,
            evidence_ids,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &SummaryTraceId {
        &self.id
    }

    #[must_use]
    pub const fn scope(&self) -> &MemoryScope {
        &self.scope
    }

    #[must_use]
    pub const fn subgraph_id(&self) -> &MemorySubgraphId {
        &self.subgraph_id
    }

    #[must_use]
    pub const fn summary_claim_id(&self) -> &ClaimId {
        &self.summary_claim_id
    }

    #[must_use]
    pub fn inner_claim_ids(&self) -> &[ClaimId] {
        &self.inner_claim_ids
    }

    #[must_use]
    pub fn evidence_ids(&self) -> &[EvidenceId] {
        &self.evidence_ids
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntityGraphAttachmentType {
    Primary,
    Supporting,
    Historical,
}

impl Display for EntityGraphAttachmentType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Primary => "primary",
            Self::Supporting => "supporting",
            Self::Historical => "historical",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntityGraphAttachment {
    id: EntityGraphAttachmentId,
    scope: MemoryScope,
    canonical_entity_id: CanonicalEntityId,
    subgraph_id: MemorySubgraphId,
    attachment_type: EntityGraphAttachmentType,
}

impl EntityGraphAttachment {
    pub fn new(
        id: EntityGraphAttachmentId,
        scope: MemoryScope,
        canonical_entity_id: CanonicalEntityId,
        subgraph_id: MemorySubgraphId,
        attachment_type: EntityGraphAttachmentType,
    ) -> Result<Self, MemoryGraphError> {
        Ok(Self {
            id,
            scope,
            canonical_entity_id,
            subgraph_id,
            attachment_type,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &EntityGraphAttachmentId {
        &self.id
    }

    #[must_use]
    pub const fn scope(&self) -> &MemoryScope {
        &self.scope
    }

    #[must_use]
    pub const fn canonical_entity_id(&self) -> &CanonicalEntityId {
        &self.canonical_entity_id
    }

    #[must_use]
    pub const fn subgraph_id(&self) -> &MemorySubgraphId {
        &self.subgraph_id
    }

    #[must_use]
    pub const fn attachment_type(&self) -> EntityGraphAttachmentType {
        self.attachment_type
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum MemoryGraphError {
    #[error("{field} cannot be empty")]
    EmptyField { field: &'static str },
    #[error("memory subgraph activation requires an owner")]
    MissingOwner,
    #[error("memory subgraph activation requires a contract")]
    MissingContract,
    #[error("memory subgraph activation requires permissions")]
    MissingPermissions,
    #[error("memory subgraph activation requires a summary")]
    MissingSummary,
    #[error("memory subgraph activation requires a summary trace")]
    MissingSummaryTrace,
    #[error("summary traces and boundary edges require supporting claims or evidence")]
    MissingTraceSupport,
    #[error("confirmed entity resolution requires evidence")]
    MissingEvidence,
    #[error("subgraph edge endpoints must cross subgraph boundaries")]
    SameSubgraphBoundary,
    #[error("permissions must be a JSON object")]
    InvalidPermissions,
}

fn non_empty(value: String, field: &'static str) -> Result<String, MemoryGraphError> {
    if value.trim().is_empty() {
        return Err(MemoryGraphError::EmptyField { field });
    }
    Ok(value)
}
