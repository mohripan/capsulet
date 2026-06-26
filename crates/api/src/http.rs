//! HTTP transport boundary for the API crate.

mod internal;

pub(crate) mod activity {}
pub(crate) mod artifacts {}
pub(crate) mod health {}
pub(crate) mod identity {}
pub(crate) mod jobs {}
pub(crate) mod workflows {}

pub use internal::router;
pub(crate) use internal::{
    ProjectContext, assign_resource_project, generated_id, json_from_string, project_context,
    require_project_role, require_resource_project, valid_json_object_string,
};
