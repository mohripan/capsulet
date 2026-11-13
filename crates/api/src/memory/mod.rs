//! Claim memory over HTTP: what the memory routes take, what they answer, and the
//! handlers between.
//!
//! One module per kind of record. What several share, the project scope a request
//! writes in and the validation errors it is refused with, is here.

mod contracts;
mod graph;
mod records;
mod review;
mod sources;

use axum::http::HeaderMap;
use capsulet_core::{Authority, MemoryScope};

use crate::{
    auth::Principal,
    error::ApiError,
    http::{ProjectContext, project_context, require_project_role},
};

pub(crate) use self::contracts::{
    CreateMemoryContractRequest, ListMemoryContractsResponse, MemoryContractResponse,
    create_contract, get_contract, list_contracts,
};
pub(crate) use self::graph::{
    ActivateMemorySubgraphRequest, CanonicalEntityResponse, CreateCanonicalEntityRequest,
    CreateEntityGraphAttachmentRequest, CreateMemorySubgraphMemberRequest,
    CreateMemorySubgraphRequest, CreateSubgraphEdgeRequest, CreateSummaryTraceRequest,
    EntityGraphAttachmentResponse, ListCanonicalEntitiesResponse, ListMemorySubgraphsResponse,
    MemorySubgraphMemberResponse, MemorySubgraphResponse, SubgraphEdgeResponse,
    SummaryTraceResponse, activate_subgraph, create_canonical_entity,
    create_entity_graph_attachment, create_subgraph, create_subgraph_edge, create_subgraph_member,
    create_summary_trace, get_subgraph, list_canonical_entities, list_subgraphs,
};
pub(crate) use self::records::{
    ClaimResponse, CreateClaimRequest, CreateEntityRequest, CreateEventRequest,
    CreateRelationshipRequest, EntityResponse, EventResponse, ListClaimsResponse,
    ListEntitiesResponse, ListEventsResponse, ListRelationshipsResponse, RelationshipResponse,
    create_claim, create_entity, create_event, create_relationship, get_claim, get_entity,
    get_event, get_relationship, list_claims, list_entities, list_events, list_relationships,
};
pub(crate) use self::review::{
    ClaimConflictResponse, CreateEntityResolutionRequest, EntityResolutionResponse,
    ListClaimConflictsQuery, ListClaimConflictsResponse, ListEntityResolutionsQuery,
    ListEntityResolutionsResponse, ResolveClaimConflictRequest, confirm_entity_resolution,
    create_entity_resolution, dismiss_claim_conflict, list_claim_conflicts,
    list_entity_resolutions, reject_entity_resolution, resolve_claim_conflict,
};
pub(crate) use self::sources::{
    CreateEvidenceRequest, CreateSourceRequest, EvidenceResponse, ListEvidenceResponse,
    ListSourcesResponse, SourceResponse, create_evidence, create_source, get_evidence, get_source,
    list_evidence, list_sources,
};

fn write_context(headers: &HeaderMap, principal: &Principal) -> Result<ProjectContext, ApiError> {
    let context = project_context(headers, principal)?;
    require_project_role(&context, crate::ProjectRole::Operator)?;
    Ok(context)
}

fn scope(context: &ProjectContext) -> Result<MemoryScope, ApiError> {
    MemoryScope::new(context.tenant_id.clone(), context.project_id.clone())
        .map_err(memory_validation)
}

fn in_scope(scope: &MemoryScope, context: &ProjectContext) -> bool {
    scope.tenant_id() == context.tenant_id && scope.project_id() == context.project_id
}

/// Memory ids share one global namespace, so a record loaded by id may belong to any tenant.
/// Out-of-scope records report as missing rather than forbidden so the id space stays opaque.
pub(crate) fn require_scope(
    scope: &MemoryScope,
    context: &ProjectContext,
    id: &str,
) -> Result<(), ApiError> {
    if in_scope(scope, context) {
        Ok(())
    } else {
        Err(ApiError::MemoryNotFound(id.to_string()))
    }
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "map_err supplies the owned MemoryError and this helper keeps conversions concise"
)]
fn memory_validation(error: capsulet_core::MemoryError) -> ApiError {
    ApiError::Validation(error.to_string())
}

fn parse_authority(value: &str) -> Result<Authority, ApiError> {
    match value {
        "low" => Ok(Authority::Low),
        "medium" => Ok(Authority::Medium),
        "high" => Ok(Authority::High),
        value => Err(ApiError::Validation(format!("unknown authority {value}"))),
    }
}
