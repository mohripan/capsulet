use std::{
    collections::BTreeMap,
    env,
    fmt::Write as _,
    time::{SystemTime, UNIX_EPOCH},
};

use capsulet_core::{
    ExecutionPoolName, JobDefinition, JobDefinitionId, JobRun, JobRunId, JobRunStatus, RetryPolicy,
};
use capsulet_postgres::{CustomRuntimeTrigger, PostgresStore, TriggerEvent};
use capsulet_runner::{
    ExecutionPoolConfig, KubernetesRunner, NeverCancelled, PoolResources, RunExecution, RunOutcome,
    Runner,
};
use serde_json::Value;
use sha2::{Digest, Sha256};

use crate::EvaluatorError;

const DEFAULT_POLL_SECONDS: i64 = 60;
const DEFAULT_TIMEOUT_SECONDS: u64 = 300;
const DEFAULT_LOG_LIMIT_BYTES: usize = 65_536;

pub(crate) async fn produce_due_event(
    store: &PostgresStore,
    owner: &str,
    lease_seconds: i64,
) -> Result<bool, EvaluatorError> {
    let Some(trigger) = store.claim_custom_trigger(owner, lease_seconds).await? else {
        return Ok(false);
    };
    match process_claim(store, owner, &trigger).await {
        Ok(matched) => Ok(matched),
        Err(error) => {
            store
                .fail_custom_trigger(owner, &trigger, &error.to_string(), 30)
                .await?;
            Err(error)
        }
    }
}

async fn process_claim(
    store: &PostgresStore,
    owner: &str,
    trigger: &CustomRuntimeTrigger,
) -> Result<bool, EvaluatorError> {
    let result = execute(trigger).await?;
    let now = epoch_seconds()?;
    let config: Value = serde_json::from_str(&trigger.config_json)?;
    let poll_seconds = config
        .get("poll_seconds")
        .and_then(Value::as_i64)
        .unwrap_or(DEFAULT_POLL_SECONDS)
        .max(1);
    let bucket = trigger.scheduled_epoch;
    let delivery = format!("custom-{bucket}");
    let correlation = result
        .get("correlation_key")
        .and_then(Value::as_str)
        .map_or_else(|| bucket.to_string(), str::to_string);
    let matched = result
        .get("matched")
        .and_then(Value::as_bool)
        .ok_or_else(|| {
            EvaluatorError::InvalidDefinition(
                "custom trigger output requires boolean matched".to_string(),
            )
        })?;
    let event = matched.then(|| TriggerEvent {
        id: deterministic_event_id(trigger, &delivery),
        automation_id: trigger.automation_id.clone(),
        trigger_name: trigger.trigger_name.clone(),
        correlation_key: correlation,
        payload_json: result
            .get("payload")
            .cloned()
            .unwrap_or(Value::Null)
            .to_string(),
        occurred_at: now.to_string(),
    });
    store
        .complete_custom_trigger(owner, trigger, poll_seconds, event.as_ref(), &delivery)
        .await?;
    Ok(matched)
}

async fn execute(trigger: &CustomRuntimeTrigger) -> Result<Value, EvaluatorError> {
    let namespace =
        env::var("CAPSULET_EXECUTION_NAMESPACE").unwrap_or_else(|_| "default".to_string());
    let log_limit = env::var("CAPSULET_LOG_LIMIT_BYTES")
        .ok()
        .map(|value| value.parse())
        .transpose()
        .map_err(|error| EvaluatorError::InvalidDefinition(format!("invalid log limit: {error}")))?
        .unwrap_or(DEFAULT_LOG_LIMIT_BYTES);
    let runner = KubernetesRunner::from_default_config(namespace, log_limit)
        .await
        .map_err(|error| {
            EvaluatorError::InvalidDefinition(format!("custom trigger Kubernetes runner: {error}"))
        })?;
    let now = epoch_seconds()?;
    let digest = short_digest(&format!(
        "{}\0{}\0{}",
        trigger.automation_id, trigger.trigger_name, trigger.scheduled_epoch
    ));
    let definition_id = JobDefinitionId::new(format!("custom-definition-{digest}"))
        .map_err(|error| EvaluatorError::InvalidDefinition(error.clone()))?;
    let definition = JobDefinition::new(
        definition_id.clone(),
        format!("custom trigger {}", trigger.trigger_name),
        trigger.runtime_image.clone(),
        trigger.command.clone(),
        Vec::new(),
        format!("custom-triggers/{digest}"),
        "{}",
        RetryPolicy::no_retry(),
    )
    .map_err(|error| EvaluatorError::InvalidDefinition(error.clone()))?;
    let run_id = JobRunId::new(format!("custom-trigger-{digest}"))
        .map_err(|error| EvaluatorError::InvalidDefinition(error.clone()))?;
    let pool_name = ExecutionPoolName::new("custom-triggers")
        .map_err(|error| EvaluatorError::InvalidDefinition(error.clone()))?;
    let run = JobRun::from_persisted(
        run_id,
        definition_id,
        pool_name,
        trigger.config_json.clone(),
        JobRunStatus::Running,
        1,
        now.to_string(),
    );
    let report = runner
        .execute(
            &RunExecution {
                run,
                definition,
                pool: ExecutionPoolConfig {
                    description: "isolated custom trigger plugins".to_string(),
                    node_selector: BTreeMap::new(),
                    tolerations: Vec::new(),
                    resources: PoolResources::default(),
                    timeout_seconds: DEFAULT_TIMEOUT_SECONDS,
                    max_concurrent_jobs: 1,
                    ttl_seconds_after_finished: Some(300),
                    runtime_class_name: env::var("CAPSULET_EXECUTION_RUNTIME_CLASS").ok(),
                    service_account_name: env::var("CAPSULET_EXECUTION_SERVICE_ACCOUNT").ok(),
                    ..ExecutionPoolConfig::default()
                },
                input_artifacts: Vec::new(),
            },
            &NeverCancelled,
        )
        .await
        .map_err(|error| {
            EvaluatorError::InvalidDefinition(format!("custom trigger execution: {error}"))
        })?;
    if report.outcome != RunOutcome::Succeeded {
        return Err(EvaluatorError::InvalidDefinition(format!(
            "custom trigger exited with {:?}: {}",
            report.outcome,
            report.logs.unwrap_or_default().trim()
        )));
    }
    parse_output(report.logs.as_deref().unwrap_or_default())
}

fn parse_output(logs: &str) -> Result<Value, EvaluatorError> {
    let line = logs
        .lines()
        .rev()
        .find(|line| !line.trim().is_empty())
        .ok_or_else(|| {
            EvaluatorError::InvalidDefinition("custom trigger produced no JSON output".to_string())
        })?;
    let output: Value = serde_json::from_str(line)?;
    if output.get("matched").and_then(Value::as_bool).is_none() {
        return Err(EvaluatorError::InvalidDefinition(
            "custom trigger output requires boolean matched".to_string(),
        ));
    }
    if output
        .get("payload")
        .is_some_and(|payload| !payload.is_object() && !payload.is_null())
    {
        return Err(EvaluatorError::InvalidDefinition(
            "custom trigger payload must be a JSON object or null".to_string(),
        ));
    }
    Ok(output)
}

fn epoch_seconds() -> Result<i64, EvaluatorError> {
    i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| EvaluatorError::Clock(error.to_string()))?
            .as_secs(),
    )
    .map_err(|_| EvaluatorError::Clock("epoch seconds exceed i64".to_string()))
}

fn deterministic_event_id(trigger: &CustomRuntimeTrigger, delivery: &str) -> String {
    format!(
        "evt_{}",
        short_digest(&format!(
            "{}\0{}\0{delivery}",
            trigger.automation_id, trigger.trigger_name
        ))
    )
}

fn short_digest(value: &str) -> String {
    let mut output = String::with_capacity(24);
    for byte in &Sha256::digest(value.as_bytes())[..12] {
        write!(output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use super::parse_output;

    #[test]
    fn parses_last_json_log_line_and_validates_contract() {
        let output = parse_output("diagnostic\n{\"matched\":true,\"payload\":{\"id\":7}}\n")
            .expect("valid plugin output");
        assert_eq!(output["payload"]["id"], 7);
        assert!(parse_output("{\"payload\":{}}\n").is_err());
        assert!(parse_output("{\"matched\":true,\"payload\":[]}").is_err());
    }
}
