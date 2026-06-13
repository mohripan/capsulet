use std::time::{SystemTime, UNIX_EPOCH};

use capsulet_core::{
    ArtifactId, ArtifactObjectKind, Automation, AutomationId, AutomationStatus, AutomationTrigger,
    AutomationTriggerKind, CustomTriggerPlugin, ExecutionPoolName, JobArtifact, JobAttemptId,
    JobDefinition, JobDefinitionId, JobRun, JobRunId, JobRunLog, JobRunStatus, RetryPolicy,
    TriggerKind, TriggerName, WorkflowId, WorkflowRun, WorkflowRunId, WorkflowRunStatus,
    WorkflowStatus, WorkflowStep, WorkflowStepId, WorkflowStepRun, WorkflowStepRunId,
};
use sqlx::Row;

use crate::PostgresStoreError;
pub(crate) fn row_to_job_run(row: &sqlx::postgres::PgRow) -> Result<JobRun, PostgresStoreError> {
    let id: String = row.try_get("id")?;
    let job_definition_id: String = row.try_get("job_definition_id")?;
    let status: String = row.try_get("status")?;
    let execution_pool: String = row.try_get("execution_pool")?;
    let attempt_count: i32 = row.try_get("attempt_count")?;
    let input_json: String = row.try_get("input")?;

    let mut run = JobRun::new(
        JobRunId::new(id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        JobDefinitionId::new(job_definition_id)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        ExecutionPoolName::new(execution_pool)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
    );
    run.input_json = input_json;
    run.status = parse_status(&status)?;
    run.attempt_count = u32::try_from(attempt_count)
        .map_err(|_| PostgresStoreError::InvalidPersistedValue("negative attempt count".into()))?;

    Ok(run)
}

pub(crate) fn generated_store_id(prefix: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_nanos());
    format!("{prefix}_{nanos}")
}

pub(crate) fn row_to_job_definition(
    row: &sqlx::postgres::PgRow,
) -> Result<JobDefinition, PostgresStoreError> {
    let id: String = row.try_get("id")?;
    let name: String = row.try_get("name")?;
    let runtime_image: String = row.try_get("runtime_image")?;
    let command: Vec<String> = row.try_get("command")?;
    let bundle_object_key: String = row.try_get("bundle_object_key")?;
    let input_schema: String = row.try_get("input_schema")?;
    let retry_max_attempts: i32 = row.try_get("retry_max_attempts")?;
    let retry_delay_seconds: i32 = row.try_get("retry_delay_seconds")?;

    JobDefinition::new(
        JobDefinitionId::new(id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        name,
        runtime_image,
        command,
        bundle_object_key,
        input_schema,
        RetryPolicy {
            max_attempts: u32::try_from(retry_max_attempts).map_err(|_| {
                PostgresStoreError::InvalidPersistedValue("negative retry max attempts".into())
            })?,
            delay_seconds: u64::try_from(retry_delay_seconds).map_err(|_| {
                PostgresStoreError::InvalidPersistedValue("negative retry delay".into())
            })?,
        },
    )
    .map_err(PostgresStoreError::InvalidPersistedValue)
}

pub(crate) fn row_to_workflow_step(
    row: &sqlx::postgres::PgRow,
) -> Result<WorkflowStep, PostgresStoreError> {
    let id: String = row.try_get("id")?;
    let workflow_id: String = row.try_get("workflow_id")?;
    let job_definition_id: String = row.try_get("job_definition_id")?;
    let execution_pool: String = row.try_get("execution_pool")?;

    Ok(WorkflowStep {
        id: WorkflowStepId::new(id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        workflow_id: WorkflowId::new(workflow_id)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        position: row.try_get("position")?,
        name: row.try_get("name")?,
        job_definition_id: JobDefinitionId::new(job_definition_id)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        execution_pool: ExecutionPoolName::new(execution_pool)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
    })
}

pub(crate) fn row_to_automation(
    row: &sqlx::postgres::PgRow,
) -> Result<Automation, PostgresStoreError> {
    let id: String = row.try_get("id")?;
    let workflow_id: String = row.try_get("workflow_id")?;
    let status: String = row.try_get("status")?;
    let trigger_kind: String = row.try_get("trigger_kind")?;
    let interval_seconds: Option<i32> = row.try_get("interval_seconds")?;
    let job_input_json: String = row.try_get("job_input")?;

    Ok(Automation {
        id: AutomationId::new(id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        workflow_id: WorkflowId::new(workflow_id)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        job_input_json,
        status: parse_automation_status(&status)?,
        trigger_kind: parse_automation_trigger_kind(&trigger_kind)?,
        interval_seconds: interval_seconds.map(i64::from),
    })
}

pub(crate) fn row_to_automation_trigger(
    row: &sqlx::postgres::PgRow,
) -> Result<AutomationTrigger, PostgresStoreError> {
    let automation_id: String = row.try_get("automation_id")?;
    let name: String = row.try_get("name")?;
    let kind: String = row.try_get("kind")?;

    Ok(AutomationTrigger {
        automation_id: AutomationId::new(automation_id)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        name: TriggerName::new(name).map_err(PostgresStoreError::InvalidPersistedValue)?,
        kind: parse_trigger_kind(&kind)?,
        config_json: row.try_get("config")?,
        plugin_id: row.try_get("plugin_id")?,
        enabled: row.try_get("enabled")?,
    })
}

pub(crate) fn row_to_custom_trigger_plugin(
    row: &sqlx::postgres::PgRow,
) -> Result<CustomTriggerPlugin, PostgresStoreError> {
    Ok(CustomTriggerPlugin {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        runtime_image: row.try_get("runtime_image")?,
        command: row.try_get("command")?,
        config_schema_json: row.try_get("config_schema")?,
    })
}

pub(crate) fn row_to_workflow_run(
    row: &sqlx::postgres::PgRow,
) -> Result<WorkflowRun, PostgresStoreError> {
    let id: String = row.try_get("id")?;
    let workflow_id: String = row.try_get("workflow_id")?;
    let automation_id: Option<String> = row.try_get("automation_id")?;
    let status: String = row.try_get("status")?;
    let input_json: String = row.try_get("input")?;

    Ok(WorkflowRun {
        id: WorkflowRunId::new(id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        workflow_id: WorkflowId::new(workflow_id)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        automation_id: automation_id
            .map(AutomationId::new)
            .transpose()
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        input_json,
        status: parse_workflow_run_status(&status)?,
        current_step_position: row.try_get("current_step_position")?,
    })
}

pub(crate) fn row_to_workflow_step_run(
    row: &sqlx::postgres::PgRow,
) -> Result<WorkflowStepRun, PostgresStoreError> {
    let id: String = row.try_get("id")?;
    let workflow_run_id: String = row.try_get("workflow_run_id")?;
    let workflow_step_id: String = row.try_get("workflow_step_id")?;
    let job_run_id: String = row.try_get("job_run_id")?;
    let status: String = row.try_get("status")?;

    Ok(WorkflowStepRun {
        id: WorkflowStepRunId::new(id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        workflow_run_id: WorkflowRunId::new(workflow_run_id)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        workflow_step_id: WorkflowStepId::new(workflow_step_id)
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        job_run_id: JobRunId::new(job_run_id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        position: row.try_get("position")?,
        status: parse_workflow_run_status(&status)?,
    })
}

pub(crate) fn row_to_job_run_log(
    row: &sqlx::postgres::PgRow,
) -> Result<JobRunLog, PostgresStoreError> {
    let run_id: String = row.try_get("job_run_id")?;
    let log_text: String = row.try_get("log_text")?;

    JobRunLog::new(
        JobRunId::new(run_id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        log_text,
    )
    .map_err(PostgresStoreError::InvalidPersistedValue)
}

pub(crate) fn row_to_job_artifact(
    row: &sqlx::postgres::PgRow,
) -> Result<JobArtifact, PostgresStoreError> {
    let id: String = row.try_get("id")?;
    let job_run_id: String = row.try_get("job_run_id")?;
    let job_attempt_id: Option<String> = row.try_get("job_attempt_id")?;
    let name: String = row.try_get("name")?;
    let object_key: String = row.try_get("object_key")?;
    let content_type: String = row.try_get("content_type")?;
    let size_bytes: i64 = row.try_get("size_bytes")?;
    let checksum_sha256: Option<String> = row.try_get("checksum_sha256")?;
    let kind: String = row.try_get("kind")?;

    JobArtifact::new(
        ArtifactId::new(id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        JobRunId::new(job_run_id).map_err(PostgresStoreError::InvalidPersistedValue)?,
        job_attempt_id
            .map(JobAttemptId::new)
            .transpose()
            .map_err(PostgresStoreError::InvalidPersistedValue)?,
        name,
        object_key,
        content_type,
        u64::try_from(size_bytes).map_err(|_| {
            PostgresStoreError::InvalidPersistedValue("negative artifact size".into())
        })?,
        checksum_sha256,
        parse_artifact_kind(&kind)?,
    )
    .map_err(PostgresStoreError::InvalidPersistedValue)
}

pub(crate) fn parse_status(status: &str) -> Result<JobRunStatus, PostgresStoreError> {
    match status {
        "queued" => Ok(JobRunStatus::Queued),
        "leased" => Ok(JobRunStatus::Leased),
        "running" => Ok(JobRunStatus::Running),
        "succeeded" => Ok(JobRunStatus::Succeeded),
        "failed" => Ok(JobRunStatus::Failed),
        "cancelled" => Ok(JobRunStatus::Cancelled),
        "timed_out" => Ok(JobRunStatus::TimedOut),
        "retry_scheduled" => Ok(JobRunStatus::RetryScheduled),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown job run status {value}"
        ))),
    }
}

pub(crate) fn parse_workflow_status(status: &str) -> Result<WorkflowStatus, PostgresStoreError> {
    match status {
        "draft" => Ok(WorkflowStatus::Draft),
        "enabled" => Ok(WorkflowStatus::Enabled),
        "disabled" => Ok(WorkflowStatus::Disabled),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown workflow status {value}"
        ))),
    }
}

pub(crate) fn parse_workflow_run_status(
    status: &str,
) -> Result<WorkflowRunStatus, PostgresStoreError> {
    match status {
        "queued" => Ok(WorkflowRunStatus::Queued),
        "running" => Ok(WorkflowRunStatus::Running),
        "succeeded" => Ok(WorkflowRunStatus::Succeeded),
        "failed" => Ok(WorkflowRunStatus::Failed),
        "cancelled" => Ok(WorkflowRunStatus::Cancelled),
        "timed_out" => Ok(WorkflowRunStatus::TimedOut),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown workflow run status {value}"
        ))),
    }
}

pub(crate) fn parse_automation_status(
    status: &str,
) -> Result<AutomationStatus, PostgresStoreError> {
    match status {
        "enabled" => Ok(AutomationStatus::Enabled),
        "disabled" => Ok(AutomationStatus::Disabled),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown automation status {value}"
        ))),
    }
}

pub(crate) fn parse_automation_trigger_kind(
    trigger_kind: &str,
) -> Result<AutomationTriggerKind, PostgresStoreError> {
    match trigger_kind {
        "manual" => Ok(AutomationTriggerKind::Manual),
        "interval" => Ok(AutomationTriggerKind::Interval),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown automation trigger kind {value}"
        ))),
    }
}

pub(crate) fn parse_trigger_kind(trigger_kind: &str) -> Result<TriggerKind, PostgresStoreError> {
    match trigger_kind {
        "manual" => Ok(TriggerKind::Manual),
        "schedule" => Ok(TriggerKind::Schedule),
        "sql" => Ok(TriggerKind::Sql),
        "custom" => Ok(TriggerKind::Custom),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown trigger kind {value}"
        ))),
    }
}

pub(crate) fn parse_artifact_kind(kind: &str) -> Result<ArtifactObjectKind, PostgresStoreError> {
    match kind {
        "bundle" => Ok(ArtifactObjectKind::Bundle),
        "log" => Ok(ArtifactObjectKind::Log),
        "artifact" => Ok(ArtifactObjectKind::Artifact),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown artifact kind {value}"
        ))),
    }
}
