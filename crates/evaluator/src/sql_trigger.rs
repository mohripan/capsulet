use std::{
    collections::HashMap,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use capsulet_postgres::{PostgresStore, TriggerEvent};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, postgres::PgPoolOptions};

use crate::EvaluatorError;

#[derive(Clone, Default)]
pub struct SqlConnections(Arc<HashMap<String, PgPool>>);

impl std::fmt::Debug for SqlConnections {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("SqlConnections")
            .field("count", &self.0.len())
            .finish()
    }
}

impl SqlConnections {
    pub async fn from_json(value: &str) -> Result<Self, EvaluatorError> {
        let urls: HashMap<String, String> = serde_json::from_str(value)?;
        let mut pools = HashMap::with_capacity(urls.len());
        for (name, url) in urls {
            let pool = PgPoolOptions::new()
                .max_connections(2)
                .connect(&url)
                .await
                .map_err(capsulet_postgres::PostgresStoreError::from)?;
            pools.insert(name, pool);
        }
        Ok(Self(Arc::new(pools)))
    }
}

pub(crate) async fn produce_due_events(
    store: &PostgresStore,
    connections: &SqlConnections,
) -> Result<u64, EvaluatorError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| EvaluatorError::Clock(error.to_string()))?
        .as_secs() as i64;
    let mut produced = 0;
    for trigger in store.list_sql_triggers().await? {
        let config: Value = serde_json::from_str(&trigger.config_json)?;
        let connection = config
            .get("connection_name")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                EvaluatorError::InvalidDefinition(
                    "sql trigger requires connection_name".to_string(),
                )
            })?;
        let query = config.get("query").and_then(Value::as_str).ok_or_else(|| {
            EvaluatorError::InvalidDefinition("sql trigger requires query".to_string())
        })?;
        validate_read_query(query)?;
        let poll_seconds = config
            .get("poll_seconds")
            .and_then(Value::as_i64)
            .unwrap_or(60)
            .max(1);
        let Some(pool) = connections.0.get(connection) else {
            return Err(EvaluatorError::InvalidDefinition(format!(
                "unknown SQL connection: {connection}"
            )));
        };
        let mut transaction = pool
            .begin()
            .await
            .map_err(capsulet_postgres::PostgresStoreError::from)?;
        sqlx::query("SET TRANSACTION READ ONLY")
            .execute(&mut *transaction)
            .await
            .map_err(capsulet_postgres::PostgresStoreError::from)?;
        sqlx::query("SET LOCAL statement_timeout = '5s'")
            .execute(&mut *transaction)
            .await
            .map_err(capsulet_postgres::PostgresStoreError::from)?;
        let matched = sqlx::query_scalar::<_, bool>(query)
            .fetch_one(&mut *transaction)
            .await
            .map_err(capsulet_postgres::PostgresStoreError::from)?;
        transaction
            .rollback()
            .await
            .map_err(capsulet_postgres::PostgresStoreError::from)?;
        if !matched {
            continue;
        }
        let bucket = now - now.rem_euclid(poll_seconds);
        let delivery = format!("sql-{bucket}");
        let digest = Sha256::digest(format!(
            "{}\0{}\0{delivery}",
            trigger.automation_id, trigger.trigger_name
        ));
        let id = format!(
            "evt_{}",
            digest[..12]
                .iter()
                .map(|byte| format!("{byte:02x}"))
                .collect::<String>()
        );
        let event = TriggerEvent {
            id,
            automation_id: trigger.automation_id,
            trigger_name: trigger.trigger_name,
            correlation_key: delivery.clone(),
            payload_json: serde_json::json!({"matched": true, "observed_at": now}).to_string(),
            occurred_at: now.to_string(),
        };
        produced += u64::from(store.enqueue_trigger_event(&event, &delivery).await?);
    }
    Ok(produced)
}

fn validate_read_query(query: &str) -> Result<(), EvaluatorError> {
    let normalized = query.trim().trim_end_matches(';').trim();
    if normalized.contains(';')
        || !(normalized.to_ascii_lowercase().starts_with("select ")
            || normalized.to_ascii_lowercase().starts_with("with "))
    {
        return Err(EvaluatorError::InvalidDefinition(
            "SQL trigger query must be one SELECT or WITH statement".to_string(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::validate_read_query;

    #[test]
    fn rejects_multiple_or_mutating_sql_statements() {
        assert!(validate_read_query("SELECT true").is_ok());
        assert!(validate_read_query("WITH value AS (SELECT true) SELECT * FROM value").is_ok());
        assert!(validate_read_query("DELETE FROM users").is_err());
        assert!(validate_read_query("SELECT true; DELETE FROM users").is_err());
    }
}
