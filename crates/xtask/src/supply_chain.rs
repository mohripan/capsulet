//! `supply-chain check`: the workflows and Dockerfiles that build, scan and
//! publish the release hold to the supply-chain contract
//! (`docs/security/supply-chain.md`).
//!
//! Every workflow under `.github/workflows` is parsed, and each rule names the
//! file and job that breaks it:
//!
//! - every third-party action is pinned to a full commit SHA, with a `# vX.Y.Z`
//!   comment naming the release, and Dependabot proposes newer pins;
//! - nothing a pull request can trigger gets a write permission other than
//!   code scanning, `pull_request_target` is never used, and no required check
//!   is allowed to fail soft;
//! - a job builds an image at most once. A job that publishes pushes it by
//!   digest with no tag, and scans, describes, attests and signs that digest
//!   alone. It verifies the signature, the provenance and the SBOM before it
//!   tags anything;
//! - the release manifest is written only after every image is verified again;
//! - a CI job scans the whole Git history for secrets;
//! - every base image a Dockerfile pulls is pinned by digest.

use std::{collections::BTreeSet, fs, path::Path};

use serde_yaml::Value;

use crate::release::{git, split_root};

const WORKFLOWS: &str = ".github/workflows";
const DEPENDABOT: &str = ".github/dependabot.yml";

/// The one write permission a pull request's workflow may hold: uploading its
/// own scan results.
const PULL_REQUEST_WRITES: &[&str] = &["security-events"];

const USAGE: &str = "usage: capsulet-xtask supply-chain check [--root <directory>]";

pub(crate) fn run(arguments: &[String]) -> Result<(), String> {
    let (root, arguments) = split_root(arguments)?;
    if arguments != ["check"] {
        return Err(USAGE.to_string());
    }
    let findings = check(&root)?;
    if findings.is_empty() {
        println!(
            "[supply-chain] every workflow and Dockerfile holds to the contract in \
             docs/security/supply-chain.md"
        );
        return Ok(());
    }
    let mut message = format!("{} supply-chain contract violation(s):\n", findings.len());
    for finding in &findings {
        message.push_str("  - ");
        message.push_str(finding);
        message.push('\n');
    }
    message.push_str("See docs/security/supply-chain.md for what each rule protects.");
    Err(message)
}

struct Workflow {
    path: String,
    text: String,
    document: Value,
}

impl Workflow {
    fn jobs(&self) -> impl Iterator<Item = (&str, &Value)> {
        self.document["jobs"]
            .as_mapping()
            .into_iter()
            .flatten()
            .filter_map(|(name, job)| Some((name.as_str()?, job)))
    }

    fn events(&self) -> BTreeSet<String> {
        match &self.document["on"] {
            Value::String(event) => BTreeSet::from([event.clone()]),
            Value::Sequence(events) => events
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
            Value::Mapping(events) => events
                .keys()
                .filter_map(Value::as_str)
                .map(str::to_string)
                .collect(),
            _ => BTreeSet::new(),
        }
    }
}

pub(crate) fn check(root: &Path) -> Result<Vec<String>, String> {
    let workflows = load(root)?;
    let mut findings = Findings::default();
    for workflow in &workflows {
        pins(root, workflow, &mut findings);
        pull_requests(workflow, &workflows, &mut findings);
        soft_failures(workflow, &mut findings);
        for (name, job) in workflow.jobs() {
            images(workflow, name, job, &mut findings);
            signing(workflow, name, job, &mut findings);
        }
    }
    publication(&workflows, &mut findings);
    secret_scanning(&workflows, &mut findings);
    dependabot(root, &mut findings);
    dockerfiles(root, &mut findings)?;
    Ok(findings.0)
}

#[derive(Default)]
struct Findings(Vec<String>);

impl Findings {
    fn push(&mut self, finding: String) {
        if !self.0.contains(&finding) {
            self.0.push(finding);
        }
    }
}

fn load(root: &Path) -> Result<Vec<Workflow>, String> {
    let directory = root.join(WORKFLOWS);
    let mut paths: Vec<_> = fs::read_dir(&directory)
        .map_err(|error| format!("{WORKFLOWS}: {error}"))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "yml" || extension == "yaml")
        })
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| {
            let name = format!(
                "{WORKFLOWS}/{}",
                path.file_name().unwrap_or_default().to_string_lossy()
            );
            let text = fs::read_to_string(&path).map_err(|error| format!("{name}: {error}"))?;
            let mut document: Value =
                serde_yaml::from_str(&text).map_err(|error| format!("{name}: {error}"))?;
            document
                .apply_merge()
                .map_err(|error| format!("{name}: {error}"))?;
            Ok(Workflow {
                path: name,
                text,
                document,
            })
        })
        .collect()
}

fn steps(job: &Value) -> Vec<&Value> {
    job["steps"].as_sequence().into_iter().flatten().collect()
}

fn step_name(step: &Value, index: usize) -> String {
    step["name"]
        .as_str()
        .or_else(|| step["uses"].as_str())
        .map_or_else(|| format!("step {}", index + 1), str::to_string)
}

fn uses(step: &Value, action: &str) -> bool {
    step["uses"]
        .as_str()
        .is_some_and(|uses| uses.split('@').next() == Some(action))
}

fn script(step: &Value) -> &str {
    step["run"].as_str().unwrap_or_default()
}

fn with<'a>(step: &'a Value, input: &str) -> &'a str {
    step["with"][input].as_str().unwrap_or_default()
}

/// Every `uses:` in the workflow, with where it is.
fn references(workflow: &Workflow) -> Vec<(String, String)> {
    let mut found = Vec::new();
    for (name, job) in workflow.jobs() {
        if let Some(uses) = job["uses"].as_str() {
            found.push((format!("job {name}"), uses.to_string()));
        }
        for (index, step) in steps(job).iter().enumerate() {
            if let Some(uses) = step["uses"].as_str() {
                found.push((
                    format!("job {name}, {}", step_name(step, index)),
                    uses.to_string(),
                ));
            }
        }
    }
    found
}

fn pins(root: &Path, workflow: &Workflow, findings: &mut Findings) {
    for (location, reference) in references(workflow) {
        let path = &workflow.path;
        if let Some(local) = reference.strip_prefix("./") {
            if !root.join(local).exists() {
                findings.push(format!(
                    "{path}: {location} uses {reference}, which does not exist"
                ));
            }
            continue;
        }
        if let Some(image) = reference.strip_prefix("docker://") {
            if !image.contains("@sha256:") {
                findings.push(format!(
                    "{path}: {location} runs the container {image}, which is not pinned by digest"
                ));
            }
            continue;
        }
        let pinned = reference.rsplit_once('@').is_some_and(|(_, sha)| {
            sha.len() == 40 && sha.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
        });
        if !pinned {
            findings.push(format!(
                "{path}: {location} uses {reference}, which is not pinned to a full commit SHA; a \
                 tag can be moved to other code after review"
            ));
            continue;
        }
        let commented = workflow.text.lines().any(|line| {
            line.split_once("uses:").is_some_and(|(_, rest)| {
                rest.trim_start().starts_with(&reference)
                    && rest.split_once('#').is_some_and(|(_, comment)| {
                        comment.trim_start().starts_with('v')
                            && comment.trim_start()[1..].starts_with(|c: char| c.is_ascii_digit())
                    })
            })
        });
        if !commented {
            findings.push(format!(
                "{path}: {location} pins {reference} without a `# vX.Y.Z` comment naming the \
                 release, which Dependabot updates and reviewers read"
            ));
        }
    }
}

/// Nothing a pull request can start may write anything but its own scan
/// results, or read a secret: a pull request's code runs there.
fn pull_requests(workflow: &Workflow, workflows: &[Workflow], findings: &mut Findings) {
    let events = workflow.events();
    let path = &workflow.path;
    if events.contains("pull_request_target") {
        findings.push(format!(
            "{path}: runs on pull_request_target, which gives a pull request's code the \
             repository's secrets and a write token"
        ));
    }
    if !events.contains("pull_request") && !events.contains("pull_request_target") {
        return;
    }
    let mut seen = BTreeSet::new();
    pull_request_privileges(workflow, workflows, path, true, &mut seen, findings);
}

fn pull_request_privileges(
    workflow: &Workflow,
    workflows: &[Workflow],
    trigger: &str,
    top_level_required: bool,
    seen: &mut BTreeSet<String>,
    findings: &mut Findings,
) {
    if !seen.insert(workflow.path.clone()) {
        return;
    }
    let path = &workflow.path;
    let reached = if path == trigger {
        String::new()
    } else {
        format!(", which {trigger} calls for pull requests,")
    };
    match &workflow.document["permissions"] {
        Value::Null if top_level_required => findings.push(format!(
            "{path}: runs for pull requests without a top-level `permissions`, so it gets the \
             repository's default token"
        )),
        permissions => {
            for scope in writes(permissions) {
                findings.push(format!(
                    "{path}{reached} grants `{scope}: write` to pull request code"
                ));
            }
        }
    }
    for (name, job) in workflow.jobs() {
        for scope in writes(&job["permissions"]) {
            findings.push(format!(
                "{path}{reached} job {name} grants `{scope}: write` to pull request code"
            ));
        }
        if job["secrets"].as_str() == Some("inherit") {
            findings.push(format!(
                "{path}{reached} job {name} passes every secret to pull request code"
            ));
        }
        if let Some(callee) = job["uses"]
            .as_str()
            .and_then(|uses| uses.strip_prefix("./"))
            && let Some(called) = workflows.iter().find(|workflow| workflow.path == callee)
        {
            pull_request_privileges(called, workflows, trigger, false, seen, findings);
        }
    }
    let mut rest = workflow.text.as_str();
    while let Some(at) = rest.find("secrets.") {
        let name: String = rest[at + "secrets.".len()..]
            .chars()
            .take_while(|c| c.is_ascii_alphanumeric() || *c == '_')
            .collect();
        if name != "GITHUB_TOKEN" {
            findings.push(format!(
                "{path}{reached} reads the secret {name} in a workflow pull requests run"
            ));
        }
        rest = &rest[at + "secrets.".len()..];
    }
}

/// The scopes a `permissions` value grants write access to, beyond what a pull
/// request may hold.
fn writes(permissions: &Value) -> Vec<String> {
    match permissions {
        Value::String(all) if all == "write-all" => vec!["every scope".to_string()],
        Value::Mapping(scopes) => scopes
            .iter()
            .filter(|(_, level)| level.as_str() == Some("write"))
            .filter_map(|(scope, _)| scope.as_str())
            .filter(|scope| !PULL_REQUEST_WRITES.contains(scope))
            .map(str::to_string)
            .collect(),
        _ => Vec::new(),
    }
}

/// A required check fails red. Nothing marks a step or a job as allowed to
/// fail, and a scan that only reports is paired with one that gates.
fn soft_failures(workflow: &Workflow, findings: &mut Findings) {
    let path = &workflow.path;
    for (name, job) in workflow.jobs() {
        if soft(&job["continue-on-error"]) {
            findings.push(format!(
                "{path}: job {name} may fail without failing the run (`continue-on-error`)"
            ));
        }
        let steps = steps(job);
        for (index, step) in steps.iter().enumerate() {
            let step_label = step_name(step, index);
            if soft(&step["continue-on-error"]) {
                findings.push(format!(
                    "{path}: job {name}, {step_label} may fail without failing the job \
                     (`continue-on-error`)"
                ));
            }
            for line in script(step).lines() {
                let scanner = [
                    "trivy ",
                    "gitleaks",
                    "cosign ",
                    "gh attestation",
                    "cargo audit",
                    "cargo deny",
                    "npm audit",
                    "--gate",
                ]
                .iter()
                .any(|scanner| line.contains(scanner));
                if scanner && (line.contains("|| true") || line.contains("|| exit 0")) {
                    findings.push(format!(
                        "{path}: job {name}, {step_label} turns a failed check into a pass: {}",
                        line.trim()
                    ));
                }
            }
        }
        let reports = steps
            .iter()
            .any(|step| uses(step, "aquasecurity/trivy-action") && with(step, "exit-code") == "0");
        let gates = steps.iter().any(|step| {
            uses(step, "aquasecurity/trivy-action")
                && with(step, "exit-code") != "0"
                && !with(step, "trivy-config").is_empty()
        });
        if reports && !gates {
            findings.push(format!(
                "{path}: job {name} scans an image but only reports, with no step that fails on \
                 the trivy.yaml policy"
            ));
        }
    }
}

fn soft(value: &Value) -> bool {
    !matches!(value, Value::Null | Value::Bool(false))
}

const BUILD: &str = "docker/build-push-action";
const TRIVY: &str = "aquasecurity/trivy-action";
const SBOM: &str = "anchore/sbom-action";
const PROVENANCE: &str = "actions/attest-build-provenance";
const SBOM_ATTESTATION: &str = "actions/attest-sbom";

fn builds(step: &Value) -> bool {
    uses(step, BUILD) || {
        let script = script(step);
        script.contains("docker build ")
            || script.contains("docker buildx build")
            || script.contains("buildx bake")
    }
}

/// One build per job. A job that pushes does so by digest, and everything after
/// the build names that digest; it verifies before it tags.
fn images(workflow: &Workflow, name: &str, job: &Value, findings: &mut Findings) {
    let path = &workflow.path;
    let steps = steps(job);
    let build_steps: Vec<usize> = (0..steps.len()).filter(|&i| builds(steps[i])).collect();
    if build_steps.len() > 1 {
        findings.push(format!(
            "{path}: job {name} builds an image {} times; what is scanned must be what is \
             published, so build once",
            build_steps.len()
        ));
    }
    let Some(&build) = build_steps.first() else {
        return;
    };
    let step = steps[build];
    let outputs = with(step, "outputs");
    let pushes = step["with"]["push"].as_bool() == Some(true)
        || with(step, "push") == "true"
        || outputs.contains("push=true");
    if !pushes {
        return;
    }
    let Some(id) = step["id"].as_str() else {
        findings.push(format!(
            "{path}: job {name} publishes an image from a build step with no `id`, so nothing \
             after it can name the digest it pushed"
        ));
        return;
    };
    let digest = format!("steps.{id}.outputs.digest");
    if !with(step, "tags").is_empty() {
        findings.push(format!(
            "{path}: job {name} pushes tags while it builds, before the image is scanned, \
             attested and signed; push by digest and tag the verified digest"
        ));
    }
    if !outputs.contains("push-by-digest=true") {
        findings.push(format!(
            "{path}: job {name} does not push by digest (`push-by-digest=true`)"
        ));
    }
    let after: Vec<(usize, &Value)> = steps.iter().copied().enumerate().skip(build + 1).collect();
    digest_only(path, name, &after, &digest, findings);
    publication_order(path, name, &after, findings);
}

/// Every step after a publishing build that scans, describes or attests an
/// image names the digest the build pushed.
fn digest_only(
    path: &str,
    name: &str,
    after: &[(usize, &Value)],
    digest: &str,
    findings: &mut Findings,
) {
    for (index, step) in after {
        let label = step_name(step, *index);
        let target = if uses(step, TRIVY) {
            Some(with(step, "image-ref"))
        } else if uses(step, SBOM) {
            Some(with(step, "image"))
        } else if uses(step, PROVENANCE) || uses(step, SBOM_ATTESTATION) {
            Some(with(step, "subject-digest"))
        } else {
            None
        };
        if let Some(target) = target
            && !target.contains(digest)
        {
            findings.push(format!(
                "{path}: job {name}, {label} reads {target:?} rather than the digest the build \
                 pushed ({digest})"
            ));
        }
    }
}

/// A publishing job scans, attests, signs and verifies, and tags only after
/// the image is scanned, signed and verified.
fn publication_order(path: &str, name: &str, after: &[(usize, &Value)], findings: &mut Findings) {
    let position = |found: &dyn Fn(&Value) -> bool| {
        after
            .iter()
            .find(|(_, step)| found(step))
            .map(|(index, _)| *index)
    };
    let gate = position(&|step| uses(step, TRIVY) && !with(step, "trivy-config").is_empty());
    let provenance = position(&|step| {
        uses(step, PROVENANCE) && step["with"]["push-to-registry"].as_bool() == Some(true)
    });
    let sbom = position(&|step| uses(step, SBOM_ATTESTATION));
    let signature = position(&|step| script(step).contains("cosign sign "));
    let verification = position(&|step| {
        let script = script(step);
        script.contains("cosign verify ")
            && script.contains("gh attestation verify")
            && script.contains("https://slsa.dev/provenance/v1")
            && script.contains("https://cyclonedx.org/bom")
    });
    let tagging = position(&|step| script(step).contains("imagetools create"));
    for (found, missing) in [
        (gate, "a scan that fails on the trivy.yaml policy"),
        (
            provenance,
            "a provenance attestation pushed to the registry",
        ),
        (sbom, "an SBOM attestation"),
        (signature, "a signature"),
        (
            verification,
            "a verification of its signature, provenance and SBOM",
        ),
    ] {
        if found.is_none() {
            findings.push(format!(
                "{path}: job {name} publishes an image without {missing}"
            ));
        }
    }
    if let Some(tagging) = tagging {
        for (before, what) in [
            (gate, "scanned"),
            (signature, "signed"),
            (verification, "verified"),
        ] {
            if before.is_none_or(|before| before > tagging) {
                findings.push(format!(
                    "{path}: job {name} tags the image before it is {what}"
                ));
            }
        }
    }
}

/// Whatever is signed or attested by reference is named by digest: a tag can
/// be pointed at another image after its signature is made.
fn signing(workflow: &Workflow, name: &str, job: &Value, findings: &mut Findings) {
    let path = &workflow.path;
    for (index, step) in steps(job).into_iter().enumerate() {
        let label = step_name(step, index);
        let environment: String = [&step["env"], &job["env"], &workflow.document["env"]]
            .iter()
            .filter_map(|env| env.as_mapping())
            .flatten()
            .filter_map(|(key, value)| Some(format!("{}={}\n", key.as_str()?, value.as_str()?)))
            .collect();
        for line in script(step).lines() {
            let signs = ["cosign sign ", "cosign attest "]
                .iter()
                .any(|command| line.contains(command));
            if !signs {
                continue;
            }
            if !names_digest(line, &environment) {
                findings.push(format!(
                    "{path}: job {name}, {label} signs a reference that is not a digest: {}",
                    line.trim()
                ));
            }
        }
        if (uses(step, PROVENANCE) || uses(step, SBOM_ATTESTATION))
            && with(step, "subject-path").is_empty()
            && !with(step, "subject-digest").contains("outputs.digest")
            && !with(step, "subject-digest").starts_with("sha256:")
        {
            findings.push(format!(
                "{path}: job {name}, {label} attests something other than a digest or a file"
            ));
        }
    }
}

/// Whether a command line names its image by digest, directly or through an
/// environment variable set to one.
fn names_digest(line: &str, environment: &str) -> bool {
    if line.contains("outputs.digest") || line.contains("@sha256:") {
        return true;
    }
    environment.lines().any(|assignment| {
        let Some((variable, value)) = assignment.split_once('=') else {
            return false;
        };
        (value.contains("outputs.digest }}") && value.contains('@') || value.contains("@sha256:"))
            && (line.contains(&format!("${variable}"))
                || line.contains(&format!("${{{variable}}}")))
    })
}

/// Something publishes the images, and the release manifest is written only
/// after every image is verified again.
fn publication(workflows: &[Workflow], findings: &mut Findings) {
    let publishes = workflows.iter().any(|workflow| {
        workflow.jobs().any(|(_, job)| {
            steps(job).iter().any(|step| {
                uses(step, BUILD) && with(step, "outputs").contains("push-by-digest=true")
            })
        })
    });
    if !publishes {
        findings.push(format!(
            "{WORKFLOWS}: no workflow publishes the release images by digest"
        ));
    }
    let mut manifests = 0;
    for workflow in workflows {
        for (name, job) in workflow.jobs() {
            let steps = steps(job);
            let Some(written) = steps
                .iter()
                .position(|step| script(step).contains("release manifest"))
            else {
                continue;
            };
            manifests += 1;
            let verified = steps[..written].iter().any(|step| {
                let script = script(step);
                script.contains("cosign verify ") && script.contains("gh attestation verify")
            });
            if !verified {
                findings.push(format!(
                    "{}: job {name} writes the release manifest without verifying every image's \
                     signature and attestations first",
                    workflow.path
                ));
            }
        }
    }
    if manifests == 0 {
        findings.push(format!(
            "{WORKFLOWS}: no workflow writes the release manifest"
        ));
    }
}

fn secret_scanning(workflows: &[Workflow], findings: &mut Findings) {
    let scans = workflows.iter().any(|workflow| {
        workflow.jobs().any(|(_, job)| {
            let steps = steps(job);
            let whole_history = steps.iter().any(|step| {
                uses(step, "actions/checkout") && step["with"]["fetch-depth"].as_u64() == Some(0)
            });
            whole_history
                && steps
                    .iter()
                    .any(|step| script(step).contains("--gate secrets"))
        })
    });
    if !scans {
        findings.push(format!(
            "{WORKFLOWS}: no CI job runs the secrets gate over the whole Git history \
             (a checkout with `fetch-depth: 0`)"
        ));
    }
}

/// Dependabot proposes newer pins for the actions and for every Dockerfile's
/// base images, so a pin never silently goes stale.
fn dependabot(root: &Path, findings: &mut Findings) {
    let Ok(text) = fs::read_to_string(root.join(DEPENDABOT)) else {
        findings.push(format!(
            "{DEPENDABOT}: missing; nothing proposes newer pins"
        ));
        return;
    };
    let Ok(config) = serde_yaml::from_str::<Value>(&text) else {
        findings.push(format!("{DEPENDABOT}: is not valid YAML"));
        return;
    };
    let updates: Vec<(String, String)> = config["updates"]
        .as_sequence()
        .into_iter()
        .flatten()
        .filter_map(|update| {
            Some((
                update["package-ecosystem"].as_str()?.to_string(),
                update["directory"]
                    .as_str()?
                    .trim_end_matches('/')
                    .to_string(),
            ))
        })
        .collect();
    if !updates
        .iter()
        .any(|(ecosystem, directory)| ecosystem == "github-actions" && directory.is_empty())
    {
        findings.push(format!(
            "{DEPENDABOT}: does not update the pinned GitHub Actions (`github-actions` in `/`)"
        ));
    }
    let Ok(files) = dockerfile_paths(root) else {
        return;
    };
    for file in files {
        let directory = file
            .rsplit_once('/')
            .map_or(String::new(), |(directory, _)| format!("/{directory}"));
        if !updates
            .iter()
            .any(|(ecosystem, configured)| ecosystem == "docker" && *configured == directory)
        {
            findings.push(format!(
                "{DEPENDABOT}: does not update the base images of {file} (`docker` in `{}`)",
                if directory.is_empty() {
                    "/"
                } else {
                    &directory
                }
            ));
        }
    }
}

fn dockerfile_paths(root: &Path) -> Result<Vec<String>, String> {
    Ok(git(
        root,
        &["ls-files", "--cached", "--others", "--exclude-standard"],
    )?
    .lines()
    .filter(|file| {
        let name = file.rsplit('/').next().unwrap_or(file);
        name == "Dockerfile" || name.starts_with("Dockerfile.") || name.ends_with(".Dockerfile")
    })
    .filter(|file| !file.starts_with("docs/superpowers/") && !file.starts_with("planning/"))
    .map(str::to_string)
    .collect())
}

/// Every base image a Dockerfile pulls is pinned by digest. A stage built
/// earlier in the same file is not pulled, and `scratch` is not an image.
fn dockerfiles(root: &Path, findings: &mut Findings) -> Result<(), String> {
    for file in dockerfile_paths(root)? {
        let Ok(text) = fs::read_to_string(root.join(&file)) else {
            continue;
        };
        let mut stages = BTreeSet::new();
        for (index, line) in text.lines().enumerate() {
            let words: Vec<&str> = line.split_whitespace().collect();
            if !words
                .first()
                .is_some_and(|word| word.eq_ignore_ascii_case("FROM"))
            {
                continue;
            }
            let Some(image) = words[1..].iter().find(|word| !word.starts_with("--")) else {
                continue;
            };
            let pulled = !stages.contains(*image) && *image != "scratch";
            if pulled && !image.contains("@sha256:") {
                findings.push(format!(
                    "{file}:{}: FROM {image} is not pinned by digest",
                    index + 1
                ));
            }
            if let Some(position) = words
                .iter()
                .position(|word| word.eq_ignore_ascii_case("AS"))
                && let Some(alias) = words.get(position + 1)
            {
                stages.insert((*alias).to_string());
            }
        }
    }
    Ok(())
}
