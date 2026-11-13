//! HTTP transport boundary for the API crate.
//!
//! [`router`] builds the server from the endpoint contract. The modules below
//! hold the handlers it routes to, by area, and what they share: who is calling
//! and in which project (`identity`), admission control, and small helpers.

mod activity;
mod admission;
mod artifacts;
mod common;
mod health;
mod identity;
mod jobs;
mod projects;
mod router;
mod workflows;

pub(crate) use common::{generated_id, json_from_string, valid_json_object_string};
pub(crate) use identity::{
    ProjectContext, project_context, require_project_role, require_resource_project,
};
pub use router::router;
