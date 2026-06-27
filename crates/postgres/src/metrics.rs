use std::fmt::Write as _;

use sqlx::Row;

use crate::{AdmissionSnapshot, PostgresStore, PostgresStoreError};

impl PostgresStore {
    /// Returns queue state used by API admission backpressure.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when an aggregate query fails.
    pub async fn admission_snapshot(
        &self,
        execution_pool: &str,
    ) -> Result<AdmissionSnapshot, PostgresStoreError> {
        let queued_runs = sqlx::query_scalar::<_, i64>(
            "SELECT count(*)::bigint FROM job_runs WHERE status = 'queued'",
        )
        .fetch_one(&self.pool)
        .await?;
        let queued_runs_in_pool = sqlx::query_scalar::<_, i64>(
            "SELECT count(*)::bigint FROM job_runs WHERE status = 'queued' AND execution_pool = $1",
        )
        .bind(execution_pool)
        .fetch_one(&self.pool)
        .await?;
        let queued_workflow_runs = sqlx::query_scalar::<_, i64>(
            "SELECT count(*)::bigint FROM workflow_runs WHERE status = 'queued'",
        )
        .fetch_one(&self.pool)
        .await?;

        Ok(AdmissionSnapshot {
            queued_runs: u64::try_from(queued_runs)
                .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?,
            queued_runs_in_pool: u64::try_from(queued_runs_in_pool)
                .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?,
            queued_workflow_runs: u64::try_from(queued_workflow_runs)
                .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))?,
        })
    }

    /// Renders current control-plane gauges in Prometheus text format.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when any aggregate query fails.
    pub async fn prometheus_metrics(&self) -> Result<String, PostgresStoreError> {
        let mut output = String::from(
            "# HELP capsulet_job_runs Number of job runs by status and execution pool.\n\
             # TYPE capsulet_job_runs gauge\n",
        );
        for row in sqlx::query(
            "SELECT status, execution_pool, count(*)::bigint AS count FROM job_runs GROUP BY status, execution_pool ORDER BY status, execution_pool",
        )
        .fetch_all(&self.pool)
        .await?
        {
            let status: String = row.try_get("status")?;
            let pool: String = row.try_get("execution_pool")?;
            let count: i64 = row.try_get("count")?;
            writeln!(
                output,
                "capsulet_job_runs{{status=\"{}\",pool=\"{}\"}} {count}",
                escape_label(&status),
                escape_label(&pool)
            )
            .expect("writing to a String cannot fail");
        }
        output.push_str(
            "# HELP capsulet_workflow_runs Number of workflow runs by status.\n\
             # TYPE capsulet_workflow_runs gauge\n",
        );
        append_status_counts(
            &self.pool,
            &mut output,
            "workflow_runs",
            "capsulet_workflow_runs",
        )
        .await?;
        output.push_str(
            "# HELP capsulet_trigger_events Number of durable trigger events by status.\n\
             # TYPE capsulet_trigger_events gauge\n",
        );
        append_status_counts(
            &self.pool,
            &mut output,
            "trigger_events",
            "capsulet_trigger_events",
        )
        .await?;
        append_job_queue_slo_metrics(&self.pool, &mut output).await?;
        append_workflow_slo_metrics(&self.pool, &mut output).await?;
        let trigger_failures = sqlx::query_scalar::<_, i64>(
            "SELECT COALESCE(sum(consecutive_failures), 0)::bigint FROM trigger_runtime_status",
        )
        .fetch_one(&self.pool)
        .await?;
        output.push_str(
            "# HELP capsulet_trigger_runtime_failures Consecutive cron and SQL trigger failures.\n\
             # TYPE capsulet_trigger_runtime_failures gauge\n",
        );
        writeln!(
            output,
            "capsulet_trigger_runtime_failures {trigger_failures}"
        )
        .expect("writing to a String cannot fail");
        Ok(output)
    }
}

async fn append_status_counts(
    pool: &sqlx::PgPool,
    output: &mut String,
    table: &str,
    metric: &str,
) -> Result<(), PostgresStoreError> {
    let query = format!(
        "SELECT status, count(*)::bigint AS count FROM {table} GROUP BY status ORDER BY status"
    );
    for row in sqlx::query(&query).fetch_all(pool).await? {
        let status: String = row.try_get("status")?;
        let count: i64 = row.try_get("count")?;
        writeln!(
            output,
            "{metric}{{status=\"{}\"}} {count}",
            escape_label(&status)
        )
        .expect("writing to a String cannot fail");
    }
    Ok(())
}

async fn append_job_queue_slo_metrics(
    pool: &sqlx::PgPool,
    output: &mut String,
) -> Result<(), PostgresStoreError> {
    append_job_queue_age_metrics(pool, output).await?;
    append_job_lease_age_metrics(pool, output).await?;
    append_execution_pool_saturation_metrics(pool, output).await?;
    append_retry_exhaustion_metrics(pool, output).await?;
    Ok(())
}

async fn append_job_queue_age_metrics(
    pool: &sqlx::PgPool,
    output: &mut String,
) -> Result<(), PostgresStoreError> {
    output.push_str(
        "# HELP capsulet_job_queue_depth Queued job runs by execution pool.\n\
         # TYPE capsulet_job_queue_depth gauge\n\
         # HELP capsulet_job_queue_oldest_age_seconds Age of the oldest queued job run by execution pool.\n\
         # TYPE capsulet_job_queue_oldest_age_seconds gauge\n",
    );
    for row in sqlx::query(
        r"
        SELECT execution_pool,
               count(*)::bigint AS queued,
               EXTRACT(EPOCH FROM now() - min(created_at))::double precision AS oldest_age_seconds
        FROM job_runs
        WHERE status = 'queued'
        GROUP BY execution_pool
        ORDER BY execution_pool
        ",
    )
    .fetch_all(pool)
    .await?
    {
        let execution_pool: String = row.try_get("execution_pool")?;
        let queued: i64 = row.try_get("queued")?;
        let oldest_age_seconds: f64 = row.try_get("oldest_age_seconds")?;
        writeln!(
            output,
            "capsulet_job_queue_depth{{pool=\"{}\"}} {queued}",
            escape_label(&execution_pool)
        )
        .expect("writing to a String cannot fail");
        writeln!(
            output,
            "capsulet_job_queue_oldest_age_seconds{{pool=\"{}\"}} {oldest_age_seconds}",
            escape_label(&execution_pool)
        )
        .expect("writing to a String cannot fail");
    }

    Ok(())
}

async fn append_job_lease_age_metrics(
    pool: &sqlx::PgPool,
    output: &mut String,
) -> Result<(), PostgresStoreError> {
    output.push_str(
        "# HELP capsulet_job_lease_age_seconds Oldest active heartbeat age by execution pool and run status.\n\
         # TYPE capsulet_job_lease_age_seconds gauge\n",
    );
    for row in sqlx::query(
        r"
        SELECT execution_pool,
               status,
               EXTRACT(EPOCH FROM now() - min(COALESCE(heartbeat_at, updated_at)))::double precision AS lease_age_seconds
        FROM job_runs
        WHERE status IN ('leased', 'running')
        GROUP BY execution_pool, status
        ORDER BY execution_pool, status
        ",
    )
    .fetch_all(pool)
    .await?
    {
        let execution_pool: String = row.try_get("execution_pool")?;
        let status: String = row.try_get("status")?;
        let lease_age_seconds: f64 = row.try_get("lease_age_seconds")?;
        writeln!(
            output,
            "capsulet_job_lease_age_seconds{{pool=\"{}\",status=\"{}\"}} {lease_age_seconds}",
            escape_label(&execution_pool),
            escape_label(&status)
        )
        .expect("writing to a String cannot fail");
    }

    Ok(())
}

async fn append_execution_pool_saturation_metrics(
    pool: &sqlx::PgPool,
    output: &mut String,
) -> Result<(), PostgresStoreError> {
    output.push_str(
        "# HELP capsulet_execution_pool_saturation Active share of active plus queued demand by execution pool.\n\
         # TYPE capsulet_execution_pool_saturation gauge\n",
    );
    for row in sqlx::query(
        r"
        SELECT execution_pool,
               (
                   count(*) FILTER (WHERE status IN ('leased', 'running'))::double precision
                   / NULLIF(count(*)::double precision, 0.0)
               ) AS saturation
        FROM job_runs
        WHERE status IN ('queued', 'leased', 'running')
        GROUP BY execution_pool
        ORDER BY execution_pool
        ",
    )
    .fetch_all(pool)
    .await?
    {
        let execution_pool: String = row.try_get("execution_pool")?;
        let saturation: f64 = row.try_get("saturation")?;
        writeln!(
            output,
            "capsulet_execution_pool_saturation{{pool=\"{}\"}} {saturation}",
            escape_label(&execution_pool)
        )
        .expect("writing to a String cannot fail");
    }

    Ok(())
}

async fn append_retry_exhaustion_metrics(
    pool: &sqlx::PgPool,
    output: &mut String,
) -> Result<(), PostgresStoreError> {
    let retry_exhausted = sqlx::query_scalar::<_, i64>(
        r"
        SELECT count(*)::bigint
        FROM job_runs jr
        JOIN job_definitions jd ON jd.id = jr.job_definition_id
        WHERE jr.status IN ('failed', 'timed_out')
          AND jr.attempt_count >= jd.retry_max_attempts
        ",
    )
    .fetch_one(pool)
    .await?;
    output.push_str(
        "# HELP capsulet_job_retry_exhausted_runs Terminal failed or timed-out job runs with no retry attempts remaining.\n\
         # TYPE capsulet_job_retry_exhausted_runs gauge\n",
    );
    writeln!(
        output,
        "capsulet_job_retry_exhausted_runs {retry_exhausted}"
    )
    .expect("writing to a String cannot fail");

    Ok(())
}

async fn append_workflow_slo_metrics(
    pool: &sqlx::PgPool,
    output: &mut String,
) -> Result<(), PostgresStoreError> {
    let scheduler_lag = sqlx::query_scalar::<_, Option<f64>>(
        "SELECT EXTRACT(EPOCH FROM now() - min(created_at))::double precision FROM workflow_runs WHERE status = 'queued'",
    )
    .fetch_one(pool)
    .await?
    .unwrap_or(0.0);
    output.push_str(
        "# HELP capsulet_scheduler_lag_seconds Age of the oldest queued workflow run.\n\
         # TYPE capsulet_scheduler_lag_seconds gauge\n",
    );
    writeln!(output, "capsulet_scheduler_lag_seconds {scheduler_lag}")
        .expect("writing to a String cannot fail");

    output.push_str(
        "# HELP capsulet_workflow_critical_path_latency_seconds Longest workflow run age or completed duration by status.\n\
         # TYPE capsulet_workflow_critical_path_latency_seconds gauge\n",
    );
    for row in sqlx::query(
        r"
        SELECT status,
               max(EXTRACT(EPOCH FROM COALESCE(finished_at, now()) - created_at))::double precision AS latency_seconds
        FROM workflow_runs
        GROUP BY status
        ORDER BY status
        ",
    )
    .fetch_all(pool)
    .await?
    {
        let status: String = row.try_get("status")?;
        let latency_seconds: f64 = row.try_get("latency_seconds")?;
        writeln!(
            output,
            "capsulet_workflow_critical_path_latency_seconds{{status=\"{}\"}} {latency_seconds}",
            escape_label(&status)
        )
        .expect("writing to a String cannot fail");
    }

    let stuck_workflows = sqlx::query_scalar::<_, i64>(
        r"
        SELECT count(*)::bigint
        FROM workflow_runs
        WHERE status IN ('queued', 'running')
          AND updated_at < now() - interval '15 minutes'
        ",
    )
    .fetch_one(pool)
    .await?;
    output.push_str(
        "# HELP capsulet_stuck_workflow_runs Queued or running workflow runs with no state update for fifteen minutes.\n\
         # TYPE capsulet_stuck_workflow_runs gauge\n",
    );
    writeln!(output, "capsulet_stuck_workflow_runs {stuck_workflows}")
        .expect("writing to a String cannot fail");

    Ok(())
}

fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
