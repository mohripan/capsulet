pub mod runtime;

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

pub use auth::{AuthConfig, Principal, Role};
pub use endpoint_contract::{EndpointContract, distinct_paths, endpoint_contracts, find_endpoint};
pub use http::router;
pub use models::{
    CreateAutomationRequest, CreateAutomationTriggerRequest, CreateJobDefinitionRequest,
    CreateRunRequest, CreateTriggerPluginRequest, CreateWorkflowDependencyRequest,
    CreateWorkflowRequest, CreateWorkflowStepRequest,
};
pub use openapi::{canonical_openapi_json, generated_openapi, validated_openapi};
pub use state::{AdmissionConfig, AppState};
pub use store::ApiStore;
pub use webhooks::WebhookSecrets;

#[cfg(test)]
mod tests;
