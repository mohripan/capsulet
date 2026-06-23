use std::{fmt::Write as _, path::PathBuf, process::ExitCode};

use clap::{CommandFactory, Parser, Subcommand};
use clap_complete::{Shell, generate};
use reqwest::{
    StatusCode, Url,
    header::{AUTHORIZATION, HeaderMap, HeaderValue},
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use thiserror::Error;

const DEFAULT_API_URL: &str = "http://127.0.0.1:8080";

#[derive(Debug, Parser)]
#[command(
    name = "capsulet",
    about = "Command-line client for Capsulet.",
    version
)]
struct Cli {
    #[arg(
        long,
        env = "CAPSULET_API_URL",
        default_value = DEFAULT_API_URL,
        global = true,
        help = "Base URL for the Capsulet API"
    )]
    api_url: Url,
    #[arg(
        long,
        env = "CAPSULET_API_TOKEN",
        global = true,
        hide_env_values = true,
        help = "Bearer token for the Capsulet API"
    )]
    api_token: Option<String>,
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Submit a manual job run")]
    Submit {
        #[arg(help = "Job definition ID to run")]
        job_definition_id: String,
        #[arg(
            long,
            short = 'p',
            visible_alias = "host-group",
            help = "Host group / Kubernetes execution pool"
        )]
        pool: String,
        #[arg(long, help = "Optional caller-provided run ID")]
        run_id: Option<String>,
    },
    #[command(about = "Submit a single-file Python script")]
    SubmitScript {
        #[arg(help = "Path to a Python script")]
        path: PathBuf,
        #[arg(
            long,
            short = 'p',
            visible_alias = "host-group",
            help = "Host group / Kubernetes execution pool"
        )]
        pool: String,
        #[arg(long, help = "Optional caller-provided run ID")]
        run_id: Option<String>,
    },
    #[command(about = "List job runs")]
    Runs {
        #[arg(long, default_value_t = 50, help = "Maximum runs to return")]
        limit: u16,
    },
    #[command(about = "Show status for one job run")]
    Status {
        #[arg(help = "Job run ID")]
        id: String,
    },
    #[command(about = "Print captured logs for one job run")]
    Logs {
        #[arg(help = "Job run ID")]
        id: String,
    },
    #[command(about = "Cancel a queued or running job run")]
    Cancel {
        #[arg(help = "Job run ID")]
        id: String,
    },
    #[command(subcommand, about = "List or download run artifacts")]
    Artifacts(ArtifactsCommand),
    #[command(subcommand, about = "Inspect a job run")]
    Run(RunCommand),
    #[command(about = "Generate shell completion scripts")]
    Completions {
        #[arg(value_enum, help = "Target shell")]
        shell: Shell,
    },
}

#[derive(Debug, Subcommand)]
enum ArtifactsCommand {
    #[command(about = "List artifacts for one job run")]
    List {
        #[arg(help = "Job run ID")]
        id: String,
    },
    #[command(about = "Download one artifact")]
    Download {
        #[arg(help = "Job run ID")]
        id: String,
        #[arg(help = "Artifact ID")]
        artifact_id: String,
        #[arg(long, short = 'o', help = "Output file path")]
        output: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum RunCommand {
    #[command(about = "Fetch one job run")]
    Get {
        #[arg(help = "Job run ID")]
        id: String,
    },
}

/// Runs the Capsulet CLI process.
pub async fn run() -> ExitCode {
    match execute(Cli::parse()).await {
        Ok(output) => {
            print!("{output}");
            ExitCode::SUCCESS
        }
        Err(error) => {
            eprintln!("error: {error}");
            ExitCode::FAILURE
        }
    }
}

async fn execute(cli: Cli) -> Result<String, CliError> {
    match cli.command {
        Command::Completions { shell } => {
            let mut command = Cli::command();
            let mut output = Vec::new();
            generate(shell, &mut command, "capsulet", &mut output);
            String::from_utf8(output).map_err(CliError::from)
        }
        Command::Submit {
            job_definition_id,
            pool,
            run_id,
        } => {
            let api = ApiClient::new(cli.api_url, cli.api_token.as_deref())?;
            let request = CreateRunRequest {
                job_definition_id,
                execution_pool: pool,
                run_id,
                python_script: None,
            };
            let run = api.create_run(&request).await?;
            Ok(format_run_detail(&run))
        }
        Command::SubmitScript { path, pool, run_id } => {
            let api = ApiClient::new(cli.api_url, cli.api_token.as_deref())?;
            let script = std::fs::read_to_string(&path)?;
            let request = CreateRunRequest {
                job_definition_id: "script".to_string(),
                execution_pool: pool,
                run_id,
                python_script: Some(script),
            };
            let run = api.create_run(&request).await?;
            Ok(format_run_detail(&run))
        }
        Command::Runs { limit } => {
            let api = ApiClient::new(cli.api_url, cli.api_token.as_deref())?;
            let runs = api.list_runs(limit).await?;
            Ok(format_runs_table(&runs.runs))
        }
        Command::Status { id } => {
            let api = ApiClient::new(cli.api_url, cli.api_token.as_deref())?;
            let run = api.get_run(&id).await?;
            Ok(format_run_status(&run))
        }
        Command::Logs { id } => {
            let api = ApiClient::new(cli.api_url, cli.api_token.as_deref())?;
            let logs = api.get_run_logs(&id).await?;
            Ok(logs.logs)
        }
        Command::Cancel { id } => {
            let api = ApiClient::new(cli.api_url, cli.api_token.as_deref())?;
            let run = api.cancel_run(&id).await?;
            Ok(format_run_status(&run))
        }
        Command::Artifacts(ArtifactsCommand::List { id }) => {
            let api = ApiClient::new(cli.api_url, cli.api_token.as_deref())?;
            let artifacts = api.list_artifacts(&id).await?;
            Ok(format_artifacts_table(&artifacts.artifacts))
        }
        Command::Artifacts(ArtifactsCommand::Download {
            id,
            artifact_id,
            output,
        }) => {
            let api = ApiClient::new(cli.api_url, cli.api_token.as_deref())?;
            let bytes = api.download_artifact(&id, &artifact_id).await?;
            std::fs::write(&output, bytes)?;
            Ok(format!(
                "downloaded {artifact_id} to {}\n",
                output.display()
            ))
        }
        Command::Run(RunCommand::Get { id }) => {
            let api = ApiClient::new(cli.api_url, cli.api_token.as_deref())?;
            let run = api.get_run(&id).await?;
            Ok(format_run_detail(&run))
        }
    }
}

#[derive(Debug, Clone)]
struct ApiClient {
    base_url: Url,
    client: reqwest::Client,
}

impl ApiClient {
    fn new(base_url: Url, token: Option<&str>) -> Result<Self, CliError> {
        let mut headers = HeaderMap::new();
        if let Some(token) = token.filter(|token| !token.is_empty()) {
            let value = HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| CliError::InvalidApiToken)?;
            headers.insert(AUTHORIZATION, value);
        }
        Ok(Self {
            base_url,
            client: reqwest::Client::builder()
                .default_headers(headers)
                .build()?,
        })
    }

    async fn create_run(&self, request: &CreateRunRequest) -> Result<JobRunResponse, CliError> {
        let response = self
            .client
            .post(self.url(&["v1", "jobs", "runs"])?)
            .json(request)
            .send()
            .await?;

        parse_response(response).await
    }

    async fn list_runs(&self, limit: u16) -> Result<ListRunsResponse, CliError> {
        let response = self
            .client
            .get(self.url(&["v1", "jobs", "runs"])?)
            .query(&[("limit", limit)])
            .send()
            .await?;

        parse_response(response).await
    }

    async fn get_run(&self, id: &str) -> Result<JobRunResponse, CliError> {
        let response = self
            .client
            .get(self.url(&["v1", "jobs", "runs", id])?)
            .send()
            .await?;

        parse_response(response).await
    }

    async fn get_run_logs(&self, id: &str) -> Result<JobRunLogsResponse, CliError> {
        let response = self
            .client
            .get(self.url(&["v1", "jobs", "runs", id, "logs"])?)
            .send()
            .await?;

        parse_response(response).await
    }

    async fn cancel_run(&self, id: &str) -> Result<JobRunResponse, CliError> {
        let response = self
            .client
            .post(self.url(&["v1", "jobs", "runs", id, "cancel"])?)
            .send()
            .await?;

        parse_response(response).await
    }

    async fn list_artifacts(&self, id: &str) -> Result<ListArtifactsResponse, CliError> {
        let response = self
            .client
            .get(self.url(&["v1", "jobs", "runs", id, "artifacts"])?)
            .send()
            .await?;

        parse_response(response).await
    }

    async fn download_artifact(&self, id: &str, artifact_id: &str) -> Result<Vec<u8>, CliError> {
        let response = self
            .client
            .get(self.url(&["v1", "jobs", "runs", id, "artifacts", artifact_id])?)
            .send()
            .await?;

        let status = response.status();
        let bytes = response.bytes().await?;
        if status.is_success() {
            return Ok(bytes.to_vec());
        }

        let body = String::from_utf8_lossy(&bytes).into_owned();
        let error =
            serde_json::from_str::<ApiErrorResponse>(&body).unwrap_or_else(|_| ApiErrorResponse {
                code: "http_error".to_string(),
                message: body,
            });
        Err(CliError::Api {
            status,
            code: error.code,
            message: error.message,
        })
    }

    fn url(&self, segments: &[&str]) -> Result<Url, CliError> {
        let mut url = self.base_url.clone();
        {
            let mut path = url
                .path_segments_mut()
                .map_err(|()| CliError::InvalidBaseUrl(self.base_url.to_string()))?;
            path.clear().extend(segments);
        }
        Ok(url)
    }
}

async fn parse_response<T>(response: reqwest::Response) -> Result<T, CliError>
where
    T: DeserializeOwned,
{
    let status = response.status();
    let body = response.text().await?;

    if status.is_success() {
        return serde_json::from_str(&body).map_err(CliError::Json);
    }

    let error =
        serde_json::from_str::<ApiErrorResponse>(&body).unwrap_or_else(|_| ApiErrorResponse {
            code: "http_error".to_string(),
            message: body,
        });
    Err(CliError::Api {
        status,
        code: error.code,
        message: error.message,
    })
}

fn format_run_detail(run: &JobRunResponse) -> String {
    format!(
        "id: {}\njob_definition_id: {}\nstatus: {}\nexecution_pool: {}\nattempt_count: {}\n",
        run.id, run.job_definition_id, run.status, run.execution_pool, run.attempt_count
    )
}

fn format_run_status(run: &JobRunResponse) -> String {
    format!(
        "{}  {}  attempts={}\n",
        run.id, run.status, run.attempt_count
    )
}

fn format_runs_table(runs: &[JobRunResponse]) -> String {
    let mut id_width = "ID".len();
    let mut job_width = "JOB".len();
    let mut status_width = "STATUS".len();
    let mut pool_width = "POOL".len();

    for run in runs {
        id_width = id_width.max(run.id.len());
        job_width = job_width.max(run.job_definition_id.len());
        status_width = status_width.max(run.status.len());
        pool_width = pool_width.max(run.execution_pool.len());
    }

    let mut output = format!(
        "{:<id_width$}  {:<job_width$}  {:<status_width$}  {:<pool_width$}  ATTEMPTS\n",
        "ID", "JOB", "STATUS", "POOL"
    );
    for run in runs {
        writeln!(
            output,
            "{:<id_width$}  {:<job_width$}  {:<status_width$}  {:<pool_width$}  {}",
            run.id, run.job_definition_id, run.status, run.execution_pool, run.attempt_count
        )
        .expect("writing to String cannot fail");
    }
    output
}

fn format_artifacts_table(artifacts: &[ArtifactResponse]) -> String {
    let mut id_width = "ID".len();
    let mut name_width = "NAME".len();
    let mut kind_width = "KIND".len();

    for artifact in artifacts {
        id_width = id_width.max(artifact.id.len());
        name_width = name_width.max(artifact.name.len());
        kind_width = kind_width.max(artifact.kind.len());
    }

    let mut output = format!(
        "{:<id_width$}  {:<name_width$}  {:<kind_width$}  SIZE  CONTENT_TYPE\n",
        "ID", "NAME", "KIND"
    );
    for artifact in artifacts {
        writeln!(
            output,
            "{:<id_width$}  {:<name_width$}  {:<kind_width$}  {}  {}",
            artifact.id, artifact.name, artifact.kind, artifact.size_bytes, artifact.content_type
        )
        .expect("writing to String cannot fail");
    }
    output
}

#[derive(Debug, Serialize)]
struct CreateRunRequest {
    job_definition_id: String,
    execution_pool: String,
    run_id: Option<String>,
    python_script: Option<String>,
}

#[derive(Debug, Deserialize)]
struct ListRunsResponse {
    runs: Vec<JobRunResponse>,
}

#[derive(Debug, Deserialize)]
struct JobRunResponse {
    id: String,
    job_definition_id: String,
    status: String,
    execution_pool: String,
    attempt_count: u32,
}

#[derive(Debug, Deserialize)]
struct JobRunLogsResponse {
    logs: String,
}

#[derive(Debug, Deserialize)]
struct ListArtifactsResponse {
    artifacts: Vec<ArtifactResponse>,
}

#[derive(Debug, Deserialize)]
struct ArtifactResponse {
    id: String,
    name: String,
    content_type: String,
    size_bytes: u64,
    kind: String,
}

#[derive(Debug, Deserialize)]
struct ApiErrorResponse {
    code: String,
    message: String,
}

#[derive(Debug, Error)]
enum CliError {
    #[error("invalid API base URL: {0}")]
    InvalidBaseUrl(String),
    #[error("API token contains invalid header characters")]
    InvalidApiToken,
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("failed to decode API response: {0}")]
    Json(#[from] serde_json::Error),
    #[error("failed to write output file: {0}")]
    Io(#[from] std::io::Error),
    #[error("failed to encode completion script: {0}")]
    Utf8(#[from] std::string::FromUtf8Error),
    #[error("API returned {status}: {code}: {message}")]
    Api {
        status: StatusCode,
        code: String,
        message: String,
    },
}

#[cfg(test)]
mod tests;
