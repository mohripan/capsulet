//! Entities, claims, events and the relationships between them.

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use capsulet_core::{
    Claim, ClaimId, ClaimStatus, Confidence, Entity, EntityId, Event, EventId, EvidenceId,
    Relationship, RelationshipId,
};
use capsulet_storage::ObjectStore;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{memory_validation, parse_authority, require_scope, scope, write_context};
use crate::{
    auth::Principal,
    error::ApiError,
    http::{generated_id, project_context},
    state::AppState,
    store::ApiStore,
};

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = CreateMemoryEntityRequest)]
pub(crate) struct CreateEntityRequest {
    id: Option<String>,
    entity_type: String,
    name: String,
    #[serde(default)]
    aliases: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = CreateMemoryClaimRequest)]
pub(crate) struct CreateClaimRequest {
    id: Option<String>,
    subject_id: String,
    predicate: String,
    object: String,
    evidence_ids: Vec<String>,
    confidence: f64,
    authority: String,
    status: Option<String>,
    observed_at: String,
    valid_from: Option<String>,
    valid_until: Option<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = CreateMemoryEventRequest)]
pub(crate) struct CreateEventRequest {
    id: Option<String>,
    event_type: String,
    occurred_at: String,
    #[serde(default)]
    entity_ids: Vec<String>,
    evidence_ids: Vec<String>,
}

#[derive(Debug, Deserialize, ToSchema)]
#[schema(as = CreateMemoryRelationshipRequest)]
pub(crate) struct CreateRelationshipRequest {
    id: Option<String>,
    relationship_type: String,
    from_entity_id: String,
    to_entity_id: String,
    evidence_ids: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = MemoryEntityResponse)]
pub(crate) struct EntityResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    entity_type: String,
    name: String,
    aliases: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = MemoryClaimResponse)]
pub(crate) struct ClaimResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    subject_id: String,
    predicate: String,
    object: String,
    evidence_ids: Vec<String>,
    confidence: f64,
    authority: String,
    status: String,
    observed_at: String,
    valid_from: Option<String>,
    valid_until: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = MemoryEventResponse)]
pub(crate) struct EventResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    event_type: String,
    occurred_at: String,
    entity_ids: Vec<String>,
    evidence_ids: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = MemoryRelationshipResponse)]
pub(crate) struct RelationshipResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    relationship_type: String,
    from_entity_id: String,
    to_entity_id: String,
    evidence_ids: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = ListMemoryEntitiesResponse)]
pub(crate) struct ListEntitiesResponse {
    entities: Vec<EntityResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = ListMemoryClaimsResponse)]
pub(crate) struct ListClaimsResponse {
    claims: Vec<ClaimResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = ListMemoryEventsResponse)]
pub(crate) struct ListEventsResponse {
    events: Vec<EventResponse>,
}

#[derive(Debug, Serialize, ToSchema)]
#[schema(as = ListMemoryRelationshipsResponse)]
pub(crate) struct ListRelationshipsResponse {
    relationships: Vec<RelationshipResponse>,
}

pub(crate) async fn create_entity<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateEntityRequest>,
) -> Result<(StatusCode, Json<EntityResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let entity = Entity::new(
        EntityId::new(request.id.unwrap_or_else(|| generated_id("entity")))
            .map_err(ApiError::validation)?,
        scope(&context)?,
        request.entity_type,
        request.name,
        request.aliases,
    )
    .map_err(memory_validation)?;
    if let Some(existing) = state
        .store
        .find_memory_entity(entity.id())
        .await
        .map_err(ApiError::store)?
    {
        require_scope(existing.scope(), &context, entity.id().as_str())?;
    }
    state
        .store
        .upsert_memory_entity(&entity)
        .await
        .map_err(ApiError::store)?;
    Ok((StatusCode::CREATED, Json(EntityResponse::from(&entity))))
}

pub(crate) async fn list_entities<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListEntitiesResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let entities = state
        .store
        .list_memory_entities(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListEntitiesResponse {
        entities: entities.iter().map(EntityResponse::from).collect(),
    }))
}

pub(crate) async fn get_entity<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<EntityResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = EntityId::new(id).map_err(ApiError::validation)?;
    let Some(entity) = state
        .store
        .find_memory_entity_in_project(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
    Ok(Json(EntityResponse::from(&entity)))
}

pub(crate) async fn create_claim<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateClaimRequest>,
) -> Result<(StatusCode, Json<ClaimResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let claim = Claim::new(
        ClaimId::new(request.id.unwrap_or_else(|| generated_id("claim")))
            .map_err(ApiError::validation)?,
        scope(&context)?,
        EntityId::new(request.subject_id).map_err(ApiError::validation)?,
        request.predicate,
        request.object,
        request
            .evidence_ids
            .into_iter()
            .map(EvidenceId::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ApiError::validation)?,
        Confidence::new(request.confidence).map_err(memory_validation)?,
        parse_authority(&request.authority)?,
        request.observed_at,
        request.valid_from.as_deref(),
        request.valid_until.as_deref(),
    )
    .map_err(memory_validation)?
    .with_status(match request.status {
        Some(status) => parse_claim_status(&status)?,
        None => ClaimStatus::Candidate,
    });
    if let Some(existing) = state
        .store
        .find_memory_claim(claim.id())
        .await
        .map_err(ApiError::store)?
    {
        require_scope(existing.scope(), &context, claim.id().as_str())?;
    }
    state
        .store
        .upsert_memory_claim(&claim)
        .await
        .map_err(ApiError::store)?;
    Ok((StatusCode::CREATED, Json(ClaimResponse::from(&claim))))
}

pub(crate) async fn list_claims<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListClaimsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let claims = state
        .store
        .list_memory_claims(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListClaimsResponse {
        claims: claims.iter().map(ClaimResponse::from).collect(),
    }))
}

pub(crate) async fn get_claim<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<ClaimResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = ClaimId::new(id).map_err(ApiError::validation)?;
    let Some(claim) = state
        .store
        .find_memory_claim_in_project(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
    Ok(Json(ClaimResponse::from(&claim)))
}

pub(crate) async fn create_event<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateEventRequest>,
) -> Result<(StatusCode, Json<EventResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let event = Event::new(
        EventId::new(request.id.unwrap_or_else(|| generated_id("event")))
            .map_err(ApiError::validation)?,
        scope(&context)?,
        request.event_type,
        request.occurred_at,
        request
            .entity_ids
            .into_iter()
            .map(EntityId::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ApiError::validation)?,
        request
            .evidence_ids
            .into_iter()
            .map(EvidenceId::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ApiError::validation)?,
    )
    .map_err(memory_validation)?;
    if let Some(existing) = state
        .store
        .find_memory_event(event.id())
        .await
        .map_err(ApiError::store)?
    {
        require_scope(existing.scope(), &context, event.id().as_str())?;
    }
    state
        .store
        .upsert_memory_event(&event)
        .await
        .map_err(ApiError::store)?;
    Ok((StatusCode::CREATED, Json(EventResponse::from(&event))))
}

pub(crate) async fn list_events<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListEventsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let events = state
        .store
        .list_memory_events(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListEventsResponse {
        events: events.iter().map(EventResponse::from).collect(),
    }))
}

pub(crate) async fn get_event<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<EventResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = EventId::new(id).map_err(ApiError::validation)?;
    let Some(event) = state
        .store
        .find_memory_event_in_project(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
    Ok(Json(EventResponse::from(&event)))
}

pub(crate) async fn create_relationship<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateRelationshipRequest>,
) -> Result<(StatusCode, Json<RelationshipResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let relationship = Relationship::new(
        RelationshipId::new(request.id.unwrap_or_else(|| generated_id("relationship")))
            .map_err(ApiError::validation)?,
        scope(&context)?,
        request.relationship_type,
        EntityId::new(request.from_entity_id).map_err(ApiError::validation)?,
        EntityId::new(request.to_entity_id).map_err(ApiError::validation)?,
        request
            .evidence_ids
            .into_iter()
            .map(EvidenceId::new)
            .collect::<Result<Vec<_>, _>>()
            .map_err(ApiError::validation)?,
    )
    .map_err(memory_validation)?;
    if let Some(existing) = state
        .store
        .find_memory_relationship(relationship.id())
        .await
        .map_err(ApiError::store)?
    {
        require_scope(existing.scope(), &context, relationship.id().as_str())?;
    }
    state
        .store
        .upsert_memory_relationship(&relationship)
        .await
        .map_err(ApiError::store)?;
    Ok((
        StatusCode::CREATED,
        Json(RelationshipResponse::from(&relationship)),
    ))
}

pub(crate) async fn list_relationships<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListRelationshipsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let relationships = state
        .store
        .list_memory_relationships(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListRelationshipsResponse {
        relationships: relationships
            .iter()
            .map(RelationshipResponse::from)
            .collect(),
    }))
}

pub(crate) async fn get_relationship<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<RelationshipResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = RelationshipId::new(id).map_err(ApiError::validation)?;
    let Some(relationship) = state
        .store
        .find_memory_relationship_in_project(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
    Ok(Json(RelationshipResponse::from(&relationship)))
}

pub(super) fn parse_claim_status(value: &str) -> Result<ClaimStatus, ApiError> {
    match value {
        "candidate" => Ok(ClaimStatus::Candidate),
        "active" => Ok(ClaimStatus::Active),
        "rejected" => Ok(ClaimStatus::Rejected),
        "superseded" => Ok(ClaimStatus::Superseded),
        "contradicted" => Ok(ClaimStatus::Contradicted),
        "expired" => Ok(ClaimStatus::Expired),
        value => Err(ApiError::Validation(format!(
            "unknown claim status {value}"
        ))),
    }
}

impl From<&Entity> for EntityResponse {
    fn from(entity: &Entity) -> Self {
        Self {
            id: entity.id().as_str().to_string(),
            tenant_id: entity.scope().tenant_id().to_string(),
            project_id: entity.scope().project_id().to_string(),
            entity_type: entity.entity_type().to_string(),
            name: entity.name().to_string(),
            aliases: entity.aliases().to_vec(),
        }
    }
}

impl From<&Claim> for ClaimResponse {
    fn from(claim: &Claim) -> Self {
        Self {
            id: claim.id().as_str().to_string(),
            tenant_id: claim.scope().tenant_id().to_string(),
            project_id: claim.scope().project_id().to_string(),
            subject_id: claim.subject_id().as_str().to_string(),
            predicate: claim.predicate().to_string(),
            object: claim.object().to_string(),
            evidence_ids: claim
                .evidence_ids()
                .iter()
                .map(|id| id.as_str().to_string())
                .collect(),
            confidence: claim.confidence().value(),
            authority: claim.authority().to_string(),
            status: claim.status().to_string(),
            observed_at: claim.observed_at().to_string(),
            valid_from: claim.valid_from().map(str::to_string),
            valid_until: claim.valid_until().map(str::to_string),
        }
    }
}

impl From<&Event> for EventResponse {
    fn from(event: &Event) -> Self {
        Self {
            id: event.id().as_str().to_string(),
            tenant_id: event.scope().tenant_id().to_string(),
            project_id: event.scope().project_id().to_string(),
            event_type: event.event_type().to_string(),
            occurred_at: event.occurred_at().to_string(),
            entity_ids: event
                .entity_ids()
                .iter()
                .map(|id| id.as_str().to_string())
                .collect(),
            evidence_ids: event
                .evidence_ids()
                .iter()
                .map(|id| id.as_str().to_string())
                .collect(),
        }
    }
}

impl From<&Relationship> for RelationshipResponse {
    fn from(relationship: &Relationship) -> Self {
        Self {
            id: relationship.id().as_str().to_string(),
            tenant_id: relationship.scope().tenant_id().to_string(),
            project_id: relationship.scope().project_id().to_string(),
            relationship_type: relationship.relationship_type().to_string(),
            from_entity_id: relationship.from_entity_id().as_str().to_string(),
            to_entity_id: relationship.to_entity_id().as_str().to_string(),
            evidence_ids: relationship
                .evidence_ids()
                .iter()
                .map(|id| id.as_str().to_string())
                .collect(),
        }
    }
}
