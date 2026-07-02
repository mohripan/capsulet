pub mod runtime;

mod auth;

mod automations;
mod error;
mod graphs;
mod http;
mod memory;
mod models;
mod state;
mod store;
mod webhooks;

pub use auth::{AuthConfig, Principal, Role};
pub use http::router;
pub use models::{
    CreateAutomationRequest, CreateAutomationTriggerRequest, CreateJobDefinitionRequest,
    CreateRunRequest, CreateTriggerPluginRequest, CreateWorkflowDependencyRequest,
    CreateWorkflowRequest, CreateWorkflowStepRequest,
};
pub use state::{AdmissionConfig, AppState};
pub use store::ApiStore;
pub use webhooks::WebhookSecrets;

#[cfg(test)]
mod tests;
