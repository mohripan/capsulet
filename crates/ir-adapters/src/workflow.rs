//! Job DAG workflows into the IR.
//!
//! A workflow step runs a job in a container. From the IR's point of view that
//! is an external side effect with unmodelled inputs and outputs, so a step
//! becomes an effect node with opaque ports and a declared capability. That is
//! not a limitation of the translation — it is an accurate statement of what
//! the current model knows about a job, and writing it down is how the gap
//! becomes visible instead of assumed away.
//!
//! Everything lands in `Observe` mode. Nothing about a job DAG declares
//! obligations, so claiming any verdict but `unverified` would be inventing
//! assurance the workflow never asked for.

use capsulet_core::{WorkflowDefinition, WorkflowDependencyPolicy};
use capsulet_ir::capability::{Capability, CapabilitySet, Grant};
use capsulet_ir::definition::{AssuranceMode, Definition};
use capsulet_ir::effect::{Effect, EffectKind, Idempotency, Reversibility};
use capsulet_ir::graph::{ControlEdge, Graph, GraphBuilder};
use capsulet_ir::id::Identifier;
use capsulet_ir::node::{Node, NodeKind, ResourceBudget};
use capsulet_ir::port::{InputPort, OutputPort};
use capsulet_ir::value::{ValueSchema, aliases};

use crate::{AdaptationNote, Adapted, AdapterError};

/// The capability every job step spends: the right to run a job.
const JOB_EXECUTION: &str = "job-execution";

/// Milliseconds a step is allowed when it declares no timeout of its own.
const DEFAULT_STEP_MS: u64 = 3_600_000;

/// Translates a workflow definition.
///
/// # Errors
///
/// Returns [`AdapterError`] when a workflow, step, or dependency name is not a
/// legal IR identifier, or when the translation is not admissible.
pub fn from_workflow(workflow: &WorkflowDefinition) -> Result<Adapted, AdapterError> {
    let mut notes = Vec::new();
    let mut parts = GraphBuilder::default();

    for step in workflow.steps() {
        let id = identifier(step.id().as_str(), "workflow step id")?;
        parts.nodes.push(Node {
            id: id.clone(),
            name: step.name().to_string(),
            kind: NodeKind::Effect,
            inputs: vec![InputPort::new(
                identifier("upstream", "port")?,
                opaque_step_value(),
            )],
            outputs: vec![OutputPort::new(
                identifier("result", "port")?,
                opaque_step_value(),
            )],
            capabilities: vec![identifier(JOB_EXECUTION, "capability")?],
            effects: vec![Effect {
                id: identifier("run-job", "effect")?,
                kind: EffectKind::ExternalSideEffect,
                target: step.job_definition_id().as_str().to_string(),
                capability: identifier(JOB_EXECUTION, "capability")?,
                // The current runtime leases a step and fences stale workers,
                // but nothing in the model says the job itself is safe to run
                // twice, so the honest declaration is that it is not.
                idempotency: Idempotency::NonIdempotent,
                reversibility: Reversibility::Irreversible,
            }],
            budget: ResourceBudget {
                wall_ms: step
                    .timeout_seconds()
                    .map_or(DEFAULT_STEP_MS, |seconds| seconds.saturating_mul(1_000)),
                tokens: 0,
                cost_micro_units: 0,
                effect_count: 1,
            },
            provider: None,
            sub_workflow: None,
        });
    }

    notes.push(AdaptationNote::new(
        "workflow step",
        "a job's inputs and outputs are not modelled, so its ports are opaque",
    ));

    let mut soft_dependencies = 0_usize;
    for dependency in workflow.dependencies() {
        parts.control.push(ControlEdge {
            from: identifier(dependency.from_step_id().as_str(), "workflow step id")?,
            to: identifier(dependency.to_step_id().as_str(), "workflow step id")?,
        });
        if dependency.policy() != WorkflowDependencyPolicy::Hard {
            soft_dependencies += 1;
        }
    }

    if soft_dependencies > 0 {
        notes.push(AdaptationNote::new(
            "workflow step dependency",
            format!(
                "{soft_dependencies} dependencies are soft or always-run; IR v1 carries the \
                 ordering but not the rule for continuing past a failed predecessor"
            ),
        ));
    }

    let graph = Graph::new(parts).map_err(|error| AdapterError::NotAdmissible {
        refusal: capsulet_ir::admission::AdmissionRefusal {
            code: capsulet_ir::admission::AdmissionCode::GraphInvalid,
            owner: capsulet_ir::correctness::obligation::RepairOwner::Runtime,
            detail: error.to_string(),
        },
    })?;

    let definition = Definition {
        schema_version: Definition::current_schema_version(),
        id: identifier(workflow.id().as_str(), "workflow id")?,
        version: "1".to_string(),
        name: workflow.name().to_string(),
        // A job DAG declares no obligations, so it observes. Claiming anything
        // else would be inventing assurance the workflow never asked for.
        assurance: AssuranceMode::Observe,
        capabilities: CapabilitySet::new(vec![Capability {
            id: identifier(JOB_EXECUTION, "capability")?,
            grant: Grant::Tool {
                tool: identifier("capsulet/job-runner", "tool")?,
            },
        }])
        .map_err(|error| AdapterError::Unsupported {
            construct: error.to_string(),
        })?,
        budget: ResourceBudget {
            wall_ms: workflow
                .deadline_seconds()
                .map_or(DEFAULT_STEP_MS, |seconds| seconds.saturating_mul(1_000))
                .max(DEFAULT_STEP_MS),
            tokens: 0,
            cost_micro_units: 0,
            effect_count: u32::try_from(workflow.steps().len()).unwrap_or(u32::MAX),
        },
        graph,
        boundaries: vec![],
        contracts: vec![],
    };

    Ok(Adapted { definition, notes })
}

/// What a job hands to the next step: something, structure unknown.
fn opaque_step_value() -> ValueSchema {
    aliases::opaque("a job's inputs and outputs are not modelled by the current workflow DAG")
}

fn identifier(value: &str, what: &'static str) -> Result<Identifier, AdapterError> {
    Identifier::parse(value).map_err(|source| AdapterError::Identifier { what, source })
}
