use std::str::FromStr;

use chrono::{TimeZone, Utc};
use chrono_tz::Tz;
use cron::Schedule;
use serde_json::Value;

use crate::EvaluatorError;
use capsulet_postgres::{PostgresStore, ScheduleTrigger};

pub(crate) async fn produce_due_events(store: &PostgresStore) -> Result<u64, EvaluatorError> {
    let mut fired = 0;
    for trigger in store.list_schedule_triggers().await? {
        match produce_one(store, &trigger).await {
            Ok(produced) => {
                fired += u64::from(produced);
                store
                    .record_trigger_runtime_success(&trigger.automation_id, &trigger.trigger_name)
                    .await?;
            }
            Err(error) => {
                store
                    .record_trigger_runtime_failure(
                        &trigger.automation_id,
                        &trigger.trigger_name,
                        &error.to_string(),
                    )
                    .await?;
            }
        }
    }
    Ok(fired)
}

async fn produce_one(
    store: &PostgresStore,
    trigger: &ScheduleTrigger,
) -> Result<bool, EvaluatorError> {
    let config: Value = serde_json::from_str(&trigger.config_json)?;
    let expression = config.get("cron").and_then(Value::as_str).ok_or_else(|| {
        EvaluatorError::InvalidDefinition("schedule trigger requires cron".to_string())
    })?;
    let timezone = config
        .get("timezone")
        .and_then(Value::as_str)
        .unwrap_or("UTC");
    let timezone = Tz::from_str(timezone).map_err(|_| {
        EvaluatorError::InvalidDefinition(format!("unknown schedule timezone: {timezone}"))
    })?;
    let schedule = Schedule::from_str(expression).map_err(|error| {
        EvaluatorError::InvalidDefinition(format!("invalid cron expression: {error}"))
    })?;
    let now = Utc::now();
    let initial = schedule
        .after(&now.with_timezone(&timezone))
        .next()
        .ok_or_else(|| {
            EvaluatorError::InvalidDefinition(
                "cron expression has no future occurrence".to_string(),
            )
        })?
        .timestamp();
    let due = store
        .get_or_init_schedule_cursor(&trigger.automation_id, &trigger.trigger_name, initial)
        .await?;
    if due > now.timestamp() {
        return Ok(false);
    }
    let due_time = timezone
        .timestamp_opt(due, 0)
        .single()
        .ok_or_else(|| EvaluatorError::InvalidDefinition("invalid schedule cursor".to_string()))?;
    let next = schedule
        .after(&due_time)
        .next()
        .ok_or_else(|| {
            EvaluatorError::InvalidDefinition("cron expression has no next occurrence".to_string())
        })?
        .timestamp();
    store
        .fire_due_schedule(trigger, due, next)
        .await
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use cron::Schedule;
    use std::str::FromStr;

    #[test]
    fn accepts_seven_field_cron_expressions() {
        assert!(Schedule::from_str("0 */5 * * * * *").is_ok());
        assert!(Schedule::from_str("not cron").is_err());
    }
}
