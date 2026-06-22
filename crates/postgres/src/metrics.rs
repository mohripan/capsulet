use std::fmt::Write as _;

use sqlx::Row;

use crate::{PostgresStore, PostgresStoreError};

impl PostgresStore {
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

fn escape_label(value: &str) -> String {
    value
        .replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}
