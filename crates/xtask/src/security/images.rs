//! `security images`: every third-party image the chart and Compose pull,
//! scanned by digest with Trivy and held to `trivy.yaml`'s thresholds.
//!
//! Capsulet cannot rebuild an upstream image. A finding no newer pin fixes is
//! registered in the exception registry instead: ecosystem `image`, the image
//! repository as `package`, the vulnerability ids as `versions` and scope
//! `upstream-image`, with an owner, an issue and an expiry. A finding nobody
//! registered fails, and so does an entry the scan no longer needs.

use std::{
    collections::BTreeSet,
    fmt::Write as _,
    fs,
    io::Write as _,
    path::Path,
    process::{Command, Stdio},
};

use chrono::Utc;
use serde_json::Value;
use serde_yaml::Value as Yaml;

use super::{Exception, read_json, registry_path, validate_registry};
use crate::release::{dependency_images, repository};

/// The Trivy release the pinned `aquasecurity/trivy-action` installs, by
/// digest, so Capsulet's images and the ones it pulls meet the same scanner.
const TRIVY: &str =
    "aquasec/trivy:0.70.0@sha256:be1190afcb28352bfddc4ddeb71470835d16462af68d310f9f4bca710961a41e";

/// A Docker volume that keeps Trivy's databases between runs.
const CACHE: &str = "capsulet-trivy-cache";

pub(super) const POLICY: &str = "trivy.yaml";

/// One platform of a multi-platform image, so a developer machine and CI scan
/// the same manifest.
const PLATFORM: &str = "linux/amd64";

const REPORTS: &str = ".capsulet/security/dependency-images";

const SEVERITIES: &[&str] = &["UNKNOWN", "LOW", "MEDIUM", "HIGH", "CRITICAL"];

/// The thresholds `trivy.yaml` fails an image on.
pub(super) struct Policy {
    severities: Vec<String>,
    ignore_unfixed: bool,
}

impl Policy {
    /// Read with a YAML parser, from the same file the image workflows hand to
    /// `trivy-action`, so the two cannot apply different thresholds.
    pub(super) fn read(path: &Path) -> Result<Self, String> {
        let text = fs::read_to_string(path)
            .map_err(|error| format!("{}: cannot be read: {error}", path.display()))?;
        let document: Yaml = serde_yaml::from_str(&text)
            .map_err(|error| format!("{POLICY}: is not valid YAML: {error}"))?;
        let severities: Vec<String> = match &document["severity"] {
            Yaml::Sequence(values) => values
                .iter()
                .map(|value| value.as_str().map(str::to_uppercase))
                .collect::<Option<_>>()
                .ok_or(format!("{POLICY}: severity must list severity names"))?,
            Yaml::String(values) => values
                .split(',')
                .map(|value| value.trim().to_uppercase())
                .collect(),
            _ => return Err(format!("{POLICY}: severity is missing")),
        };
        if severities.is_empty()
            || severities
                .iter()
                .any(|severity| !SEVERITIES.contains(&severity.as_str()))
        {
            return Err(format!(
                "{POLICY}: severity {severities:?} must name some of {SEVERITIES:?}"
            ));
        }
        let vulnerability = &document["vulnerability"];
        for (key, _) in vulnerability.as_mapping().into_iter().flatten() {
            if key.as_str() != Some("ignore-unfixed") {
                return Err(format!(
                    "{POLICY}: vulnerability.{} is a threshold the dependency-images gate does \
                     not apply, so it would disagree with the image workflows; teach \
                     crates/xtask/src/security/images.rs to apply it first",
                    key.as_str().unwrap_or("?")
                ));
            }
        }
        let ignore_unfixed = match &vulnerability["ignore-unfixed"] {
            Yaml::Null => false,
            Yaml::Bool(ignore) => *ignore,
            _ => {
                return Err(format!(
                    "{POLICY}: vulnerability.ignore-unfixed must be true or false"
                ));
            }
        };
        Ok(Self {
            severities,
            ignore_unfixed,
        })
    }

    /// Trivy's `ignore-unfixed` keeps only the status `fixed`: a fix exists
    /// upstream of the image, and a rebuild or a newer pin picks it up.
    fn fails(&self, severity: &str, status: &str) -> bool {
        self.severities.iter().any(|failing| failing == severity)
            && (!self.ignore_unfixed || status == "fixed")
    }

    fn describe(&self) -> String {
        format!(
            "a {}{} vulnerability fails",
            self.severities.join(" or "),
            if self.ignore_unfixed {
                " with a fixed version"
            } else {
                ""
            }
        )
    }
}

/// One image's Trivy report.
pub(super) struct Scan {
    pub(super) reference: String,
    pub(super) repository: String,
    pub(super) sources: Vec<String>,
    pub(super) report: Value,
}

/// What the scan found in one image.
struct Row {
    reference: String,
    sources: Vec<String>,
    vulnerabilities: usize,
    failing: usize,
    exceptions: BTreeSet<String>,
}

/// Scans every dependency image with Trivy in Docker, keeps each raw report
/// below `.capsulet/security/dependency-images/`, and holds the findings to
/// the policy and the exception registry.
pub(super) fn scan(root: &Path) -> Result<(), String> {
    let images = dependency_images(root)?;
    let policy = Policy::read(&root.join(POLICY))?;
    // Both databases first, so every image is matched against the same data.
    trivy(&[
        "image",
        "--cache-dir",
        "/cache",
        "--download-db-only",
        "--no-progress",
        "--skip-version-check",
    ])?;
    trivy(&[
        "image",
        "--cache-dir",
        "/cache",
        "--download-java-db-only",
        "--no-progress",
        "--skip-version-check",
    ])?;
    let version: Value = serde_json::from_slice(&trivy(&[
        "version",
        "--cache-dir",
        "/cache",
        "--format",
        "json",
    ])?)
    .map_err(|error| format!("trivy version printed no JSON: {error}"))?;
    let directory = root.join(REPORTS);
    fs::create_dir_all(&directory).map_err(|error| format!("{REPORTS}: {error}"))?;
    let mut scans = Vec::new();
    for image in images {
        println!("[dependency-images] scanning {}", image.reference);
        let output = trivy(&[
            "image",
            "--cache-dir",
            "/cache",
            "--skip-db-update",
            "--skip-java-db-update",
            "--image-src",
            "remote",
            "--platform",
            PLATFORM,
            "--scanners",
            "vuln",
            "--format",
            "json",
            "--exit-code",
            "0",
            "--no-progress",
            "--skip-version-check",
            &image.reference,
        ])?;
        fs::write(
            directory.join(format!("{}.json", file_name(&image.reference))),
            &output,
        )
        .map_err(|error| format!("{REPORTS}: {error}"))?;
        let report = serde_json::from_slice(&output).map_err(|error| {
            format!(
                "invalid Trivy report for {}: not JSON: {error}",
                image.reference
            )
        })?;
        scans.push(Scan {
            reference: image.reference,
            repository: image.repository,
            sources: image.sources,
            report,
        });
    }
    let today = Utc::now().date_naive().to_string();
    let result = check(&scans, &read_json(&registry_path(root))?, &policy, &today);
    summarise(&version, &policy, &result);
    result.map(|_| ())
}

/// Runs the pinned Trivy with its database volume, and returns what it printed.
/// Its log goes to the gate's log as it runs.
fn trivy(arguments: &[&str]) -> Result<Vec<u8>, String> {
    let output = Command::new("docker")
        .args(["run", "--rm", "-v", &format!("{CACHE}:/cache"), TRIVY])
        .args(arguments)
        .stderr(Stdio::inherit())
        .output()
        .map_err(|error| format!("could not run Trivy in Docker: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "Trivy did not complete `{}`",
            arguments
                .iter()
                .find(|argument| !argument.starts_with('-'))
                .map_or("trivy", |command| command)
        ));
    }
    Ok(output.stdout)
}

/// A report's file name: the reference with anything but name characters
/// replaced.
fn file_name(reference: &str) -> String {
    reference
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '.' | '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

/// `security check-images`: the reports in a directory, each named by its
/// `ArtifactName`, held to a registry, the way `security images` holds a scan.
pub(super) fn check_reports(
    reports: &Path,
    registry: &Path,
    policy: &Path,
    today: &str,
) -> Result<(), String> {
    let mut paths: Vec<_> = fs::read_dir(reports)
        .map_err(|error| format!("{}: {error}", reports.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    paths.sort();
    let mut scans = Vec::new();
    for path in paths {
        let report = read_json(&path)?;
        let reference = report["ArtifactName"]
            .as_str()
            .ok_or_else(|| format!("invalid Trivy report {}: no ArtifactName", path.display()))?
            .to_string();
        scans.push(Scan {
            repository: repository(&reference).to_string(),
            reference,
            sources: Vec::new(),
            report,
        });
    }
    check(&scans, &read_json(registry)?, &Policy::read(policy)?, today).map(|_| ())
}

/// Every vulnerability that fails the policy must be listed, for its image's
/// repository, by an unexpired `image` exception. Every vulnerability an
/// exception lists must still fail the policy in an image the chart or Compose
/// pulls: an entry that covers nothing is stale and fails too.
fn check(
    scans: &[Scan],
    registry: &Value,
    policy: &Policy,
    today: &str,
) -> Result<Vec<Row>, String> {
    let registry = validate_registry(registry, today)?;
    let exceptions: Vec<&Exception> = registry
        .exceptions
        .iter()
        .filter(|exception| exception.ecosystem == "image")
        .collect();
    let mut rows = Vec::new();
    let mut matched = BTreeSet::new();
    let mut unexcepted = Vec::new();
    for scan in scans {
        let mut row = Row {
            reference: scan.reference.clone(),
            sources: scan.sources.clone(),
            vulnerabilities: 0,
            failing: 0,
            exceptions: BTreeSet::new(),
        };
        for finding in findings(scan)? {
            row.vulnerabilities += 1;
            if !policy.fails(finding.severity, finding.status) {
                continue;
            }
            row.failing += 1;
            let place = format!("{} in {}", finding.describe(), scan.reference);
            match exceptions.iter().find(|exception| {
                exception.package == scan.repository
                    && exception.versions.iter().any(|id| id == finding.id)
            }) {
                Some(exception) => {
                    matched.insert((scan.repository.as_str(), finding.id));
                    row.exceptions.insert(exception.id.clone());
                    println!(
                        "EXCEPTION {}: {place}, owner {}, expires {}, issue {}",
                        exception.id, exception.owner, exception.expires, exception.issue
                    );
                }
                None => unexcepted.push(place),
            }
        }
        rows.push(row);
    }
    if !unexcepted.is_empty() {
        return Err(format!(
            "{} finding(s) in dependency images fail {POLICY} outside the exception registry:\n  \
             - {}\nMove the pin to a release that fixes it, in charts/capsulet/values.yaml and \
             compose.yaml alike. If no release does, register it in \
             docs/contracts/security-exceptions.json: ecosystem `image`, the image repository as \
             `package`, the vulnerability ids as `versions`, scope `upstream-image`, an owner, an \
             issue and an expiry.",
            unexcepted.len(),
            unexcepted.join("\n  - ")
        ));
    }
    let repositories: BTreeSet<&str> = scans.iter().map(|scan| scan.repository.as_str()).collect();
    for exception in &exceptions {
        if !repositories.contains(exception.package.as_str()) {
            return Err(format!(
                "stale image exception {}: {} is not an image the chart or Compose pulls",
                exception.id, exception.package
            ));
        }
        for id in &exception.versions {
            if !matched.contains(&(exception.package.as_str(), id.as_str())) {
                return Err(format!(
                    "stale image exception {}: {id} no longer fails {POLICY} in {}; remove it",
                    exception.id, exception.package
                ));
            }
        }
    }
    let failing: usize = rows.iter().map(|row| row.failing).sum();
    if failing == 0 {
        println!(
            "dependency image policy passed: no image has a finding that fails {POLICY} ({} \
             image(s) scanned)",
            rows.len()
        );
    } else {
        println!(
            "dependency image policy passed: {failing} finding(s) fail {POLICY} across {} \
             image(s), every one a registered, unexpired exception",
            rows.len()
        );
    }
    Ok(rows)
}

/// One vulnerability in a report.
struct Finding<'a> {
    id: &'a str,
    severity: &'a str,
    status: &'a str,
    package: &'a str,
    installed: &'a str,
    fixed: &'a str,
    target: &'a str,
}

impl Finding<'_> {
    fn describe(&self) -> String {
        let fixed = if self.fixed.is_empty() {
            "no fixed version".to_string()
        } else {
            format!("fixed in {}", self.fixed)
        };
        format!(
            "{} ({}) in {} {}, {fixed}, at {}",
            self.id, self.severity, self.package, self.installed, self.target
        )
    }
}

/// Every vulnerability in a report, which must be the Trivy JSON report of
/// the image it is said to be.
fn findings(scan: &Scan) -> Result<Vec<Finding<'_>>, String> {
    let report = &scan.report;
    let invalid = |problem: &str| format!("invalid Trivy report for {}: {problem}", scan.reference);
    if report["SchemaVersion"] != 2 {
        return Err(invalid("SchemaVersion is not 2"));
    }
    if report["ArtifactName"].as_str() != Some(scan.reference.as_str()) {
        return Err(invalid("it is the report of another image"));
    }
    if report["ArtifactType"] != "container_image" {
        return Err(invalid("it is not the report of a container image"));
    }
    let results = match &report["Results"] {
        Value::Null => &[][..],
        Value::Array(results) => results.as_slice(),
        _ => return Err(invalid("Results is not a list")),
    };
    let mut found = Vec::new();
    for result in results {
        let vulnerabilities = match &result["Vulnerabilities"] {
            Value::Null => &[][..],
            Value::Array(vulnerabilities) => vulnerabilities.as_slice(),
            _ => return Err(invalid("Vulnerabilities is not a list")),
        };
        for vulnerability in vulnerabilities {
            let field = |name: &str| vulnerability[name].as_str().unwrap_or_default();
            let id = vulnerability["VulnerabilityID"]
                .as_str()
                .filter(|id| !id.is_empty())
                .ok_or_else(|| invalid("a vulnerability has no VulnerabilityID"))?;
            let severity = field("Severity");
            if !SEVERITIES.contains(&severity) {
                return Err(invalid(&format!(
                    "{id} has an unknown severity {severity:?}"
                )));
            }
            found.push(Finding {
                id,
                severity,
                // Trivy leaves the status out when it is unknown.
                status: field("Status"),
                package: field("PkgName"),
                installed: field("InstalledVersion"),
                fixed: field("FixedVersion"),
                target: vulnerability["PkgPath"]
                    .as_str()
                    .or_else(|| result["Target"].as_str())
                    .unwrap_or("an unnamed target"),
            });
        }
    }
    Ok(found)
}

/// The scanner, its database, the policy and what each image held, in the job
/// summary, whether the gate passed or not.
fn summarise(version: &Value, policy: &Policy, result: &Result<Vec<Row>, String>) {
    let Ok(path) = std::env::var("GITHUB_STEP_SUMMARY") else {
        return;
    };
    let pinned = TRIVY.split('@').next().unwrap_or(TRIVY);
    let mut lines = format!(
        "### Dependency images\n\n| | |\n| --- | --- |\n\
         | Scanner | Trivy {}, `{pinned}` by digest, {PLATFORM} |\n\
         | Vulnerability database | updated {} |\n\
         | Policy | `{POLICY}`: {} unless it is a registered, unexpired `image` exception in \
         `docs/contracts/security-exceptions.json` |\n",
        version["Version"].as_str().unwrap_or("of unknown version"),
        version["VulnerabilityDB"]["UpdatedAt"]
            .as_str()
            .unwrap_or("at an unknown time"),
        policy.describe()
    );
    match result {
        Ok(rows) => {
            lines.push_str("| Result | passed |\n\n");
            lines.push_str(
                "| Image | Pinned in | Vulnerabilities | Failing the policy | Registered as |\n\
                 | --- | --- | --- | --- | --- |\n",
            );
            for row in rows {
                let _ = writeln!(
                    lines,
                    "| `{}` | {} | {} | {} | {} |",
                    row.reference,
                    row.sources.join("<br>"),
                    row.vulnerabilities,
                    row.failing,
                    if row.exceptions.is_empty() {
                        "—".to_string()
                    } else {
                        row.exceptions
                            .iter()
                            .map(|id| format!("`{id}`"))
                            .collect::<Vec<_>>()
                            .join(", ")
                    }
                );
            }
        }
        Err(error) => {
            lines.push_str("| Result | **failed** |\n\n```text\n");
            lines.push_str(error);
            lines.push_str("\n```\n");
        }
    }
    if let Ok(mut file) = fs::OpenOptions::new().append(true).open(path) {
        let _ = file.write_all(lines.as_bytes());
    }
}
