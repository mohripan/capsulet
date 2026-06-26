use std::{
    collections::{HashMap, HashSet},
    fmt::Write as _,
};

use capsulet_core::{AutomationId, AutomationStatus, ConditionExpr, TriggerName};
use capsulet_observability::{self as observability, tracing::Instrument};
use capsulet_postgres::{PostgresStore, PostgresStoreError, TriggerEvent};
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

const DEFAULT_LEASE_SECONDS: i64 = 60;
const DEFAULT_MAX_ATTEMPTS: i32 = 8;

#[derive(Debug, Clone)]
pub struct Evaluator {
    store: PostgresStore,
    owner: String,
    lease_seconds: i64,
    max_attempts: i32,
    sql_connections: crate::SqlConnections,
}

impl Evaluator {
    #[must_use]
    pub fn new(store: PostgresStore, owner: impl Into<String>) -> Self {
        Self {
            store,
            owner: owner.into(),
            lease_seconds: DEFAULT_LEASE_SECONDS,
            max_attempts: DEFAULT_MAX_ATTEMPTS,
            sql_connections: crate::SqlConnections::default(),
        }
    }

    #[must_use]
    pub fn with_sql_connections(mut self, sql_connections: crate::SqlConnections) -> Self {
        self.sql_connections = sql_connections;
        self
    }

    /// Evaluates at most one durable trigger correlation group.
    ///
    /// # Errors
    ///
    /// Returns [`EvaluatorError`] when production, leasing, evaluation, or completion fails.
    pub async fn tick(&self) -> Result<bool, EvaluatorError> {
        crate::schedule::produce_due_events(&self.store).await?;
        crate::sql_trigger::produce_due_events(&self.store, &self.sql_connections).await?;
        crate::custom_trigger::produce_due_event(
            &self.store,
            &self.owner,
            self.lease_seconds.max(600),
        )
        .await?;
        let events = self
            .store
            .lease_trigger_group(&self.owner, self.lease_seconds)
            .await?;
        let Some(first) = events.first() else {
            return Ok(false);
        };
        let automation_id = first.automation_id.clone();
        let correlation_key = first.correlation_key.clone();
        if let Err(error) = self.evaluate_group(&events).await {
            let retry_seconds = retry_delay_seconds(&events);
            self.store
                .retry_trigger_group(
                    &self.owner,
                    &automation_id,
                    &correlation_key,
                    &error.to_string(),
                    retry_seconds,
                    self.max_attempts,
                )
                .await?;
            return Err(error);
        }
        Ok(true)
    }

    async fn evaluate_group(&self, events: &[TriggerEvent]) -> Result<(), EvaluatorError> {
        let first = events.first().ok_or(EvaluatorError::EmptyGroup)?;
        let span = observability::tracing::info_span!(
            "evaluator.trigger_group",
            owner = %self.owner,
            automation.id = %first.automation_id,
            correlation.key = %first.correlation_key,
            trigger.events = events.len(),
            satisfied = observability::tracing::field::Empty,
            workflow.run.id = observability::tracing::field::Empty,
            outcome = observability::tracing::field::Empty,
            error = observability::tracing::field::Empty,
        );
        async move {
            let result = async {
                if events.iter().any(|event| {
                    event.automation_id != first.automation_id
                        || event.correlation_key != first.correlation_key
                }) {
                    return Err(EvaluatorError::MixedGroup);
                }
                let automation_id = AutomationId::new(first.automation_id.clone())
                    .map_err(EvaluatorError::InvalidDefinition)?;
                let automation = self
                    .store
                    .find_automation(&automation_id)
                    .await?
                    .ok_or_else(|| {
                        EvaluatorError::AutomationNotFound(first.automation_id.clone())
                    })?;
                let (triggers, condition_json) =
                    self.store.list_automation_triggers(&automation_id).await?;
                let enabled: HashMap<_, _> = triggers
                    .iter()
                    .filter(|trigger| trigger.enabled())
                    .map(|trigger| (trigger.name().as_str(), trigger.kind()))
                    .collect();
                let satisfied_triggers = events
                    .iter()
                    .filter(|event| enabled.contains_key(event.trigger_name.as_str()))
                    .map(|event| TriggerName::new(event.trigger_name.clone()))
                    .collect::<Result<HashSet<_>, _>>()
                    .map_err(EvaluatorError::InvalidDefinition)?;
                let condition = condition_from_json(
                    &serde_json::from_str(&condition_json)
                        .map_err(EvaluatorError::InvalidConditionJson)?,
                )?;
                let satisfied = automation.status() == AutomationStatus::Enabled
                    && condition.evaluate(&satisfied_triggers);
                let run_id = deterministic_run_id(&first.automation_id, &first.correlation_key);
                observability::tracing::Span::current()
                    .record("satisfied", satisfied)
                    .record("workflow.run.id", run_id.as_str());
                self.store
                    .complete_trigger_group(
                        &self.owner,
                        &first.automation_id,
                        &first.correlation_key,
                        automation.workflow_id().as_str(),
                        &run_id,
                        automation.job_input_json(),
                        satisfied,
                    )
                    .await?;
                Ok(())
            }
            .await;
            match &result {
                Ok(()) => {
                    observability::tracing::Span::current().record("outcome", "success");
                }
                Err(error) => {
                    observability::tracing::Span::current()
                        .record("outcome", "error")
                        .record("error", observability::tracing::field::display(error));
                }
            }
            result
        }
        .instrument(span)
        .await
    }
}

fn condition_from_json(value: &Value) -> Result<ConditionExpr, EvaluatorError> {
    if let Some(trigger) = value.get("trigger").and_then(Value::as_str) {
        return TriggerName::new(trigger)
            .map(ConditionExpr::Trigger)
            .map_err(EvaluatorError::InvalidDefinition);
    }
    for (field, all) in [("all", true), ("any", false)] {
        if let Some(values) = value.get(field).and_then(Value::as_array) {
            let expressions = values
                .iter()
                .map(condition_from_json)
                .collect::<Result<Vec<_>, _>>()?;
            let expression = if all {
                ConditionExpr::All(expressions)
            } else {
                ConditionExpr::Any(expressions)
            };
            expression
                .validate()
                .map_err(EvaluatorError::InvalidDefinition)?;
            return Ok(expression);
        }
    }
    Err(EvaluatorError::InvalidDefinition(
        "condition must contain trigger, all, or any".to_string(),
    ))
}

fn deterministic_run_id(automation_id: &str, correlation_key: &str) -> String {
    let digest = Sha256::digest(format!("{automation_id}\0{correlation_key}").as_bytes());
    let mut suffix = String::with_capacity(24);
    for byte in &digest[..12] {
        write!(suffix, "{byte:02x}").expect("writing to a String cannot fail");
    }
    format!("trigger_{suffix}")
}

fn retry_delay_seconds(events: &[TriggerEvent]) -> i64 {
    let exponent = u32::try_from(events.len().min(6)).expect("value is capped at six");
    2_i64.pow(exponent).min(60)
}

#[derive(Debug, Error)]
pub enum EvaluatorError {
    #[error(transparent)]
    Store(#[from] PostgresStoreError),
    #[error("trigger event group is empty")]
    EmptyGroup,
    #[error("leased trigger events do not share one automation and correlation key")]
    MixedGroup,
    #[error("automation not found: {0}")]
    AutomationNotFound(String),
    #[error("invalid automation trigger definition: {0}")]
    InvalidDefinition(String),
    #[error("invalid condition JSON: {0}")]
    InvalidConditionJson(#[from] serde_json::Error),
    #[error("system clock error: {0}")]
    Clock(String),
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde_json::json;

    use super::{condition_from_json, deterministic_run_id};

    #[test]
    fn parses_and_evaluates_nested_conditions() {
        let condition = condition_from_json(&json!({
            "any": [{"all": [{"trigger":"ready"}, {"trigger":"approved"}]}, {"trigger":"manual"}]
        }))
        .expect("condition");
        assert!(!condition.evaluate(&HashSet::default()));
    }

    #[test]
    fn workflow_run_identity_is_stable_per_correlation_group() {
        assert_eq!(
            deterministic_run_id("automation", "correlation"),
            deterministic_run_id("automation", "correlation")
        );
        assert_ne!(
            deterministic_run_id("automation", "correlation"),
            deterministic_run_id("automation", "other")
        );
    }
}
