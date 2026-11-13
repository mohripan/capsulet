use std::{fmt::Write as _, path::PathBuf, process::ExitCode};

use capsulet_runtime::bundle::{BundleError, SealedRunBundle};
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
    #[arg(
        long,
        env = "CAPSULET_PROJECT",
        global = true,
        help = "Project the request is about, for project-scoped resources such as agent runs"
    )]
    project: Option<String>,
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
    #[command(subcommand, about = "Inspect agent runs")]
    AgentRuns(AgentRunsCommand),
    #[command(subcommand, about = "Develop a validator for the verifier protocol")]
    Verifier(VerifierCommand),
    #[command(subcommand, about = "Make, sign, check and test domain packs")]
    Pack(PackCommand),
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
enum AgentRunsCommand {
    /// Exports a run as a digest-checked debug bundle, for `capsulet-replay`.
    ///
    /// The bundle holds the definition the run executed, its whole log, what the
    /// installation reported, and its trace id. It holds none of what the run was
    /// given or produced: the log names those by digest. The export is recorded
    /// in the project's audit log.
    #[command(about = "Export one agent run as a debug bundle")]
    Bundle {
        #[arg(help = "Agent run ID")]
        id: String,
        #[arg(long, short = 'o', help = "File to write the bundle to")]
        output: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum VerifierCommand {
    /// Runs one validator against a cases file, without a stack.
    ///
    /// The registration says what the validator is and where it runs: a
    /// `container` image pinned by digest, run isolated, or a local `process`,
    /// run unisolated, whose results would decide nothing in a real run. Each
    /// case names the obligations to ask about, the inputs to hand over, and the
    /// outcome expected for each obligation or the failure expected of the run.
    /// Exits non-zero when any case fails.
    #[command(about = "Run a validator against cases")]
    Test {
        #[arg(help = "Registration JSON file")]
        registration: PathBuf,
        #[arg(help = "Cases JSON file; input paths resolve beside it")]
        cases: PathBuf,
    },
}

#[derive(Debug, Subcommand)]
enum PackCommand {
    /// Makes a publisher key, and adds its public half to a trust store for the
    /// named packs. The key is a secret: keep it out of every pack and
    /// repository. An existing key file is never overwritten.
    #[command(about = "Make a publisher key and trust it for packs")]
    Keygen {
        #[arg(long, help = "File to write the private key to")]
        key: PathBuf,
        #[arg(long, help = "Trust store to add the public key to; made if missing")]
        trust: PathBuf,
        #[arg(long, num_args = 1.., required = true, help = "Pack names the key may sign, or *")]
        packs: Vec<String>,
    },
    /// Lists every file in the pack's manifest by digest, as signing does.
    #[command(about = "List a pack's files in its manifest")]
    Refresh {
        #[arg(help = "Pack directory")]
        dir: PathBuf,
    },
    /// Lists every file in the manifest, then signs the manifest, writing
    /// `pack.sig`.
    #[command(about = "Sign a pack")]
    Sign {
        #[arg(help = "Pack directory")]
        dir: PathBuf,
        #[arg(long, help = "Private key file")]
        key: PathBuf,
    },
    /// Checks a pack the way installing it would: every file listed and
    /// unchanged, the signature verifying under a key trusted for this pack,
    /// and the contracts and validators reading. Exits non-zero when it would
    /// be refused.
    #[command(about = "Check a pack against a trust store")]
    Verify {
        #[arg(help = "Pack directory")]
        dir: PathBuf,
        #[arg(long, help = "Trust store file")]
        trust: PathBuf,
    },
    /// Runs each validator in the pack against its cases, without a stack.
    /// Exits non-zero when any case fails.
    #[command(about = "Run a pack's validators against their cases")]
    Test {
        #[arg(help = "Pack directory")]
        dir: PathBuf,
    },
    /// Runs each replayable tool a certificate records again, in the container
    /// its record pins, with the inputs its bundle carries, and compares what it
    /// produced with the record. Also replays the certificate offline. Exits
    /// non-zero when a tool does not reach its record or the verdict does not
    /// reproduce. Needs Docker.
    #[command(about = "Run a certificate's tools again from its bundle")]
    Replay {
        #[arg(help = "Certificate bundle file")]
        bundle: PathBuf,
        #[arg(help = "Directory of the pack whose validators the certificate records")]
        dir: PathBuf,
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
        Command::Pack(command) => pack(command),
        Command::Verifier(VerifierCommand::Test {
            registration,
            cases,
        }) => verifier_test(&registration, &cases),
        Command::AgentRuns(AgentRunsCommand::Bundle { id, output }) => {
            let project = cli.project.ok_or(CliError::ProjectRequired)?;
            let api =
                ApiClient::new(cli.api_url, cli.api_token.as_deref())?.for_project(&project)?;
            let body = api.agent_run_bundle(&id).await?;
            let (sealed, summary) = checked_bundle(&body)?;
            let text = serde_json::to_string_pretty(&sealed)?;
            std::fs::write(&output, text + "\n")?;
            Ok(format!(
                "wrote {}: {summary}\nread it offline with: capsulet-replay {}\n",
                output.display(),
                output.display()
            ))
        }
    }
}

/// Runs one `capsulet pack` command.
fn pack(command: PackCommand) -> Result<String, CliError> {
    match command {
        PackCommand::Keygen { key, trust, packs } => {
            if key.exists() {
                return Err(CliError::Pack(capsulet_pack::PackError::Key(format!(
                    "{} exists, and a key is never overwritten",
                    key.display()
                ))));
            }
            let (private, _) = capsulet_pack::generate()?;
            let mut store = if trust.exists() {
                read_json(&trust)?
            } else {
                capsulet_pack::TrustStore::new()
            };
            store.keys.push(private.trusted_for(packs.clone())?);
            store.check()?;
            std::fs::write(&key, serde_json::to_string_pretty(&private)? + "\n")?;
            std::fs::write(&trust, serde_json::to_string_pretty(&store)? + "\n")?;
            Ok(format!(
                "wrote key {} to {} (a secret: keep it out of packs and repositories)\n\
                 trusted it for {} in {}\n",
                private.id,
                key.display(),
                packs.join(", "),
                trust.display()
            ))
        }
        PackCommand::Refresh { dir } => {
            let manifest = capsulet_pack::refresh(&dir)?;
            Ok(format!(
                "listed the files of {}: manifest {manifest}\n",
                dir.display()
            ))
        }
        PackCommand::Sign { dir, key } => {
            let private: capsulet_pack::PrivateKey = read_json(&key)?;
            let manifest = capsulet_pack::sign(&dir, &private)?;
            Ok(format!(
                "signed {}: manifest {manifest}, key {}\n",
                dir.display(),
                private.id
            ))
        }
        PackCommand::Verify { dir, trust } => {
            let store: capsulet_pack::TrustStore = read_json(&trust)?;
            let admitted = capsulet_pack::admit(&capsulet_pack::read(&dir)?, &store, &[])?;
            Ok(format!(
                "verified {} {}: manifest {}, signed by {}\n",
                admitted.name, admitted.version, admitted.manifest, admitted.key
            ))
        }
        PackCommand::Replay { bundle, dir } => {
            pack_replay(&bundle, &dir, &capsulet_verifier::ContainerTransport::new())
        }
        PackCommand::Test { dir } => {
            let reports = capsulet_pack::test(&capsulet_pack::read(&dir)?)?;
            let mut text = String::new();
            let mut all_passed = true;
            for report in &reports {
                let (lines, passed) = capsulet_verifier::harness::render(&report.cases);
                let _ = writeln!(text, "{}", report.registration);
                text.push_str(&lines);
                all_passed &= passed;
            }
            if all_passed {
                Ok(text)
            } else {
                Err(CliError::CasesFailed(text))
            }
        }
    }
}

/// Runs a certificate's tools again from its bundle, and replays it offline.
fn pack_replay(
    bundle: &std::path::Path,
    dir: &std::path::Path,
    transport: &impl capsulet_verifier::Transport,
) -> Result<String, CliError> {
    let failed = |detail: String| CliError::LocalFile {
        path: bundle.display().to_string(),
        detail,
    };
    let bytes = std::fs::read(bundle).map_err(|error| failed(error.to_string()))?;
    let bundle =
        capsulet_kernel::Bundle::read(&bytes).map_err(|error| failed(error.to_string()))?;
    let evidence = bundle
        .evidence()
        .map_err(|error| failed(error.to_string()))?;
    let pack = capsulet_pack::read(dir)?;
    let replays = capsulet_pack::replay_tools(&pack, &bundle.certificate, &evidence, transport)?;
    let mut text = String::new();
    let mut all_agree = true;
    for replay in &replays {
        match &replay.outcome {
            Ok(again) if again.agrees() => {
                let _ = writeln!(
                    text,
                    "agrees {}: the same inputs, environment, outputs and verdict",
                    replay.validator
                );
            }
            Ok(again) => {
                all_agree = false;
                let _ = writeln!(
                    text,
                    "DIFFERS {}: {}",
                    replay.validator,
                    again.differences.join("; ")
                );
            }
            Err(failure) => {
                all_agree = false;
                let _ = writeln!(
                    text,
                    "DID NOT RUN {}: {failure} ({})",
                    replay.validator,
                    failure.code()
                );
            }
        }
    }
    if replays.is_empty() {
        let _ = writeln!(
            text,
            "the certificate records no replayable tool to run again"
        );
    }
    let outcome = capsulet_kernel::replay(&bundle.certificate, &evidence);
    let reproduced = matches!(outcome, capsulet_kernel::ReplayOutcome::Reproduced { .. });
    let _ = writeln!(
        text,
        "verdict {}: {}",
        bundle.certificate.verdict().as_str(),
        if reproduced {
            "reproduced offline"
        } else {
            "NOT reproduced offline"
        }
    );
    if all_agree && reproduced {
        Ok(text)
    } else {
        Err(CliError::Replayed(text))
    }
}

/// A JSON file named on the command line.
fn read_json<T: serde::de::DeserializeOwned>(path: &std::path::Path) -> Result<T, CliError> {
    let failed = |detail: String| CliError::LocalFile {
        path: path.display().to_string(),
        detail,
    };
    let bytes = std::fs::read(path).map_err(|error| failed(error.to_string()))?;
    serde_json::from_slice(&bytes).map_err(|error| failed(error.to_string()))
}

/// Runs a validator against its cases and reports each one.
fn verifier_test(
    registration: &std::path::Path,
    cases: &std::path::Path,
) -> Result<String, CliError> {
    let registration: capsulet_verifier::Registration = read_json(registration)?;
    let parsed: capsulet_verifier::harness::Cases = read_json(cases)?;
    let base = cases.parent().unwrap_or_else(|| std::path::Path::new("."));
    let reports = capsulet_verifier::harness::run_cases(&registration, &parsed, base);
    let (text, all_passed) = capsulet_verifier::harness::render(&reports);
    if all_passed {
        Ok(text)
    } else {
        Err(CliError::CasesFailed(text))
    }
}

/// The bundle a response carries, after checking it is one and that its body is
/// the one its digest names, and a line saying what it holds. Nothing is written
/// for a bundle that does not check.
fn checked_bundle(body: &str) -> Result<(SealedRunBundle, String), CliError> {
    let sealed: SealedRunBundle = serde_json::from_str(body)?;
    let bundle = sealed.open()?;
    let summary = format!(
        "run {} of agent {}, {} events, bundle version {}, digest {}",
        bundle.run_id,
        bundle.agent_id,
        bundle.events.len(),
        sealed.bundle_version,
        sealed.digest
    );
    Ok((sealed, summary))
}

#[derive(Debug, Clone)]
struct ApiClient {
    base_url: Url,
    client: reqwest::Client,
    headers: HeaderMap,
}

/// The header that names the project a project-scoped request is about.
const PROJECT_HEADER: &str = "x-capsulet-project-id";

impl ApiClient {
    fn new(base_url: Url, token: Option<&str>) -> Result<Self, CliError> {
        let mut headers = HeaderMap::new();
        if let Some(token) = token.filter(|token| !token.is_empty()) {
            let value = HeaderValue::from_str(&format!("Bearer {token}"))
                .map_err(|_| CliError::InvalidApiToken)?;
            headers.insert(AUTHORIZATION, value);
        }
        Self::with_headers(base_url, headers)
    }

    fn with_headers(base_url: Url, headers: HeaderMap) -> Result<Self, CliError> {
        Ok(Self {
            base_url,
            client: reqwest::Client::builder()
                .default_headers(headers.clone())
                .build()?,
            headers,
        })
    }

    /// The same client, naming `project` on every request.
    fn for_project(self, project: &str) -> Result<Self, CliError> {
        let mut headers = self.headers;
        let value = HeaderValue::from_str(project)
            .map_err(|_| CliError::InvalidProject(project.to_string()))?;
        headers.insert(PROJECT_HEADER, value);
        Self::with_headers(self.base_url, headers)
    }

    /// `POST /v1/agent-runs/{id}/debug-bundle`, as the text the API returned, so
    /// the digest is checked over exactly the bytes that came back.
    async fn agent_run_bundle(&self, id: &str) -> Result<String, CliError> {
        let response = self
            .client
            .post(self.url(&["v1", "agent-runs", id, "debug-bundle"])?)
            .json(&serde_json::json!({}))
            .send()
            .await?;
        let status = response.status();
        let body = response.text().await?;
        if status.is_success() {
            return Ok(body);
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
    #[error("this command needs a project: pass --project or set CAPSULET_PROJECT")]
    ProjectRequired,
    #[error("project id contains invalid header characters: {0:?}")]
    InvalidProject(String),
    #[error("the API returned a bundle that does not check: {0}")]
    Bundle(#[from] BundleError),
    #[error("a validator case failed:\n{0}")]
    CasesFailed(String),
    /// A replay that did not reach the certificate: a tool that differs from its
    /// record or did not run, or a verdict that does not reproduce.
    #[error("the certificate's tools or verdict did not replay:\n{0}")]
    Replayed(String),
    /// A file named on the command line that is missing or is not the JSON it
    /// should be: a key, a trust store, a registration, or a cases file.
    #[error("{path} cannot be read: {detail}")]
    LocalFile { path: String, detail: String },
    #[error("{} ({})", .0, .0.code())]
    Pack(#[from] capsulet_pack::PackError),
    #[error("API returned {status}: {code}: {message}")]
    Api {
        status: StatusCode,
        code: String,
        message: String,
    },
}

#[cfg(test)]
mod tests;
