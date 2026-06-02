use std::{fmt::Write as _, process::ExitCode};

use clap::{Parser, Subcommand};
use reqwest::{StatusCode, Url};
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
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    #[command(about = "Submit a manual job run")]
    Submit {
        #[arg(help = "Job definition ID to run")]
        job_definition_id: String,
        #[arg(long, short = 'p', help = "Execution pool")]
        pool: String,
        #[arg(long, help = "Optional caller-provided run ID")]
        run_id: Option<String>,
    },
    #[command(about = "List job runs")]
    Runs {
        #[arg(long, default_value_t = 50, help = "Maximum runs to return")]
        limit: u16,
    },
    #[command(subcommand, about = "Inspect a job run")]
    Run(RunCommand),
}

#[derive(Debug, Subcommand)]
enum RunCommand {
    #[command(about = "Fetch one job run")]
    Get {
        #[arg(help = "Job run ID")]
        id: String,
    },
}

#[tokio::main]
async fn main() -> ExitCode {
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
    let api = ApiClient::new(cli.api_url);

    match cli.command {
        Command::Submit {
            job_definition_id,
            pool,
            run_id,
        } => {
            let request = CreateRunRequest {
                job_definition_id,
                execution_pool: pool,
                run_id,
            };
            let run = api.create_run(&request).await?;
            Ok(format_run_detail(&run))
        }
        Command::Runs { limit } => {
            let runs = api.list_runs(limit).await?;
            Ok(format_runs_table(&runs.runs))
        }
        Command::Run(RunCommand::Get { id }) => {
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
    fn new(base_url: Url) -> Self {
        Self {
            base_url,
            client: reqwest::Client::new(),
        }
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

#[derive(Debug, Serialize)]
struct CreateRunRequest {
    job_definition_id: String,
    execution_pool: String,
    run_id: Option<String>,
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
struct ApiErrorResponse {
    code: String,
    message: String,
}

#[derive(Debug, Error)]
enum CliError {
    #[error("invalid API base URL: {0}")]
    InvalidBaseUrl(String),
    #[error("request failed: {0}")]
    Request(#[from] reqwest::Error),
    #[error("failed to decode API response: {0}")]
    Json(#[from] serde_json::Error),
    #[error("API returned {status}: {code}: {message}")]
    Api {
        status: StatusCode,
        code: String,
        message: String,
    },
}

#[cfg(test)]
mod tests {
    use clap::Parser;
    use reqwest::Url;

    use super::{
        ApiClient, Cli, Command, JobRunResponse, RunCommand, format_run_detail, format_runs_table,
    };

    #[test]
    fn parses_submit_command() {
        let cli = Cli::parse_from([
            "capsulet",
            "--api-url",
            "http://localhost:8080",
            "submit",
            "job_hello_python",
            "--pool",
            "mini",
            "--run-id",
            "run_cli_test",
        ]);

        assert_eq!(cli.api_url.as_str(), "http://localhost:8080/");
        let Command::Submit {
            job_definition_id,
            pool,
            run_id,
        } = cli.command
        else {
            panic!("expected submit command");
        };
        assert_eq!(job_definition_id, "job_hello_python");
        assert_eq!(pool, "mini");
        assert_eq!(run_id.as_deref(), Some("run_cli_test"));
    }

    #[test]
    fn parses_run_get_command() {
        let cli = Cli::parse_from(["capsulet", "run", "get", "run_123"]);

        let Command::Run(RunCommand::Get { id }) = cli.command else {
            panic!("expected run get command");
        };
        assert_eq!(id, "run_123");
    }

    #[test]
    fn formats_run_detail() {
        let run = run("run_1", "succeeded", 1);

        assert_eq!(
            format_run_detail(&run),
            "id: run_1\njob_definition_id: job_hello_python\nstatus: succeeded\nexecution_pool: mini\nattempt_count: 1\n"
        );
    }

    #[test]
    fn formats_runs_table() {
        let output = format_runs_table(&[
            run("run_short", "queued", 0),
            run("run_longer", "failed", 2),
        ]);

        assert_eq!(
            output,
            "ID          JOB               STATUS  POOL  ATTEMPTS\nrun_short   job_hello_python  queued  mini  0\nrun_longer  job_hello_python  failed  mini  2\n"
        );
    }

    #[test]
    fn builds_api_urls_from_base_url_with_path() {
        let client =
            ApiClient::new(Url::parse("http://localhost:8080/api-prefix").expect("valid base URL"));

        let url = client.url(&["v1", "jobs", "runs", "run 1"]).expect("url");

        assert_eq!(url.as_str(), "http://localhost:8080/v1/jobs/runs/run%201");
    }

    fn run(id: &str, status: &str, attempt_count: u32) -> JobRunResponse {
        JobRunResponse {
            id: id.to_string(),
            job_definition_id: "job_hello_python".to_string(),
            status: status.to_string(),
            execution_pool: "mini".to_string(),
            attempt_count,
        }
    }
}
