use std::{
    collections::BTreeSet,
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use chrono::{NaiveDate, Utc};
use serde::Deserialize;
use serde_json::Value;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Registry {
    schema_version: u32,
    exceptions: Vec<Exception>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Exception {
    id: String,
    ecosystem: String,
    package: String,
    versions: Vec<String>,
    scope: String,
    owner: String,
    rationale: String,
    compensating_control: String,
    issue: String,
    expires: String,
}

pub(crate) fn run(arguments: &[String]) -> Result<(), String> {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    if arguments == ["npm"] {
        return scan_npm(&root);
    }
    if arguments
        .first()
        .is_none_or(|command| command != "check-npm")
    {
        return Err("usage: security npm | security check-npm --report <json> --registry <json> --lockfile <json> [--today YYYY-MM-DD]".to_string());
    }
    let report = option(arguments, "--report")?;
    let registry = option(arguments, "--registry")?;
    let lockfile = option(arguments, "--lockfile")?;
    let today = arguments
        .windows(2)
        .find(|pair| pair[0] == "--today")
        .map_or_else(
            || Utc::now().date_naive().to_string(),
            |pair| pair[1].clone(),
        );
    check_npm(
        &read_json(Path::new(report))?,
        &read_json(Path::new(registry))?,
        &read_json(Path::new(lockfile))?,
        &today,
    )
}

fn option<'a>(arguments: &'a [String], name: &str) -> Result<&'a str, String> {
    arguments
        .windows(2)
        .find(|pair| pair[0] == name)
        .map(|pair| pair[1].as_str())
        .ok_or_else(|| format!("{name} is required"))
}

fn read_json(path: &Path) -> Result<Value, String> {
    serde_json::from_slice(&fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?)
        .map_err(|error| format!("invalid JSON in {}: {error}", path.display()))
}

fn scan_npm(root: &Path) -> Result<(), String> {
    let program = if cfg!(windows) { "npm.cmd" } else { "npm" };
    let output = Command::new(program)
        .args(["audit", "--json"])
        .current_dir(root.join("dashboard"))
        .output()
        .map_err(|error| format!("npm audit could not run: {error}"))?;
    let directory = root.join(".capsulet/security");
    fs::create_dir_all(&directory).map_err(|error| error.to_string())?;
    fs::write(directory.join("npm-audit.json"), &output.stdout)
        .map_err(|error| error.to_string())?;
    if !matches!(output.status.code(), Some(0 | 1)) {
        return Err(format!(
            "npm audit failed: {}",
            String::from_utf8_lossy(&output.stderr)
        ));
    }
    let report = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("invalid npm audit: {error}"))?;
    check_npm(
        &report,
        &read_json(&registry_path(root))?,
        &read_json(&root.join("dashboard/package-lock.json"))?,
        &Utc::now().date_naive().to_string(),
    )
}

fn registry_path(root: &Path) -> PathBuf {
    root.join("docs/contracts/security-exceptions.json")
}

fn validate_registry(value: &Value, today: &str) -> Result<Registry, String> {
    let registry: Registry = serde_json::from_value(value.clone())
        .map_err(|error| format!("invalid exception registry: {error}"))?;
    if registry.schema_version != 1 {
        return Err("unsupported exception registry schema_version".to_string());
    }
    let today = NaiveDate::parse_from_str(today, "%Y-%m-%d").map_err(|error| error.to_string())?;
    let mut seen = BTreeSet::new();
    for exception in &registry.exceptions {
        if !seen.insert((&exception.ecosystem, &exception.id)) {
            return Err(format!("duplicate exception {}", exception.id));
        }
        let expiry = NaiveDate::parse_from_str(&exception.expires, "%Y-%m-%d")
            .map_err(|error| format!("invalid expiry: {error}"))?;
        if expiry <= today {
            return Err(format!(
                "expired exception {} on {}",
                exception.id, exception.expires
            ));
        }
        if [
            &exception.id,
            &exception.package,
            &exception.owner,
            &exception.rationale,
            &exception.compensating_control,
        ]
        .iter()
        .any(|value| value.trim().is_empty())
            || exception.versions.is_empty()
            || exception
                .versions
                .iter()
                .any(|version| version.trim().is_empty())
            || !exception
                .issue
                .starts_with("https://github.com/mohripan/capsulet/issues/")
            || !matches!(exception.ecosystem.as_str(), "npm" | "rust")
            || !matches!(
                exception.scope.as_str(),
                "development-only" | "uncompiled-optional" | "unmaintained-macro"
            )
        {
            return Err(format!("invalid or unowned exception {}", exception.id));
        }
    }
    Ok(registry)
}

fn check_npm(report: &Value, registry: &Value, lock: &Value, today: &str) -> Result<(), String> {
    let vulnerabilities = report
        .get("vulnerabilities")
        .and_then(Value::as_object)
        .filter(|_| report["auditReportVersion"] == 2 && report.get("error").is_none())
        .ok_or("invalid npm audit: report version/vulnerabilities missing or scanner error")?;
    let registry = validate_registry(registry, today)?;
    for (name, finding) in vulnerabilities {
        if !matches!(
            finding["severity"].as_str(),
            Some("info" | "low" | "moderate" | "high" | "critical")
        ) {
            return Err(format!("invalid npm audit: unknown severity for {name}"));
        }
        if !matches!(finding["severity"].as_str(), Some("high" | "critical")) {
            continue;
        }
        let mut visited = BTreeSet::new();
        check_finding(name, vulnerabilities, &registry, lock, &mut visited)?;
    }
    println!("npm policy passed; raw scanner output retains all findings");
    Ok(())
}

fn check_finding<'a>(
    name: &'a str,
    findings: &'a serde_json::Map<String, Value>,
    registry: &Registry,
    lock: &Value,
    visiting: &mut BTreeSet<&'a str>,
) -> Result<(), String> {
    if !visiting.insert(name) {
        return Err(format!(
            "invalid npm audit: cyclic advisory chain at {name}"
        ));
    }
    let finding = findings
        .get(name)
        .ok_or_else(|| format!("invalid npm audit: absent dependency {name}"))?;
    if finding["severity"] == "critical" {
        return Err(format!(
            "critical npm finding in {name}; no exception permitted"
        ));
    }
    let via = finding["via"]
        .as_array()
        .filter(|via| !via.is_empty())
        .ok_or("invalid npm audit: empty advisory chain")?;
    for cause in via {
        if let Some(dependency) = cause.as_str() {
            check_finding(dependency, findings, registry, lock, visiting)?;
        } else {
            check_advisory(name, finding, cause, registry, lock)?;
        }
    }
    // A development-only waiver may not propagate through a runtime parent.
    let nodes = finding["nodes"]
        .as_array()
        .filter(|nodes| !nodes.is_empty())
        .ok_or("invalid npm audit: package locations missing")?;
    for node in nodes {
        let path = node.as_str().ok_or("invalid npm audit: package location")?;
        if lock["packages"][path]["dev"] != true {
            return Err(format!(
                "development-only exception cannot cover runtime dependency {name}"
            ));
        }
    }
    visiting.remove(name);
    Ok(())
}

fn check_advisory(
    name: &str,
    finding: &Value,
    advisory: &Value,
    registry: &Registry,
    lock: &Value,
) -> Result<(), String> {
    let url = advisory["url"]
        .as_str()
        .ok_or("invalid npm audit: advisory URL missing")?;
    let id = url
        .strip_prefix("https://github.com/advisories/")
        .ok_or("invalid npm audit: unknown advisory URL")?;
    let exception = registry
        .exceptions
        .iter()
        .find(|exception| {
            exception.ecosystem == "npm" && exception.package == name && exception.id == id
        })
        .ok_or_else(|| format!("unexcepted high npm advisory {id} in {name}"))?;
    if exception.scope != "development-only" || advisory["severity"] == "critical" {
        return Err(format!(
            "critical or non-development-only exception for {id}"
        ));
    }
    let nodes = finding["nodes"]
        .as_array()
        .filter(|nodes| !nodes.is_empty())
        .ok_or("invalid npm audit: package locations missing")?;
    for node in nodes {
        let path = node.as_str().ok_or("invalid npm audit: package location")?;
        let package = &lock["packages"][path];
        let version = package["version"]
            .as_str()
            .ok_or("invalid npm lockfile: package version missing")?;
        if package["dev"] != true || !exception.versions.iter().any(|allowed| allowed == version) {
            return Err(format!(
                "development-only exception {id} cannot cover production or unreviewed {name}@{version}"
            ));
        }
    }
    println!(
        "EXCEPTION {id}: {name}, owner {}, expires {}, issue {}",
        exception.owner, exception.expires, exception.issue
    );
    Ok(())
}
