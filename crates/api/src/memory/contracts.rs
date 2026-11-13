//! Memory contracts, and the policy each compiles to.

use axum::{
    Extension, Json,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use capsulet_core::{
    CompiledMemoryPolicy, MemoryContract, MemoryContractId, RelationTypeSpec, RetrievalPolicySpec,
};
use capsulet_storage::ObjectStore;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;

use super::{require_scope, scope, write_context};
use crate::{
    auth::Principal,
    error::ApiError,
    http::{generated_id, project_context},
    state::AppState,
    store::ApiStore,
};

#[derive(Debug, Deserialize, ToSchema)]
pub(crate) struct CreateMemoryContractRequest {
    id: Option<String>,
    name: String,
    source: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct MemoryContractResponse {
    id: String,
    tenant_id: String,
    project_id: String,
    name: String,
    source: String,
    compiled: CompiledMemoryPolicyResponse,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct CompiledMemoryPolicyResponse {
    entity_types: Vec<String>,
    relations: Vec<RelationPolicyResponse>,
    claim_policy: ClaimPolicyResponse,
    source_priority: Vec<String>,
    retrieval_policies: Vec<RetrievalPolicyResponse>,
    contradiction_rules: Vec<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct RelationPolicyResponse {
    name: String,
    from: String,
    to: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ClaimPolicyResponse {
    require_source: bool,
    store_confidence: bool,
    allow_contradictions: bool,
    min_confidence: f64,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct RetrievalPolicyResponse {
    name: String,
    seed_from: Vec<String>,
    max_hops: Option<u32>,
}

#[derive(Debug, Serialize, ToSchema)]
pub(crate) struct ListMemoryContractsResponse {
    contracts: Vec<MemoryContractResponse>,
}

pub(crate) async fn create_contract<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Json(request): Json<CreateMemoryContractRequest>,
) -> Result<(StatusCode, Json<MemoryContractResponse>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = write_context(&headers, &principal)?;
    let contract = MemoryContract::parse_scoped(
        MemoryContractId::new(request.id.unwrap_or_else(|| generated_id("contract")))
            .map_err(ApiError::validation)?,
        scope(&context)?,
        request.name,
        request.source,
    )
    .map_err(contract_validation)?;
    contract.compile().map_err(contract_validation)?;
    if let Some(existing) = state
        .store
        .find_memory_contract(contract.id())
        .await
        .map_err(ApiError::store)?
    {
        require_scope(existing.scope(), &context, contract.id().as_str())?;
    }
    state
        .store
        .upsert_memory_contract(&contract)
        .await
        .map_err(ApiError::store)?;
    Ok((
        StatusCode::CREATED,
        Json(MemoryContractResponse::new(&contract)?),
    ))
}

pub(crate) async fn list_contracts<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
) -> Result<Json<ListMemoryContractsResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let contracts = state
        .store
        .list_memory_contracts(&context.tenant_id, &context.project_id, 100)
        .await
        .map_err(ApiError::store)?;
    Ok(Json(ListMemoryContractsResponse {
        contracts: contracts
            .iter()
            .map(MemoryContractResponse::new)
            .collect::<Result<Vec<_>, _>>()?,
    }))
}

pub(crate) async fn get_contract<S, O>(
    State(state): State<AppState<S, O>>,
    headers: HeaderMap,
    Extension(principal): Extension<Principal>,
    Path(id): Path<String>,
) -> Result<Json<MemoryContractResponse>, ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let context = project_context(&headers, &principal)?;
    let id = MemoryContractId::new(id).map_err(ApiError::validation)?;
    let Some(contract) = state
        .store
        .find_memory_contract_in_project(&id, &context.tenant_id, &context.project_id)
        .await
        .map_err(ApiError::store)?
    else {
        return Err(ApiError::MemoryNotFound(id.as_str().to_string()));
    };
    Ok(Json(MemoryContractResponse::new(&contract)?))
}

#[expect(
    clippy::needless_pass_by_value,
    reason = "map_err supplies the owned MemoryContractError and this helper keeps conversions concise"
)]
pub(super) fn contract_validation(error: capsulet_core::MemoryContractError) -> ApiError {
    ApiError::Validation(error.to_string())
}

impl MemoryContractResponse {
    fn new(contract: &MemoryContract) -> Result<Self, ApiError> {
        Ok(Self {
            id: contract.id().as_str().to_string(),
            tenant_id: contract.scope().tenant_id().to_string(),
            project_id: contract.scope().project_id().to_string(),
            name: contract.name().to_string(),
            source: contract.source().to_string(),
            compiled: CompiledMemoryPolicyResponse::from(
                &contract.compile().map_err(contract_validation)?,
            ),
        })
    }
}

impl From<&CompiledMemoryPolicy> for CompiledMemoryPolicyResponse {
    fn from(policy: &CompiledMemoryPolicy) -> Self {
        Self {
            entity_types: policy
                .entity_types()
                .iter()
                .map(|entity| entity.name().to_string())
                .collect(),
            relations: policy
                .relation_types()
                .iter()
                .map(RelationPolicyResponse::from)
                .collect(),
            claim_policy: ClaimPolicyResponse {
                require_source: policy.claim_policy().require_source(),
                store_confidence: policy.claim_policy().store_confidence(),
                allow_contradictions: policy.claim_policy().allow_contradictions(),
                min_confidence: policy.claim_policy().min_confidence().value(),
            },
            source_priority: policy.trust_policy().source_priority().to_vec(),
            retrieval_policies: policy
                .retrieval_policies()
                .iter()
                .map(RetrievalPolicyResponse::from)
                .collect(),
            contradiction_rules: policy
                .contradiction_rules()
                .iter()
                .map(|rule| rule.name().to_string())
                .collect(),
        }
    }
}

impl From<&RelationTypeSpec> for RelationPolicyResponse {
    fn from(relation: &RelationTypeSpec) -> Self {
        Self {
            name: relation.name().to_string(),
            from: relation.from_entity().to_string(),
            to: relation.to_entity().to_string(),
        }
    }
}

impl From<&RetrievalPolicySpec> for RetrievalPolicyResponse {
    fn from(policy: &RetrievalPolicySpec) -> Self {
        Self {
            name: policy.name().to_string(),
            seed_from: policy.seed_from().to_vec(),
            max_hops: policy.max_hops(),
        }
    }
}
