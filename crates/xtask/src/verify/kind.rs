//! The `kind` gate: Capsulet installed by Helm into a disposable Kind cluster
//! from the images this checkout builds, exercised from inside the cluster,
//! uninstalled, and the cluster deleted.
//!
//! `kind smoke` does all of it, in this order:
//!
//! 1. builds every component's image, as the `containers` gate does, and checks
//!    each one's labels and user;
//! 2. creates a cluster named after this verifier run, with a kubeconfig of its
//!    own (the developer's is never read or written), and loads those images;
//! 3. generates the test credentials: API tokens for an administrator and for
//!    an operator and a viewer of one project, the dashboard's bootstrap login
//!    and the database password;
//! 4. validates the release before installing it: `helm lint`; the rendered
//!    manifests held to `release check-chart`'s workload rules; every image
//!    either one just built or a digest-pinned dependency; every secret a pod
//!    reads either rendered or created; and the API server's own validation of
//!    every manifest, by a server-side dry run;
//! 5. installs the chart, waits for every deployment, and runs `helm test`;
//! 6. runs `tests/kind/smoke.py` as a Job inside the cluster, which reaches the
//!    API and the dashboard through their Services;
//! 7. uninstalls, and fails if anything the release created is still there.
//!
//! On any failure it collects the namespace's events, pod descriptions and
//! logs. In every case it deletes its cluster. `kind delete`, the gate's
//! teardown, deletes this run's cluster again in case the smoke was killed
//! before it could, and nothing else: it only ever names this run's cluster.

use std::{
    env, fs,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant, SystemTime, UNIX_EPOCH},
};

use serde_json::{Value, json};
use serde_yaml::Value as Yaml;

use super::RUN_VARIABLE;
use crate::{
    fixtures::random_secret,
    release::{self, BuildIdentity, Release},
};

const DIRECTORY: &str = ".capsulet/kind";
const RELEASE_NAME: &str = "capsulet";
const NAMESPACE: &str = "capsulet";
const CLUSTER_CONFIG: &str = "tests/kind/cluster.yaml";
const SMOKE_SCRIPT: &str = "tests/kind/smoke.py";
const SMOKE_JOB: &str = "tests/kind/smoke-job.yaml";
const API_AUTH_SECRET: &str = "capsulet-api-auth";
const DASHBOARD_LOGIN_SECRET: &str = "capsulet-dashboard-login";
const SMOKE_SECRET: &str = "capsulet-smoke";

const USAGE: &str = "usage: capsulet-xtask kind smoke | kind delete";

pub(crate) fn run(arguments: &[String]) -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    match arguments {
        [command] if command == "smoke" => smoke(&root),
        [command] if command == "delete" => delete_this_run(&root),
        _ => Err(USAGE.to_string()),
    }
}

/// The cluster's name: this verifier run's, or a fresh one outside a run.
fn cluster_name() -> Result<String, String> {
    let run = match env::var(RUN_VARIABLE) {
        Ok(run) if !run.is_empty() => run,
        _ => {
            let now = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_err(|error| error.to_string())?;
            format!("{}-{}", now.as_millis(), std::process::id())
        }
    };
    let name: String = format!("capsulet-verify-{run}")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() {
                c.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect();
    Ok(name)
}

/// The test credentials of one cluster. None is ever written outside the
/// cluster's directory, which is deleted with the cluster.
struct Credentials {
    admin: String,
    operator: String,
    viewer: String,
    dashboard_password: String,
    postgres_password: String,
    expires_at_unix: u64,
}

impl Credentials {
    fn generate() -> Result<Self, String> {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| error.to_string())?
            .as_secs();
        Ok(Self {
            admin: random_secret()?,
            operator: random_secret()?,
            viewer: random_secret()?,
            dashboard_password: random_secret()?,
            postgres_password: random_secret()?,
            expires_at_unix: now + 4 * 60 * 60,
        })
    }

    /// `CAPSULET_API_TOKENS`: the bootstrap identities, each expiring.
    fn api_tokens(&self) -> String {
        let identity = |name: &str, role: &str, project: &str, token: &str| {
            json!({
                "name": name, "role": role, "tenant_id": "default", "project_id": project,
                "token": token, "expires_at_unix": self.expires_at_unix,
            })
        };
        Value::Array(vec![
            identity("kind-admin", "admin", "default", &self.admin),
            identity("kind-operator", "operator", "kind-alpha", &self.operator),
            identity("kind-viewer", "viewer", "kind-alpha", &self.viewer),
        ])
        .to_string()
    }
}

struct Cluster {
    root: PathBuf,
    name: String,
    directory: PathBuf,
    kubeconfig: PathBuf,
    images: Vec<String>,
    created: bool,
    deleted: bool,
}

impl Cluster {
    fn prepare(root: &Path) -> Result<Self, String> {
        let name = cluster_name()?;
        let directory = root.join(DIRECTORY).join(&name);
        fs::create_dir_all(&directory)
            .map_err(|error| format!("could not create {}: {error}", directory.display()))?;
        // The record `kind delete` reads, written before anything exists to delete.
        fs::write(directory.join("cluster"), &name).map_err(|error| error.to_string())?;
        Ok(Self {
            root: root.to_path_buf(),
            kubeconfig: directory.join("kubeconfig"),
            name,
            directory,
            images: Vec::new(),
            created: false,
            deleted: false,
        })
    }

    fn kubectl(&self) -> Command {
        let mut command = Command::new("kubectl");
        command.arg("--kubeconfig").arg(&self.kubeconfig);
        command
    }

    fn helm(&self) -> Command {
        let mut command = Command::new("helm");
        command
            .arg("--kubeconfig")
            .arg(&self.kubeconfig)
            .current_dir(&self.root);
        command
    }

    fn file(&self, name: &str, contents: &str) -> Result<PathBuf, String> {
        let path = self.directory.join(name);
        fs::write(&path, contents).map_err(|error| format!("{}: {error}", path.display()))?;
        Ok(path)
    }

    fn build_images(&mut self) -> Result<(), String> {
        step("building every component's image from this checkout");
        let result = release::build_verified(&self.root, &mut self.images);
        fs::write(self.directory.join("images"), self.images.join("\n"))
            .map_err(|error| error.to_string())?;
        result
    }

    fn create(&mut self) -> Result<(), String> {
        step(&format!("creating the Kind cluster {}", self.name));
        self.created = true;
        exec(
            Command::new("kind")
                .args(["create", "cluster", "--name", &self.name, "--config"])
                .arg(self.root.join(CLUSTER_CONFIG))
                .arg("--kubeconfig")
                .arg(&self.kubeconfig)
                .args(["--wait", "180s"]),
        )?;
        step("loading the images just built into the cluster");
        exec(
            Command::new("kind")
                .args(["load", "docker-image", "--name", &self.name])
                .args(&self.images),
        )
    }

    fn values(
        &self,
        identity: &BuildIdentity,
        credentials: &Credentials,
    ) -> Result<PathBuf, String> {
        let tag = format!("{}-{}", identity.version, std::process::id());
        let values = format!(
            "# Generated for one Kind cluster and deleted with it.\n\
             image:\n  registry: \"\"\n  namespace: capsulet-verify\n  tag: \"{tag}\"\n  pullPolicy: Never\n\
             api:\n  replicaCount: 1\n\
             worker:\n  replicaCount: 1\n\
             graphWorker:\n  replicaCount: 1\n\
             evaluator:\n  replicaCount: 1\n\
             dashboard:\n  replicaCount: 1\n  temporaryAdmin:\n    existingSecret: {DASHBOARD_LOGIN_SECRET}\n\
             postgresql:\n  auth:\n    password: \"{}\"\n  persistence:\n    enabled: false\n\
             seaweedfs:\n  persistence:\n    enabled: false\n",
            credentials.postgres_password
        );
        self.file("values.yaml", &values)
    }

    fn secrets(&self, identity: &BuildIdentity, credentials: &Credentials) -> Result<(), String> {
        step("creating the namespace and the generated credentials");
        exec(self.kubectl().args(["create", "namespace", NAMESPACE]))?;
        let tokens = self.file("tokens.json", &credentials.api_tokens())?;
        exec(
            self.kubectl()
                .args([
                    "create",
                    "secret",
                    "generic",
                    API_AUTH_SECRET,
                    "-n",
                    NAMESPACE,
                ])
                .arg(format!("--from-file=tokens={}", tokens.display())),
        )?;
        let password = self.file("dashboard-password", &credentials.dashboard_password)?;
        let token = self.file("dashboard-token", &credentials.admin)?;
        exec(
            self.kubectl()
                .args([
                    "create",
                    "secret",
                    "generic",
                    DASHBOARD_LOGIN_SECRET,
                    "-n",
                    NAMESPACE,
                ])
                .arg("--from-literal=username=kind-admin")
                .arg(format!("--from-file=password={}", password.display()))
                .arg(format!("--from-file=token={}", token.display())),
        )?;
        let smoke = self.file(
            "smoke.env",
            &format!(
                "CAPSULET_SMOKE_VERSION={}\nCAPSULET_SMOKE_ADMIN_TOKEN={}\n\
                 CAPSULET_SMOKE_OPERATOR_TOKEN={}\nCAPSULET_SMOKE_VIEWER_TOKEN={}\n\
                 CAPSULET_SMOKE_DASHBOARD_USER=kind-admin\nCAPSULET_SMOKE_DASHBOARD_PASSWORD={}\n",
                identity.version,
                credentials.admin,
                credentials.operator,
                credentials.viewer,
                credentials.dashboard_password
            ),
        )?;
        exec(
            self.kubectl()
                .args(["create", "secret", "generic", SMOKE_SECRET, "-n", NAMESPACE])
                .arg(format!("--from-env-file={}", smoke.display())),
        )
    }

    /// Everything that can be known about the release before it is installed.
    fn validate(&self, release: &Release, values: &Path) -> Result<(), String> {
        step("linting the chart with this cluster's values");
        exec(
            self.helm()
                .args(["lint", "charts/capsulet", "-f"])
                .arg(values),
        )?;
        step("rendering the release and checking every workload");
        let arguments = vec![
            "--namespace".to_string(),
            NAMESPACE.to_string(),
            "-f".to_string(),
            values.display().to_string(),
        ];
        let documents = release::chart_documents(&self.root, &arguments)?;
        let chart_values: Yaml = serde_yaml::from_str(
            &fs::read_to_string(self.root.join("charts/capsulet/values.yaml"))
                .map_err(|error| error.to_string())?,
        )
        .map_err(|error| error.to_string())?;
        let overrides: Yaml =
            serde_yaml::from_str(&fs::read_to_string(values).map_err(|error| error.to_string())?)
                .map_err(|error| error.to_string())?;
        let mut both = serde_yaml::Mapping::new();
        both.insert(Yaml::from("defaults"), chart_values);
        both.insert(Yaml::from("cluster"), overrides);
        let mut findings = release::workload_findings(&documents, release, &Yaml::Mapping(both));
        for image in release::workload_images(&documents) {
            if !self.images.contains(&image) && !image.contains("@sha256:") {
                findings.push(format!(
                    "a workload runs {image}, neither an image this run built nor one pinned by \
                     digest"
                ));
            }
        }
        for (workload, secret) in release::secrets_read(&documents) {
            let rendered = documents.iter().any(|manifest| {
                manifest["kind"].as_str() == Some("Secret")
                    && manifest["metadata"]["name"].as_str() == Some(secret.as_str())
            });
            if !rendered
                && !self
                    .kubectl()
                    .args(["get", "secret", &secret, "-n", NAMESPACE])
                    .stdout(Stdio::null())
                    .stderr(Stdio::null())
                    .status()
                    .is_ok_and(|status| status.success())
            {
                findings.push(format!(
                    "{workload} reads the secret {secret}, which neither the chart nor this \
                     install provides"
                ));
            }
        }
        if !findings.is_empty() {
            return Err(format!(
                "the release would install with {} defect(s):\n  - {}",
                findings.len(),
                findings.join("\n  - ")
            ));
        }
        let rendered = self.file(
            "manifests.yaml",
            &documents
                .iter()
                .map(|document| serde_yaml::to_string(document).unwrap_or_default())
                .collect::<Vec<_>>()
                .join("---\n"),
        )?;
        step("validating every manifest against the API server (server-side dry run)");
        exec(
            self.kubectl()
                .args(["apply", "--dry-run=server", "-n", NAMESPACE, "-f"])
                .arg(&rendered)
                .stdout(Stdio::null()),
        )
    }

    fn install(&self, values: &Path) -> Result<(), String> {
        step("installing the chart and waiting for it to be ready");
        exec(
            self.helm()
                .args([
                    "install",
                    RELEASE_NAME,
                    "charts/capsulet",
                    "-n",
                    NAMESPACE,
                    "-f",
                ])
                .arg(values)
                .args(["--wait", "--wait-for-jobs", "--timeout", "15m"]),
        )?;
        exec(self.kubectl().args([
            "wait",
            "--for=condition=Available",
            "deployment",
            "--all",
            "-n",
            NAMESPACE,
            "--timeout=5m",
        ]))?;
        step("running the chart's own test");
        exec(
            self.helm()
                .args(["test", RELEASE_NAME, "-n", NAMESPACE, "--timeout", "5m"]),
        )
    }

    fn smoke(&self, api_image: &str) -> Result<(), String> {
        step("running the smoke inside the cluster, against the release's Services");
        exec(
            self.kubectl()
                .args(["create", "configmap", SMOKE_SECRET, "-n", NAMESPACE])
                .arg(format!(
                    "--from-file=smoke.py={}",
                    self.root.join(SMOKE_SCRIPT).display()
                )),
        )?;
        let template = fs::read_to_string(self.root.join(SMOKE_JOB))
            .map_err(|error| format!("{SMOKE_JOB}: {error}"))?;
        let job = self.file(
            "smoke-job.yaml",
            &template
                .replace("@IMAGE@", api_image)
                .replace("@RELEASE@", RELEASE_NAME),
        )?;
        exec(
            self.kubectl()
                .args(["apply", "-n", NAMESPACE, "-f"])
                .arg(&job),
        )?;
        let deadline = Instant::now() + Duration::from_mins(15);
        let outcome = loop {
            let status = output(self.kubectl().args([
                "get",
                "job",
                "capsulet-smoke",
                "-n",
                NAMESPACE,
                "-o",
                "json",
            ]))?;
            let status: Value = serde_json::from_str(&status).map_err(|error| error.to_string())?;
            if status["status"]["succeeded"].as_u64().unwrap_or(0) > 0 {
                break Ok(());
            }
            if status["status"]["failed"].as_u64().unwrap_or(0) > 0 {
                break Err("the smoke Job failed".to_string());
            }
            if Instant::now() > deadline {
                break Err("the smoke Job did not finish within 15 minutes".to_string());
            }
            thread::sleep(Duration::from_secs(5));
        };
        let logs = output(
            self.kubectl()
                .args(["logs", "-n", NAMESPACE, "job/capsulet-smoke"]),
        )
        .unwrap_or_default();
        println!("{logs}");
        outcome?;
        if !logs.contains("PASS kind smoke") {
            return Err("the smoke Job succeeded without reporting PASS kind smoke".to_string());
        }
        // Not the release's: removed before the uninstall is held to leaving nothing.
        exec(self.kubectl().args([
            "delete",
            "job",
            "capsulet-smoke",
            "-n",
            NAMESPACE,
            "--wait=true",
        ]))?;
        exec(
            self.kubectl()
                .args(["delete", "configmap,secret", SMOKE_SECRET, "-n", NAMESPACE]),
        )
    }

    fn uninstall(&self) -> Result<(), String> {
        step("uninstalling, and checking the release left nothing behind");
        exec(self.helm().args([
            "uninstall",
            RELEASE_NAME,
            "-n",
            NAMESPACE,
            "--wait",
            "--timeout",
            "5m",
        ]))?;
        // `--wait` returns once the objects Helm deleted are gone, but the
        // pods they owned are collected after that and may still be shutting
        // down. Anything still there once they have had time to is left behind.
        let deadline = Instant::now() + Duration::from_mins(3);
        loop {
            let left = self.release_objects()?;
            if left.is_empty() {
                return Ok(());
            }
            if Instant::now() >= deadline {
                return Err(format!(
                    "helm uninstall left {} object(s) of the release behind: {}",
                    left.len(),
                    left.join(", ")
                ));
            }
            thread::sleep(Duration::from_secs(5));
        }
    }

    /// Every object, namespaced or cluster-wide, labelled as this release's.
    fn release_objects(&self) -> Result<Vec<String>, String> {
        let selector = format!("app.kubernetes.io/instance={RELEASE_NAME}");
        let namespaced = output(self.kubectl().args([
            "get",
            "all,configmap,secret,serviceaccount,role,rolebinding,networkpolicy,\
             poddisruptionbudget,horizontalpodautoscaler,persistentvolumeclaim,job,cronjob",
            "-n",
            NAMESPACE,
            "-l",
            &selector,
            "-o",
            "name",
        ]))?;
        let cluster_wide = output(self.kubectl().args([
            "get",
            "clusterrole,clusterrolebinding,validatingadmissionpolicy,\
             validatingadmissionpolicybinding",
            "-l",
            &selector,
            "-o",
            "name",
        ]))?;
        Ok(namespaced
            .lines()
            .chain(cluster_wide.lines())
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(str::to_string)
            .collect())
    }

    /// What someone needs to see why it failed, written to the gate log.
    fn collect_diagnostics(&self) {
        step("collecting the namespace's events, pod descriptions and logs");
        for arguments in [
            vec!["get", "all", "-n", NAMESPACE, "-o", "wide"],
            vec!["get", "events", "-n", NAMESPACE, "--sort-by=.lastTimestamp"],
            vec!["describe", "pods", "-n", NAMESPACE],
            vec![
                "logs",
                "-n",
                NAMESPACE,
                "--all-containers",
                "--prefix",
                "--tail=400",
                "--max-log-requests=30",
                "-l",
                "app.kubernetes.io/instance=capsulet",
            ],
            vec!["logs", "-n", NAMESPACE, "job/capsulet-smoke", "--tail=400"],
        ] {
            println!("[kind] $ kubectl {}", arguments.join(" "));
            let _ = self.kubectl().args(&arguments).status();
        }
        let _ = self
            .helm()
            .args(["status", RELEASE_NAME, "-n", NAMESPACE])
            .status();
    }

    fn delete(&mut self) -> Result<(), String> {
        if self.deleted {
            return Ok(());
        }
        let result = if self.created {
            step(&format!("deleting the Kind cluster {}", self.name));
            exec(Command::new("kind").args(["delete", "cluster", "--name", &self.name]))
        } else {
            Ok(())
        };
        remove_images(&self.images);
        let _ = fs::remove_dir_all(&self.directory);
        self.deleted = true;
        result
    }
}

impl Drop for Cluster {
    fn drop(&mut self) {
        let _ = self.delete();
    }
}

fn smoke(root: &Path) -> Result<(), String> {
    let release = Release::load(root)?;
    let identity = BuildIdentity::current(root)?;
    let mut cluster = Cluster::prepare(root)?;
    let result = exercise(&mut cluster, &release, &identity);
    if result.is_err() && cluster.created {
        cluster.collect_diagnostics();
    }
    let deleted = cluster.delete();
    result?;
    deleted?;
    println!(
        "[kind] PASS: Capsulet {} installed, smoke-tested inside the cluster and uninstalled \
         cleanly; the cluster is deleted",
        identity.version
    );
    Ok(())
}

fn exercise(
    cluster: &mut Cluster,
    release: &Release,
    identity: &BuildIdentity,
) -> Result<(), String> {
    cluster.build_images()?;
    let api_image = format!(
        "capsulet-verify/{}:{}-{}",
        release.images.components["api"].image,
        identity.version,
        std::process::id()
    );
    if !cluster.images.contains(&api_image) {
        return Err(format!("the API image {api_image} was not built"));
    }
    cluster.create()?;
    let credentials = Credentials::generate()?;
    let values = cluster.values(identity, &credentials)?;
    cluster.secrets(identity, &credentials)?;
    cluster.validate(release, &values)?;
    cluster.install(&values)?;
    cluster.smoke(&api_image)?;
    cluster.uninstall()
}

/// The gate's teardown: deletes this verifier run's cluster if the smoke did
/// not get to, and nothing else.
fn delete_this_run(root: &Path) -> Result<(), String> {
    if env::var(RUN_VARIABLE).map_or(true, |run| run.is_empty()) {
        return Err(format!(
            "kind delete only removes a verifier run's own cluster, named by {RUN_VARIABLE}"
        ));
    }
    let name = cluster_name()?;
    let directory = root.join(DIRECTORY).join(&name);
    if !directory.exists() {
        return Ok(());
    }
    let clusters = output(Command::new("kind").args(["get", "clusters"])).unwrap_or_default();
    if clusters.lines().any(|line| line.trim() == name) {
        step(&format!(
            "deleting the Kind cluster {name}, which the smoke left"
        ));
        exec(Command::new("kind").args(["delete", "cluster", "--name", &name]))?;
    }
    let images = fs::read_to_string(directory.join("images")).unwrap_or_default();
    remove_images(&images.lines().map(str::to_string).collect::<Vec<_>>());
    fs::remove_dir_all(&directory).map_err(|error| error.to_string())
}

fn remove_images(images: &[String]) {
    for image in images.iter().filter(|image| !image.is_empty()) {
        let _ = Command::new("docker")
            .args(["image", "rm", "--force", image])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
    }
}

fn step(what: &str) {
    println!("[kind] {what}");
}

fn exec(command: &mut Command) -> Result<(), String> {
    let description = format!("{command:?}");
    let status = command
        .status()
        .map_err(|error| format!("could not run {description}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{description} failed ({status})"))
    }
}

fn output(command: &mut Command) -> Result<String, String> {
    let description = format!("{command:?}");
    let output = command
        .output()
        .map_err(|error| format!("could not run {description}: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "{description} failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}
