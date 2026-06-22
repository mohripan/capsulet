use sqlx::Row;

use crate::{PostgresStore, PostgresStoreError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TriggerEvent {
    pub id: String,
    pub automation_id: String,
    pub trigger_name: String,
    pub correlation_key: String,
    pub payload_json: String,
    pub occurred_at: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScheduleTrigger {
    pub automation_id: String,
    pub trigger_name: String,
    pub config_json: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomRuntimeTrigger {
    pub automation_id: String,
    pub trigger_name: String,
    pub config_json: String,
    pub runtime_image: String,
    pub command: Vec<String>,
    pub scheduled_epoch: i64,
}

impl PostgresStore {
    /// Clears the persisted failure state after a successful trigger poll.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn record_trigger_runtime_success(
        &self,
        automation_id: &str,
        trigger_name: &str,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"INSERT INTO trigger_runtime_status
               (automation_id, trigger_name, last_error, consecutive_failures, last_success_at)
               VALUES ($1, $2, NULL, 0, now())
               ON CONFLICT (automation_id, trigger_name) DO UPDATE
               SET last_error = NULL, consecutive_failures = 0, last_success_at = now(), updated_at = now()",
        )
        .bind(automation_id)
        .bind(trigger_name)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Records a trigger poll failure without stopping other triggers.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn record_trigger_runtime_failure(
        &self,
        automation_id: &str,
        trigger_name: &str,
        error: &str,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"INSERT INTO trigger_runtime_status
               (automation_id, trigger_name, last_error, consecutive_failures)
               VALUES ($1, $2, $3, 1)
               ON CONFLICT (automation_id, trigger_name) DO UPDATE
               SET last_error = EXCLUDED.last_error,
                   consecutive_failures = trigger_runtime_status.consecutive_failures + 1,
                   updated_at = now()",
        )
        .bind(automation_id)
        .bind(trigger_name)
        .bind(error)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Exclusively claims the next due custom trigger for execution.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the transaction or decoding fails.
    pub async fn claim_custom_trigger(
        &self,
        owner: &str,
        lease_seconds: i64,
    ) -> Result<Option<CustomRuntimeTrigger>, PostgresStoreError> {
        let mut transaction = self.pool.begin().await?;
        sqlx::query(
            r"INSERT INTO custom_trigger_runtime (automation_id, trigger_name)
               SELECT trigger.automation_id, trigger.name
               FROM automation_triggers trigger
               JOIN automations automation ON automation.id = trigger.automation_id
               WHERE trigger.kind = 'custom' AND trigger.enabled AND automation.status = 'enabled'
               ON CONFLICT DO NOTHING",
        )
        .execute(&mut *transaction)
        .await?;
        let row = sqlx::query(
            r"WITH candidate AS (
                   SELECT runtime.automation_id, runtime.trigger_name
                   FROM custom_trigger_runtime runtime
                   JOIN automation_triggers trigger
                     ON trigger.automation_id = runtime.automation_id AND trigger.name = runtime.trigger_name
                   JOIN automations automation ON automation.id = runtime.automation_id
                   WHERE runtime.next_poll_at <= now()
                     AND (runtime.lease_owner IS NULL OR runtime.lease_expires_at <= now())
                     AND trigger.enabled AND automation.status = 'enabled'
                   ORDER BY runtime.next_poll_at, runtime.automation_id, runtime.trigger_name
                   FOR UPDATE OF runtime SKIP LOCKED LIMIT 1
               ), leased AS (
                   UPDATE custom_trigger_runtime runtime
                   SET lease_owner = $1, lease_expires_at = now() + make_interval(secs => $2),
                       updated_at = now()
                   FROM candidate
                   WHERE runtime.automation_id = candidate.automation_id
                     AND runtime.trigger_name = candidate.trigger_name
                   RETURNING runtime.automation_id, runtime.trigger_name, runtime.next_poll_at
               )
               SELECT leased.automation_id, leased.trigger_name, trigger.config::text AS config_json,
                      plugin.runtime_image, plugin.command,
                      extract(epoch FROM leased.next_poll_at)::bigint AS scheduled_epoch
               FROM leased
               JOIN automation_triggers trigger
                 ON trigger.automation_id = leased.automation_id AND trigger.name = leased.trigger_name
               JOIN custom_trigger_plugins plugin ON plugin.id = trigger.plugin_id",
        )
        .bind(owner)
        .bind(lease_seconds)
        .fetch_optional(&mut *transaction)
        .await?;
        transaction.commit().await?;
        row.map(|row| {
            Ok(CustomRuntimeTrigger {
                automation_id: row.try_get("automation_id")?,
                trigger_name: row.try_get("trigger_name")?,
                config_json: row.try_get("config_json")?,
                runtime_image: row.try_get("runtime_image")?,
                command: row.try_get("command")?,
                scheduled_epoch: row.try_get("scheduled_epoch")?,
            })
        })
        .transpose()
    }

    /// Completes a custom-trigger claim and atomically enqueues its event.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the transaction fails.
    pub async fn complete_custom_trigger(
        &self,
        owner: &str,
        trigger: &CustomRuntimeTrigger,
        poll_seconds: i64,
        event: Option<&TriggerEvent>,
        idempotency_key: &str,
    ) -> Result<bool, PostgresStoreError> {
        let mut transaction = self.pool.begin().await?;
        let inserted = if let Some(event) = event {
            sqlx::query(
                r"INSERT INTO trigger_events
                   (id, automation_id, trigger_name, correlation_key, idempotency_key, payload, occurred_at)
                   VALUES ($1, $2, $3, $4, $5, $6::jsonb, to_timestamp($7::double precision))
                   ON CONFLICT (automation_id, trigger_name, idempotency_key) DO NOTHING",
            )
            .bind(&event.id)
            .bind(&event.automation_id)
            .bind(&event.trigger_name)
            .bind(&event.correlation_key)
            .bind(idempotency_key)
            .bind(&event.payload_json)
            .bind(&event.occurred_at)
            .execute(&mut *transaction)
            .await?
            .rows_affected()
                == 1
        } else {
            false
        };
        sqlx::query(
            r"UPDATE custom_trigger_runtime
               SET next_poll_at = now() + make_interval(secs => $4), lease_owner = NULL,
                   lease_expires_at = NULL, last_error = NULL, updated_at = now()
               WHERE automation_id = $1 AND trigger_name = $2 AND lease_owner = $3",
        )
        .bind(&trigger.automation_id)
        .bind(&trigger.trigger_name)
        .bind(owner)
        .bind(poll_seconds.max(1))
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(inserted)
    }

    /// Releases a failed custom-trigger claim with a durable retry schedule.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn fail_custom_trigger(
        &self,
        owner: &str,
        trigger: &CustomRuntimeTrigger,
        error: &str,
        retry_seconds: i64,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"UPDATE custom_trigger_runtime
               SET next_poll_at = now() + make_interval(secs => $4), lease_owner = NULL,
                   lease_expires_at = NULL, last_error = $3, updated_at = now()
               WHERE automation_id = $1 AND trigger_name = $2 AND lease_owner = $5",
        )
        .bind(&trigger.automation_id)
        .bind(&trigger.trigger_name)
        .bind(error)
        .bind(retry_seconds.max(1))
        .bind(owner)
        .execute(&self.pool)
        .await?;
        Ok(())
    }

    /// Lists enabled cron triggers and their runtime configuration.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when querying or decoding fails.
    pub async fn list_schedule_triggers(&self) -> Result<Vec<ScheduleTrigger>, PostgresStoreError> {
        self.list_runtime_triggers("schedule").await
    }

    /// Lists enabled SQL triggers and their runtime configuration.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when querying or decoding fails.
    pub async fn list_sql_triggers(&self) -> Result<Vec<ScheduleTrigger>, PostgresStoreError> {
        self.list_runtime_triggers("sql").await
    }

    async fn list_runtime_triggers(
        &self,
        kind: &str,
    ) -> Result<Vec<ScheduleTrigger>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT trigger.automation_id, trigger.name AS trigger_name,
                   trigger.config::text AS config_json
            FROM automation_triggers trigger
            JOIN automations automation ON automation.id = trigger.automation_id
            WHERE trigger.kind = $1 AND trigger.enabled AND automation.status = 'enabled'
            ORDER BY trigger.automation_id, trigger.name
            ",
        )
        .bind(kind)
        .fetch_all(&self.pool)
        .await?;
        rows.into_iter()
            .map(|row| {
                Ok(ScheduleTrigger {
                    automation_id: row.try_get("automation_id")?,
                    trigger_name: row.try_get("trigger_name")?,
                    config_json: row.try_get("config_json")?,
                })
            })
            .collect()
    }

    /// Creates a schedule cursor if absent and returns its current epoch.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn get_or_init_schedule_cursor(
        &self,
        automation_id: &str,
        trigger_name: &str,
        initial_epoch: i64,
    ) -> Result<i64, PostgresStoreError> {
        sqlx::query(
            r"
            INSERT INTO trigger_schedule_cursors (automation_id, trigger_name, next_fire_at)
            VALUES ($1, $2, to_timestamp($3))
            ON CONFLICT (automation_id, trigger_name) DO NOTHING
            ",
        )
        .bind(automation_id)
        .bind(trigger_name)
        .bind(initial_epoch)
        .execute(&self.pool)
        .await?;
        Ok(sqlx::query_scalar::<_, i64>(
            "SELECT extract(epoch FROM next_fire_at)::bigint FROM trigger_schedule_cursors WHERE automation_id = $1 AND trigger_name = $2"
        ).bind(automation_id).bind(trigger_name).fetch_one(&self.pool).await?)
    }

    /// Atomically advances a due cursor and enqueues its trigger event.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the transaction fails.
    pub async fn fire_due_schedule(
        &self,
        schedule: &ScheduleTrigger,
        due_epoch: i64,
        next_epoch: i64,
    ) -> Result<bool, PostgresStoreError> {
        let mut transaction = self.pool.begin().await?;
        let updated = sqlx::query(
            r"
            UPDATE trigger_schedule_cursors
            SET last_fire_at = next_fire_at, next_fire_at = to_timestamp($4), updated_at = now()
            WHERE automation_id = $1 AND trigger_name = $2
              AND next_fire_at = to_timestamp($3) AND next_fire_at <= now()
            ",
        )
        .bind(&schedule.automation_id)
        .bind(&schedule.trigger_name)
        .bind(due_epoch)
        .bind(next_epoch)
        .execute(&mut *transaction)
        .await?;
        if updated.rows_affected() == 0 {
            transaction.commit().await?;
            return Ok(false);
        }
        let delivery = format!("schedule-{due_epoch}");
        let id = format!(
            "evt_{}_{}_{}",
            schedule.automation_id, schedule.trigger_name, due_epoch
        );
        sqlx::query(
            r"
            INSERT INTO trigger_events (
                id, automation_id, trigger_name, correlation_key, idempotency_key,
                payload, occurred_at
            ) VALUES ($1, $2, $3, $4, $5, jsonb_build_object('scheduled_at', $6), to_timestamp($6))
            ON CONFLICT (automation_id, trigger_name, idempotency_key) DO NOTHING
            ",
        )
        .bind(id)
        .bind(&schedule.automation_id)
        .bind(&schedule.trigger_name)
        .bind(due_epoch.to_string())
        .bind(delivery)
        .bind(due_epoch)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(true)
    }
    /// Inserts an idempotent trigger event and returns whether it was new.
    /// Enqueues an idempotent durable trigger event.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn enqueue_trigger_event(
        &self,
        event: &TriggerEvent,
        idempotency_key: &str,
    ) -> Result<bool, PostgresStoreError> {
        let result = sqlx::query(
            r"
            INSERT INTO trigger_events (
                id, automation_id, trigger_name, correlation_key, idempotency_key, payload, occurred_at
            ) VALUES ($1, $2, $3, $4, $5, $6::jsonb, to_timestamp($7::double precision))
            ON CONFLICT (automation_id, trigger_name, idempotency_key) DO NOTHING
            ",
        )
        .bind(&event.id)
        .bind(&event.automation_id)
        .bind(&event.trigger_name)
        .bind(&event.correlation_key)
        .bind(idempotency_key)
        .bind(&event.payload_json)
        .bind(&event.occurred_at)
        .execute(&self.pool)
        .await?;
        Ok(result.rows_affected() == 1)
    }

    /// Leases one due correlation group and returns all events known for it.
    /// Leases the next correlation group for condition evaluation.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the transaction or decoding fails.
    pub async fn lease_trigger_group(
        &self,
        owner: &str,
        lease_seconds: i64,
    ) -> Result<Vec<TriggerEvent>, PostgresStoreError> {
        let mut transaction = self.pool.begin().await?;
        let candidate = sqlx::query(
            r"
            SELECT id, automation_id, correlation_key
            FROM trigger_events
            WHERE available_at <= now()
              AND (status = 'pending' OR (status = 'leased' AND lease_expires_at <= now()))
            ORDER BY occurred_at, id
            FOR UPDATE SKIP LOCKED
            LIMIT 1
            ",
        )
        .fetch_optional(&mut *transaction)
        .await?;
        let Some(candidate) = candidate else {
            transaction.commit().await?;
            return Ok(Vec::new());
        };
        let candidate_id: String = candidate.try_get("id")?;
        let automation_id: String = candidate.try_get("automation_id")?;
        let correlation_key: String = candidate.try_get("correlation_key")?;
        sqlx::query(
            r"
            UPDATE trigger_events
            SET status = 'leased', lease_owner = $3,
                lease_expires_at = now() + make_interval(secs => $4),
                attempt_count = attempt_count + 1, updated_at = now()
            WHERE id = $1
              AND (status = 'pending' OR (status = 'leased' AND lease_expires_at <= now()))
            ",
        )
        .bind(&candidate_id)
        .bind(&correlation_key)
        .bind(owner)
        .bind(lease_seconds)
        .execute(&mut *transaction)
        .await?;
        let rows = sqlx::query(
            r"
            SELECT id, automation_id, trigger_name, correlation_key,
                   payload::text AS payload_json, occurred_at::text AS occurred_at
            FROM trigger_events
            WHERE automation_id = $1 AND correlation_key = $2 AND status <> 'failed'
            ORDER BY occurred_at, id
            ",
        )
        .bind(&automation_id)
        .bind(&correlation_key)
        .fetch_all(&mut *transaction)
        .await?;
        transaction.commit().await?;
        rows.iter().map(trigger_event_from_row).collect()
    }

    /// Completes a leased group and atomically creates a run when satisfied.
    #[allow(clippy::too_many_arguments)]
    /// Atomically records an evaluation and optionally creates its workflow run.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when the transaction fails.
    pub async fn complete_trigger_group(
        &self,
        owner: &str,
        automation_id: &str,
        correlation_key: &str,
        workflow_id: &str,
        workflow_run_id: &str,
        input_json: &str,
        satisfied: bool,
    ) -> Result<bool, PostgresStoreError> {
        let mut transaction = self.pool.begin().await?;
        let mut created = false;
        if satisfied {
            let inserted = sqlx::query(
                r"
                INSERT INTO workflow_runs (
                    id, workflow_id, automation_id, input, status, current_step_position, updated_at
                )
                SELECT $3, $4, $1, $5::jsonb, 'queued', 0, now()
                WHERE NOT EXISTS (
                    SELECT 1 FROM trigger_evaluations
                    WHERE automation_id = $1 AND correlation_key = $2
                )
                ON CONFLICT (id) DO NOTHING
                ",
            )
            .bind(automation_id)
            .bind(correlation_key)
            .bind(workflow_run_id)
            .bind(workflow_id)
            .bind(input_json)
            .execute(&mut *transaction)
            .await?;
            created = inserted.rows_affected() == 1;
            if created {
                sqlx::query(
                    r"
                    INSERT INTO trigger_evaluations (automation_id, correlation_key, workflow_run_id)
                    VALUES ($1, $2, $3)
                    ",
                )
                .bind(automation_id)
                .bind(correlation_key)
                .bind(workflow_run_id)
                .execute(&mut *transaction)
                .await?;
            }
        }
        sqlx::query(
            r"
            UPDATE trigger_events
            SET status = 'evaluated', lease_owner = NULL, lease_expires_at = NULL, updated_at = now()
            WHERE automation_id = $1 AND correlation_key = $2
              AND status = 'leased' AND lease_owner = $3
            ",
        )
        .bind(automation_id)
        .bind(correlation_key)
        .bind(owner)
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(created)
    }

    /// Releases or dead-letters a failed trigger correlation group.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn retry_trigger_group(
        &self,
        owner: &str,
        automation_id: &str,
        correlation_key: &str,
        error: &str,
        retry_seconds: i64,
        max_attempts: i32,
    ) -> Result<(), PostgresStoreError> {
        sqlx::query(
            r"
            UPDATE trigger_events
            SET status = CASE WHEN attempt_count >= $6 THEN 'failed' ELSE 'pending' END,
                available_at = now() + make_interval(secs => $5),
                lease_owner = NULL, lease_expires_at = NULL, last_error = $4, updated_at = now()
            WHERE automation_id = $1 AND correlation_key = $2
              AND status = 'leased' AND lease_owner = $3
            ",
        )
        .bind(automation_id)
        .bind(correlation_key)
        .bind(owner)
        .bind(error)
        .bind(retry_seconds)
        .bind(max_attempts)
        .execute(&self.pool)
        .await?;
        Ok(())
    }
}

fn trigger_event_from_row(row: &sqlx::postgres::PgRow) -> Result<TriggerEvent, PostgresStoreError> {
    Ok(TriggerEvent {
        id: row.try_get("id")?,
        automation_id: row.try_get("automation_id")?,
        trigger_name: row.try_get("trigger_name")?,
        correlation_key: row.try_get("correlation_key")?,
        payload_json: row.try_get("payload_json")?,
        occurred_at: row.try_get("occurred_at")?,
    })
}
