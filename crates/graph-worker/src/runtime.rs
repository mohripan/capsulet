//! Lease, fold, decide, execute, append.
//!
//! In that order, every time. The worker takes no decision the decision core
//! could take: it does not choose which node is ready, whether a budget is
//! spent, or whether a claimed effect may be retried. It leases a run, folds
//! its log, asks [`capsulet_runtime::decide`] what may happen, does that one
//! thing, and appends what resulted. Then it folds again.
//!
//! Re-folding after every step is deliberate and is not a performance
//! oversight. It means the worker is always deciding from what is durably
//! written rather than from what it believes it wrote, so a worker that has
//! just lost its lease finds out on the next append instead of continuing
//! against a state nobody else agrees with.
//!
//! Every append carries the epoch the lease was granted under. When one is
//! refused the worker stops immediately: it is no longer the owner, and the
//! run it was advancing belongs to somebody else now.

use std::sync::Arc;
use std::time::Duration;

use capsulet_ir::correctness::evidence::RecordedTime;
use capsulet_ir::definition::Definition;
use capsulet_ir::id::Identifier;
use capsulet_postgres::{IrRunRecord, PostgresStore, PostgresStoreError, SignalOutcome};
use capsulet_runtime::effect::{EffectAttempt, EffectContext};
use capsulet_runtime::failure::{self, Compensation};
use capsulet_runtime::wait::{self, WaitError};
use capsulet_runtime::{Decision, RunEvent, RunFailure, RunState, decide};

use crate::clock::Clock;
use crate::execute::{
    CompensationRequest, EffectOutcome, EffectRequest, Executor, NodeOutcome, NodeRequest,
};

/// How the worker is configured.
#[derive(Debug, Clone)]
pub struct WorkerConfig {
    /// This worker's identity, recorded in every event it writes.
    pub worker_id: String,
    /// Restrict to one tenant's runs, for a dedicated fleet. `None` is the
    /// whole installation.
    pub tenant: Option<String>,
    pub lease_seconds: i64,
    /// How long to wait when there was no work.
    pub idle_poll: Duration,
    /// A ceiling on how many decisions one lease may carry out before the
    /// worker goes back to the queue.
    ///
    /// Not a correctness bound — the log is — but it stops one long run from
    /// monopolising a worker while others wait.
    pub steps_per_lease: u32,
}

impl Default for WorkerConfig {
    fn default() -> Self {
        Self {
            worker_id: "capsulet-graph-worker".to_string(),
            tenant: None,
            lease_seconds: 60,
            idle_poll: Duration::from_secs(2),
            steps_per_lease: 64,
        }
    }
}

/// What one leased pass over a run came to.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Progress {
    /// The queue had nothing to offer.
    NothingToDo,
    /// The run ended, one way or another.
    Ended,
    /// The run is suspended, and the lease was given back.
    Suspended,
    /// The step budget ran out with the run still going.
    StillRunning,
    /// Somebody else owns this run now.
    LeaseLost,
}

/// Why a worker could not carry on.
#[derive(Debug, thiserror::Error)]
pub enum WorkerError {
    #[error("storage: {0}")]
    Storage(#[from] PostgresStoreError),
    /// A foreign key makes this unreachable in a healthy database. It exists
    /// for the one that was restored from a partial backup, where refusing to
    /// execute is the only safe answer.
    #[error("run {run} names definition {digest}, which is not stored")]
    UnknownDefinition { run: String, digest: String },
    #[error("run {run} has a history that does not fold: {reason}")]
    Unfoldable { run: String, reason: String },
    #[error("run {run} declares an effect this runtime cannot perform: {reason}")]
    Undeliverable { run: String, reason: String },
}

/// A worker that advances IR runs.
pub struct GraphWorker {
    store: PostgresStore,
    executor: Arc<dyn Executor>,
    clock: Arc<dyn Clock>,
    config: WorkerConfig,
}

impl GraphWorker {
    #[must_use]
    pub fn new(
        store: PostgresStore,
        executor: Arc<dyn Executor>,
        clock: Arc<dyn Clock>,
        config: WorkerConfig,
    ) -> Self {
        Self {
            store,
            executor,
            clock,
            config,
        }
    }

    /// Leases one run and advances it as far as the step budget allows.
    ///
    /// # Errors
    ///
    /// Returns [`WorkerError`] when storage fails, when the run names a
    /// definition that is not stored, or when its log does not fold — which
    /// would mean the stored history is not one any run could have produced.
    pub async fn advance_one(&self) -> Result<Progress, WorkerError> {
        let now = self.clock.now();
        let Some(lease) = self
            .store
            .lease_next_ir_run(
                &self.config.worker_id,
                self.config.lease_seconds,
                self.config.tenant.as_deref(),
                now.epoch_millis(),
            )
            .await?
        else {
            return Ok(Progress::NothingToDo);
        };

        let definition = self.definition_for(&lease).await?;
        let outcome = self.advance_leased(&lease, &definition).await;

        // A lease is given back whatever happened, so a run that is waiting or
        // has ended does not sit under a dead lease until it expires. Losing
        // the lease is the one case where there is nothing to give back.
        if !matches!(outcome, Ok(Progress::LeaseLost)) {
            self.store
                .release_ir_run_lease(
                    &lease.tenant_id,
                    &lease.project_id,
                    &lease.id,
                    &self.config.worker_id,
                    lease.epoch,
                )
                .await?;
        }
        outcome
    }

    /// Advances a run the worker already holds a lease on.
    async fn advance_leased(
        &self,
        lease: &IrRunRecord,
        definition: &Definition,
    ) -> Result<Progress, WorkerError> {
        for _ in 0..self.config.steps_per_lease {
            let state = self.fold(lease).await?;
            let now = self.clock.now();

            // A signal that arrived while the run was suspended is answered
            // before anything else: it is the reason this run is work again.
            if self.answer_signals(lease, &state, now).await? {
                continue;
            }

            match decide(definition, &state, now) {
                Decision::Idle => {
                    return Ok(if state.status().is_terminal() {
                        Progress::Ended
                    } else if state.status() == capsulet_runtime::RunStatus::Waiting {
                        Progress::Suspended
                    } else {
                        Progress::StillRunning
                    });
                }
                decision => {
                    if !self
                        .carry_out(lease, definition, &state, decision, now)
                        .await?
                    {
                        return Ok(Progress::LeaseLost);
                    }
                }
            }

            // Still alive, still the owner. A heartbeat that says otherwise is
            // the earliest warning the worker gets.
            if !self
                .store
                .heartbeat_ir_run(
                    &lease.tenant_id,
                    &lease.project_id,
                    &lease.id,
                    &self.config.worker_id,
                    lease.epoch,
                    self.config.lease_seconds,
                )
                .await?
            {
                return Ok(Progress::LeaseLost);
            }
        }
        Ok(Progress::StillRunning)
    }

    /// Carries out one decision. Returns whether the worker still owns the run.
    async fn carry_out(
        &self,
        lease: &IrRunRecord,
        definition: &Definition,
        state: &RunState,
        decision: Decision,
        now: RecordedTime,
    ) -> Result<bool, WorkerError> {
        let worker = self.identity();
        match decision {
            Decision::Start => {
                self.append(lease, &RunEvent::Started { by: worker }, now)
                    .await
            }
            Decision::StartNode { node } => self.run_node(lease, definition, &node, now).await,
            Decision::PerformEffect {
                node,
                effect,
                attempt,
            } => {
                self.perform_effect(lease, definition, state, &node, &effect, attempt, None, now)
                    .await
            }
            Decision::RetryClaimedEffect {
                node,
                effect,
                attempt,
            } => {
                // The key comes from the claim the previous attempt left, never
                // from deriving it again: a retry has to present the key the far
                // side already saw or it is a second effect.
                let recovered = state
                    .outstanding_effects()
                    .iter()
                    .find(|claim| claim.node == node && claim.effect == effect)
                    .map(|claim| EffectAttempt::recovered(claim, attempt));
                self.perform_effect(
                    lease, definition, state, &node, &effect, attempt, recovered, now,
                )
                .await
            }
            Decision::HaltUncertainEffect { node, effect } => {
                let attempt = state
                    .outstanding_effects()
                    .iter()
                    .find(|claim| claim.node == node && claim.effect == effect)
                    .map_or(0, |claim| claim.attempt);
                // Record the doubt and stop deciding here. Failing the run is
                // the next decision, not this one — and going through `decide`
                // is what makes anything the run still owes get compensated
                // before it ends.
                self.append(
                    lease,
                    &RunEvent::EffectUncertain {
                        node,
                        effect,
                        attempt,
                    },
                    now,
                )
                .await
            }
            Decision::StopLoop { region, reason } => {
                self.append(lease, &RunEvent::LoopStopped { region, reason }, now)
                    .await
            }
            Decision::TimeOutNode { node } => {
                let Some(timed_out) = failure::timed_out_node(definition, state, now) else {
                    // It finished between the decision and here, which is a
                    // race worth losing quietly.
                    return Ok(true);
                };
                debug_assert_eq!(timed_out.node, node);
                self.append(lease, &failure::timed_out(&timed_out), now)
                    .await
            }
            Decision::Compensate { compensation } => {
                self.compensate(lease, definition, &compensation).await
            }
            Decision::Cancel { by } => self.append(lease, &failure::cancelled(by), now).await,
            Decision::Suspend { wait } => {
                self.append(lease, &RunEvent::Suspended { wait }, now).await
            }
            Decision::ResumeWait { wait } => {
                self.append(lease, &RunEvent::Resumed { wait, by: worker }, now)
                    .await
            }
            Decision::Complete => {
                self.append(
                    lease,
                    &RunEvent::Completed {
                        outputs: state.outputs(),
                    },
                    now,
                )
                .await
            }
            Decision::Fail { reason } => {
                self.append(lease, &RunEvent::Failed { reason }, now).await
            }
            Decision::Idle => Ok(true),
        }
    }

    /// Runs one node and records what happened.
    async fn run_node(
        &self,
        lease: &IrRunRecord,
        definition: &Definition,
        node: &Identifier,
        now: RecordedTime,
    ) -> Result<bool, WorkerError> {
        let Some(declared) = definition.graph.node(node) else {
            return Err(WorkerError::Unfoldable {
                run: lease.id.clone(),
                reason: format!("`{node}` is not in the definition"),
            });
        };

        // Started is written before the work, so a crash inside it leaves a
        // node that started and never finished rather than no trace at all.
        if !self
            .append(lease, &RunEvent::NodeStarted { node: node.clone() }, now)
            .await?
        {
            return Ok(false);
        }

        let outcome = self
            .executor
            .run_node(NodeRequest {
                run: &lease.id,
                definition,
                node: declared,
            })
            .await;

        let event = match outcome {
            NodeOutcome::Finished { outputs } => RunEvent::NodeFinished {
                node: node.clone(),
                outputs,
            },
            NodeOutcome::Failed { failure, detail } => RunEvent::NodeFailed {
                node: node.clone(),
                failure,
                detail,
            },
        };
        self.append(lease, &event, self.clock.now()).await
    }

    /// Claims an effect, performs it, and records how it came out.
    #[allow(
        clippy::too_many_arguments,
        reason = "each argument names something the claim protocol needs, and bundling them would hide the ordering this function exists to enforce"
    )]
    async fn perform_effect(
        &self,
        lease: &IrRunRecord,
        definition: &Definition,
        state: &RunState,
        node: &Identifier,
        effect: &Identifier,
        attempt: u32,
        recovered: Option<EffectAttempt>,
        now: RecordedTime,
    ) -> Result<bool, WorkerError> {
        let declared = definition
            .graph
            .node(node)
            .and_then(|declared| declared.effects.iter().find(|each| &each.id == effect))
            .ok_or_else(|| WorkerError::Unfoldable {
                run: lease.id.clone(),
                reason: format!("`{effect}` is not declared on `{node}`"),
            })?;

        let claim = match recovered {
            Some(recovered) => recovered,
            None => EffectAttempt::claim(
                declared,
                &EffectContext {
                    run: &lease.id,
                    node,
                    attempt,
                },
            )
            .map_err(|error| WorkerError::Undeliverable {
                run: lease.id.clone(),
                reason: error.to_string(),
            })?,
        };

        // Performing an effect *is* the effect node running, so the node opens
        // here and closes when the effect resolves. A recovered attempt finds
        // the node already open and does not open it twice.
        let already_running = state.running().contains_key(node);
        if !already_running
            && !self
                .append(lease, &RunEvent::NodeStarted { node: node.clone() }, now)
                .await?
        {
            return Ok(false);
        }

        // Claim, then perform. The gap between them is the only window in which
        // a crash leaves a question, and the claim is what makes the question
        // answerable.
        if !self.append(lease, &claim.claimed(), now).await? {
            return Ok(false);
        }

        let outcome = self
            .executor
            .perform_effect(EffectRequest {
                run: &lease.id,
                node,
                effect: declared,
                attempt: claim.attempt(),
                key: claim.key(),
            })
            .await;

        let at = self.clock.now();
        match outcome {
            EffectOutcome::Performed { receipt } => {
                if !self.append(lease, &claim.finalized(receipt), at).await? {
                    return Ok(false);
                }
                self.append(
                    lease,
                    &RunEvent::NodeFinished {
                        node: node.clone(),
                        outputs: std::collections::BTreeMap::new(),
                    },
                    at,
                )
                .await
            }
            EffectOutcome::Failed { failure, detail } => {
                // It definitely did not happen, so the claim is abandoned
                // rather than left in doubt, and the node's failure is what the
                // run has to answer.
                if !self
                    .append(lease, &claim.abandoned(detail.clone()), at)
                    .await?
                {
                    return Ok(false);
                }
                self.append(
                    lease,
                    &RunEvent::NodeFailed {
                        node: node.clone(),
                        failure,
                        detail,
                    },
                    at,
                )
                .await
            }
            EffectOutcome::Uncertain { .. } => {
                // Leave the claim outstanding. The next fold sees it, and what
                // happens then follows from the declared idempotency rather
                // than from anything this worker decides.
                Ok(true)
            }
        }
    }

    /// Undoes an effect that happened, before the run is allowed to end.
    async fn compensate(
        &self,
        lease: &IrRunRecord,
        definition: &Definition,
        compensation: &Compensation,
    ) -> Result<bool, WorkerError> {
        let declared = definition
            .graph
            .node(&compensation.node)
            .and_then(|node| {
                node.effects
                    .iter()
                    .find(|each| each.id == compensation.effect)
            })
            .ok_or_else(|| WorkerError::Unfoldable {
                run: lease.id.clone(),
                reason: format!(
                    "`{}` is not declared on `{}`",
                    compensation.effect, compensation.node
                ),
            })?;

        let outcome = self
            .executor
            .compensate(CompensationRequest {
                run: &lease.id,
                node: &compensation.node,
                effect: declared,
                route: &compensation.route,
            })
            .await;

        let at = self.clock.now();
        match outcome {
            EffectOutcome::Performed { receipt } => {
                self.append(lease, &failure::compensated(compensation, receipt), at)
                    .await
            }
            // A compensation that did not happen, or might not have, is not
            // something to record as done. The run stops with the effect still
            // standing, which is the truth and is what somebody needs to see.
            EffectOutcome::Failed { .. } | EffectOutcome::Uncertain { .. } => {
                self.append(
                    lease,
                    &RunEvent::Failed {
                        reason: RunFailure::EffectUncertain {
                            node: compensation.node.clone(),
                            effect: compensation.effect.clone(),
                        },
                    },
                    at,
                )
                .await
            }
        }
    }

    /// Answers whatever arrived in the run's inbox. Returns whether it resumed.
    async fn answer_signals(
        &self,
        lease: &IrRunRecord,
        state: &RunState,
        now: RecordedTime,
    ) -> Result<bool, WorkerError> {
        let pending = self
            .store
            .pending_ir_run_signals(&lease.tenant_id, &lease.project_id, &lease.id)
            .await?;
        if pending.is_empty() {
            return Ok(false);
        }

        let mut resumed = false;
        for signal in pending {
            let outcome = if resumed {
                // The run is running again, so anything else in the inbox is a
                // duplicate delivery rather than a second thing to answer.
                SignalOutcome::Refused
            } else {
                match wait::resume(state.waiting_on(), &signal.signal, now) {
                    Ok(event) => {
                        let recorded = self.append_recorded(lease, &event, now).await?;
                        match recorded {
                            Some(position) => {
                                resumed = true;
                                SignalOutcome::Resumed { position }
                            }
                            None => return Ok(false),
                        }
                    }
                    Err(WaitError::NotWaiting) => {
                        // The run is not suspended yet. A webhook can arrive
                        // before the run reaches the wait it answers, so this
                        // one stays in the inbox. It costs nothing: a running
                        // run is work whether or not its inbox has anything.
                        continue;
                    }
                    Err(WaitError::NotDue { .. }) => {
                        // A scheduler that fired early. Answering it "not yet"
                        // and keeping it would leave the run permanently
                        // leasable and the worker spinning on it; the wake-up
                        // comes from the stored wake time, which is the durable
                        // mechanism and does not need this signal.
                        SignalOutcome::Refused
                    }
                    Err(_) => SignalOutcome::Refused,
                }
            };
            self.store.consume_ir_run_signal(signal.id, outcome).await?;
        }
        Ok(resumed)
    }

    /// Appends an event under the lease's epoch.
    ///
    /// Returns whether the worker still owns the run. A refused append is not
    /// an error to retry — it is the answer to a question the worker did not
    /// know it was asking.
    async fn append(
        &self,
        lease: &IrRunRecord,
        event: &RunEvent,
        at: RecordedTime,
    ) -> Result<bool, WorkerError> {
        Ok(self.append_recorded(lease, event, at).await?.is_some())
    }

    async fn append_recorded(
        &self,
        lease: &IrRunRecord,
        event: &RunEvent,
        at: RecordedTime,
    ) -> Result<Option<u64>, WorkerError> {
        match self
            .store
            .append_ir_run_event(
                &lease.tenant_id,
                &lease.project_id,
                &lease.id,
                lease.epoch,
                event,
                at,
            )
            .await
        {
            Ok(recorded) => Ok(Some(recorded.position)),
            Err(PostgresStoreError::EpochSuperseded { .. }) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    async fn fold(&self, lease: &IrRunRecord) -> Result<RunState, WorkerError> {
        let events = self
            .store
            .load_ir_run_events(&lease.tenant_id, &lease.project_id, &lease.id)
            .await?;
        RunState::fold(&events).map_err(|error| WorkerError::Unfoldable {
            run: lease.id.clone(),
            reason: error.to_string(),
        })
    }

    async fn definition_for(&self, lease: &IrRunRecord) -> Result<Definition, WorkerError> {
        let stored = self
            .store
            .get_ir_definition_version(
                &lease.tenant_id,
                &lease.project_id,
                &lease.definition_digest,
            )
            .await?
            .ok_or_else(|| WorkerError::UnknownDefinition {
                run: lease.id.clone(),
                digest: lease.definition_digest.clone(),
            })?;
        Ok(stored.definition()?)
    }

    /// This worker, as an identifier events can carry.
    ///
    /// Falls back to a fixed name when the configured id is not a legal
    /// identifier, because a run should not fail over how a worker was named.
    fn identity(&self) -> Identifier {
        Identifier::parse(&self.config.worker_id)
            .unwrap_or_else(|_| Identifier::parse("graph-worker").expect("a legal identifier"))
    }
}
