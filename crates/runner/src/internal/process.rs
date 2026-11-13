//! A trusted local process runner.

use std::process::Stdio;

use async_trait::async_trait;
use thiserror::Error;
use tokio::{process::Command, time::sleep};

use super::artifacts::{collect_local_artifacts, materialize_local_inputs, reset_artifact_dir};
use super::contract::{CancellationCheck, RunExecution, RunOutcome, RunReport, Runner};
use super::kubernetes::POLL_INTERVAL;
use super::pools::validate_execution_policy;

/// Local process runner for Docker Compose smoke tests.
#[derive(Debug, Clone, Copy)]
pub struct ProcessRunner;

#[async_trait]
impl Runner for ProcessRunner {
    type Error = ProcessRunnerError;

    async fn execute<C>(
        &self,
        execution: &RunExecution,
        cancellation: &C,
    ) -> Result<RunReport, Self::Error>
    where
        C: CancellationCheck + Sync,
    {
        if cancellation
            .is_cancelled(execution.run.id())
            .await
            .map_err(|error| ProcessRunnerError::CancellationCheck(error.to_string()))?
        {
            return Ok(RunReport::cancelled(None));
        }

        validate_execution_policy(execution).map_err(ProcessRunnerError::Policy)?;
        reset_artifact_dir()?;
        materialize_local_inputs(&execution.input_artifacts)?;
        if !execution.definition.python_dependencies().is_empty() {
            let install_output =
                install_python_dependencies(execution.definition.python_dependencies()).await?;
            if !install_output.status.success() {
                return Ok(RunReport::failed(Some(combined_output_logs(
                    &install_output,
                ))));
            }
        }
        let Some((program, args)) = execution.definition.command().split_first() else {
            return Err(ProcessRunnerError::EmptyCommand);
        };
        let mut child = Command::new(local_program(program))
            .args(args)
            .env("CAPSULET_INPUT_JSON", execution.run.input_json())
            .env("PYTHONPATH", process_python_path())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()?;
        loop {
            if cancellation
                .is_cancelled(execution.run.id())
                .await
                .map_err(|error| ProcessRunnerError::CancellationCheck(error.to_string()))?
            {
                child.start_kill()?;
                let output = child.wait_with_output().await?;
                let logs = combined_output_logs(&output);
                return Ok(RunReport::cancelled(Some(logs)));
            }
            if child.try_wait()?.is_some() {
                break;
            }
            sleep(POLL_INTERVAL).await;
        }
        let output = child.wait_with_output().await?;
        let logs = combined_output_logs(&output);
        let artifacts = collect_local_artifacts()?;
        if output.status.success() {
            Ok(RunReport::succeeded_with_artifacts(Some(logs), artifacts))
        } else {
            Ok(RunReport {
                outcome: RunOutcome::Failed,
                logs: Some(logs),
                artifacts,
            })
        }
    }
}

pub(super) fn process_python_path() -> String {
    match std::env::var("PYTHONPATH") {
        Ok(existing) if !existing.is_empty() => format!("/capsulet/python-site:{existing}"),
        _ => "/capsulet/python-site".to_string(),
    }
}

pub(super) fn combined_output_logs(output: &std::process::Output) -> String {
    let mut logs = String::new();
    logs.push_str(&String::from_utf8_lossy(&output.stdout));
    logs.push_str(&String::from_utf8_lossy(&output.stderr));
    logs
}

pub(super) fn local_program(program: &str) -> &str {
    if program == "python" {
        "python3"
    } else {
        program
    }
}

pub(super) async fn install_python_dependencies(
    dependencies: &[String],
) -> Result<std::process::Output, std::io::Error> {
    Command::new(local_program("python"))
        .arg("-m")
        .arg("pip")
        .arg("install")
        .arg("--disable-pip-version-check")
        .arg("--no-input")
        .arg("--break-system-packages")
        .arg("--target")
        .arg("/capsulet/python-site")
        .args(dependencies)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .output()
        .await
}

#[derive(Debug, Error)]
pub enum ProcessRunnerError {
    #[error("process command cannot be empty")]
    EmptyCommand,
    #[error("process execution failed: {0}")]
    Io(#[from] std::io::Error),
    #[error("cancellation check failed: {0}")]
    CancellationCheck(String),
    #[error("execution policy rejected run: {0}")]
    Policy(String),
}
