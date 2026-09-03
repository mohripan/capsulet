pub mod runtime;

mod assurance;
mod auth;

mod automations;
mod endpoint_contract;
mod error;
mod graphs;
mod http;
mod ingestion;
mod memory;
mod models;
mod openapi;
mod reasoning;
mod state;
mod store;
mod webhooks;
mod wire_schemas;

pub use auth::{AuthConfig, Principal, ProjectRole, Role};
pub use endpoint_contract::{
    AuthenticationMode, EndpointContract, EndpointPolicy, ErrorContract, HttpMethod, MediaType,
    OwnershipRule, Permission, ProjectContextRule, RequiredPermission, ResourceKind,
    ResponseStatus, SchemaId, Stability, distinct_paths, endpoint_contracts, find_endpoint,
    find_operation,
};
pub use http::router;
pub use models::{
    CreateAutomationRequest, CreateAutomationTriggerRequest, CreateJobDefinitionRequest,
    CreateRunRequest, CreateServiceAccountResponse, CreateTriggerPluginRequest,
    CreateWorkflowDependencyRequest, CreateWorkflowRequest, CreateWorkflowStepRequest,
    ServiceAccountResponse,
};
pub use openapi::{canonical_openapi_json, generated_openapi, validated_openapi};
pub use state::{AdmissionConfig, AppState};
pub use store::ApiStore;
pub use webhooks::WebhookSecrets;

#[cfg(test)]
mod tests;
