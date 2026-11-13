//! The in-memory store the API's tests run against, one storage domain per module.
//!
//! `FakeStore` itself and its helpers are in the parent module, beside the tests
//! that reach into its state.

mod agents;
mod automations;
mod base;
mod iam;
mod ingestion;
mod ir;
mod jobs;
mod memory;
mod memory_graph;
mod packs;
mod reasoning;
mod workflows;
