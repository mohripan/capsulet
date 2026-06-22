pub mod runtime;

mod auth;

mod automations;
mod error;
mod http;
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
pub use state::AppState;
pub use store::ApiStore;
pub use webhooks::WebhookSecrets;

#[cfg(test)]
mod tests;
