use std::collections::{BTreeMap, BTreeSet};

use thiserror::Error;

use super::{WorkflowId, WorkflowStep, WorkflowStepDependency, WorkflowStepId};

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum WorkflowGraphError {
    #[error(
        "step {step_id} belongs to workflow {actual_workflow_id}, expected {expected_workflow_id}"
    )]
    StepFromAnotherWorkflow {
        step_id: WorkflowStepId,
        actual_workflow_id: WorkflowId,
        expected_workflow_id: WorkflowId,
    },
    #[error("dependency endpoint {step_id} does not exist in the workflow")]
    UnknownStep { step_id: WorkflowStepId },
    #[error("step {step_id} cannot depend on itself")]
    SelfDependency { step_id: WorkflowStepId },
    #[error("duplicate dependency {from_step_id} -> {to_step_id}")]
    DuplicateDependency {
        from_step_id: WorkflowStepId,
        to_step_id: WorkflowStepId,
    },
    #[error("workflow dependencies contain a cycle")]
    CycleDetected,
    #[error("duplicate workflow step id {step_id}")]
    DuplicateStepId { step_id: WorkflowStepId },
}

#[derive(Debug, Clone)]
pub struct WorkflowGraph {
    nodes: BTreeMap<WorkflowStepId, WorkflowStep>,
    outgoing: BTreeMap<WorkflowStepId, Vec<WorkflowStepId>>,
    incoming: BTreeMap<WorkflowStepId, Vec<WorkflowStepId>>,
    topological_order: Vec<WorkflowStepId>,
}

impl WorkflowGraph {
    /// Builds and validates a workflow-local directed acyclic graph.
    ///
    /// # Errors
    ///
    /// Returns [`WorkflowGraphError`] for cross-workflow or duplicate nodes,
    /// invalid/duplicate/self edges, and dependency cycles.
    pub fn new(
        workflow_id: &WorkflowId,
        steps: &[WorkflowStep],
        dependencies: &[WorkflowStepDependency],
    ) -> Result<Self, WorkflowGraphError> {
        let mut nodes = BTreeMap::new();
        for step in steps {
            if step.workflow_id() != workflow_id {
                return Err(WorkflowGraphError::StepFromAnotherWorkflow {
                    step_id: step.id().clone(),
                    actual_workflow_id: step.workflow_id().clone(),
                    expected_workflow_id: workflow_id.clone(),
                });
            }
            if nodes.insert(step.id().clone(), step.clone()).is_some() {
                return Err(WorkflowGraphError::DuplicateStepId {
                    step_id: step.id().clone(),
                });
            }
        }
        let mut outgoing = nodes
            .keys()
            .cloned()
            .map(|id| (id, Vec::new()))
            .collect::<BTreeMap<_, _>>();
        let mut incoming = outgoing.clone();
        let mut edges = BTreeSet::new();
        for dependency in dependencies {
            let from = dependency.from_step_id();
            let to = dependency.to_step_id();
            if !nodes.contains_key(from) {
                return Err(WorkflowGraphError::UnknownStep {
                    step_id: from.clone(),
                });
            }
            if !nodes.contains_key(to) {
                return Err(WorkflowGraphError::UnknownStep {
                    step_id: to.clone(),
                });
            }
            if from == to {
                return Err(WorkflowGraphError::SelfDependency {
                    step_id: from.clone(),
                });
            }
            if !edges.insert((from.clone(), to.clone())) {
                return Err(WorkflowGraphError::DuplicateDependency {
                    from_step_id: from.clone(),
                    to_step_id: to.clone(),
                });
            }
            let Some(children) = outgoing.get_mut(from) else {
                return Err(WorkflowGraphError::UnknownStep {
                    step_id: from.clone(),
                });
            };
            children.push(to.clone());
            let Some(parents) = incoming.get_mut(to) else {
                return Err(WorkflowGraphError::UnknownStep {
                    step_id: to.clone(),
                });
            };
            parents.push(from.clone());
        }
        for adjacent in outgoing.values_mut() {
            adjacent.sort();
        }
        for adjacent in incoming.values_mut() {
            adjacent.sort();
        }
        let topological_order = Self::sort(&nodes, &outgoing, &incoming)?;
        Ok(Self {
            nodes,
            outgoing,
            incoming,
            topological_order,
        })
    }

    fn sort(
        nodes: &BTreeMap<WorkflowStepId, WorkflowStep>,
        outgoing: &BTreeMap<WorkflowStepId, Vec<WorkflowStepId>>,
        incoming: &BTreeMap<WorkflowStepId, Vec<WorkflowStepId>>,
    ) -> Result<Vec<WorkflowStepId>, WorkflowGraphError> {
        let mut degrees = incoming
            .iter()
            .map(|(id, parents)| (id.clone(), parents.len()))
            .collect::<BTreeMap<_, _>>();
        let mut ready = BTreeSet::new();
        for (id, degree) in &degrees {
            if *degree == 0
                && let Some(step) = nodes.get(id)
            {
                ready.insert((step.position(), id.clone()));
            }
        }
        let mut result = Vec::with_capacity(nodes.len());
        while let Some(key) = ready.pop_first() {
            let id = key.1;
            result.push(id.clone());
            if let Some(children) = outgoing.get(&id) {
                for child in children {
                    if let Some(degree) = degrees.get_mut(child) {
                        *degree -= 1;
                        if *degree == 0
                            && let Some(step) = nodes.get(child)
                        {
                            ready.insert((step.position(), child.clone()));
                        }
                    }
                }
            }
        }
        if result.len() != nodes.len() {
            return Err(WorkflowGraphError::CycleDetected);
        }
        Ok(result)
    }

    #[must_use]
    pub fn topological_order(&self) -> &[WorkflowStepId] {
        &self.topological_order
    }

    #[must_use]
    pub fn roots(&self) -> Vec<&WorkflowStep> {
        self.topological_order
            .iter()
            .filter(|id| self.incoming.get(*id).is_some_and(Vec::is_empty))
            .filter_map(|id| self.nodes.get(id))
            .collect()
    }

    #[must_use]
    pub fn ready_steps(
        &self,
        started: &BTreeSet<WorkflowStepId>,
        succeeded: &BTreeSet<WorkflowStepId>,
    ) -> Vec<&WorkflowStep> {
        self.topological_order
            .iter()
            .filter(|id| {
                !started.contains(*id)
                    && self.incoming.get(*id).is_some_and(|parents| {
                        parents.iter().all(|parent| succeeded.contains(parent))
                    })
            })
            .filter_map(|id| self.nodes.get(id))
            .collect()
    }

    #[must_use]
    pub fn incoming(&self, id: &WorkflowStepId) -> Option<&[WorkflowStepId]> {
        self.incoming.get(id).map(Vec::as_slice)
    }

    #[must_use]
    pub fn outgoing(&self, id: &WorkflowStepId) -> Option<&[WorkflowStepId]> {
        self.outgoing.get(id).map(Vec::as_slice)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ExecutionPoolName, JobDefinitionId};

    fn id(value: &str) -> WorkflowStepId {
        WorkflowStepId::new(value).unwrap()
    }
    fn workflow_id(value: &str) -> WorkflowId {
        WorkflowId::new(value).unwrap()
    }
    fn step(workflow: &WorkflowId, value: &str, position: i32) -> WorkflowStep {
        WorkflowStep::new(
            id(value),
            workflow.clone(),
            position,
            value,
            JobDefinitionId::new(format!("job_{value}")).unwrap(),
            ExecutionPoolName::new("mini").unwrap(),
        )
    }
    fn edge(from: &str, to: &str) -> WorkflowStepDependency {
        WorkflowStepDependency::new(id(from), id(to))
    }

    #[test]
    fn accepts_fan_out_and_fan_in_with_deterministic_order() {
        let workflow = workflow_id("workflow");
        let steps = vec![
            step(&workflow, "merge", 4),
            step(&workflow, "orders", 2),
            step(&workflow, "email", 5),
            step(&workflow, "customers", 1),
            step(&workflow, "cleanup", 3),
        ];
        let graph = WorkflowGraph::new(
            &workflow,
            &steps,
            &[
                edge("customers", "merge"),
                edge("orders", "merge"),
                edge("merge", "email"),
                edge("cleanup", "email"),
            ],
        )
        .unwrap();
        assert_eq!(
            graph
                .topological_order()
                .iter()
                .map(WorkflowStepId::as_str)
                .collect::<Vec<_>>(),
            ["customers", "orders", "cleanup", "merge", "email"]
        );
    }

    #[test]
    fn finds_all_roots_and_waits_for_every_fan_in_parent() {
        let workflow = workflow_id("workflow");
        let steps = vec![
            step(&workflow, "a", 1),
            step(&workflow, "b", 2),
            step(&workflow, "merge", 3),
        ];
        let graph =
            WorkflowGraph::new(&workflow, &steps, &[edge("a", "merge"), edge("b", "merge")])
                .unwrap();
        let started = BTreeSet::from([id("a"), id("b")]);
        assert!(
            graph
                .ready_steps(&started, &BTreeSet::from([id("a")]))
                .is_empty()
        );
        assert_eq!(
            graph.ready_steps(&started, &BTreeSet::from([id("a"), id("b")]))[0]
                .id()
                .as_str(),
            "merge"
        );
    }

    #[test]
    fn rejects_invalid_edges_and_cycles() {
        let workflow = workflow_id("workflow");
        let steps = vec![
            step(&workflow, "a", 1),
            step(&workflow, "b", 2),
            step(&workflow, "c", 3),
        ];
        assert!(matches!(
            WorkflowGraph::new(&workflow, &steps, &[edge("missing", "a")]),
            Err(WorkflowGraphError::UnknownStep { .. })
        ));
        assert!(matches!(
            WorkflowGraph::new(&workflow, &steps, &[edge("a", "a")]),
            Err(WorkflowGraphError::SelfDependency { .. })
        ));
        assert!(matches!(
            WorkflowGraph::new(&workflow, &steps, &[edge("a", "b"), edge("a", "b")]),
            Err(WorkflowGraphError::DuplicateDependency { .. })
        ));
        assert!(matches!(
            WorkflowGraph::new(
                &workflow,
                &steps,
                &[edge("a", "b"), edge("b", "c"), edge("c", "a")]
            ),
            Err(WorkflowGraphError::CycleDetected)
        ));
    }

    #[test]
    fn rejects_step_from_another_workflow() {
        let workflow = workflow_id("workflow");
        assert!(matches!(
            WorkflowGraph::new(&workflow, &[step(&workflow_id("other"), "a", 1)], &[]),
            Err(WorkflowGraphError::StepFromAnotherWorkflow { .. })
        ));
    }
}
