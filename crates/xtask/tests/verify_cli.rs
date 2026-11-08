use std::{path::Path, process::Command};

fn xtask() -> Command {
    Command::new(env!("CARGO_BIN_EXE_capsulet-xtask"))
}

fn output(arguments: &[&str]) -> std::process::Output {
    xtask().args(arguments).output().expect("xtask runs")
}

fn stderr(output: &std::process::Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}

#[test]
fn rejects_unknown_profiles() {
    let result = output(&["verify", "--profile", "mystery"]);
    assert!(!result.status.success());
    assert!(stderr(&result).contains("unknown verification profile"));
}

#[test]
fn reports_absent_prerequisites() {
    let result = xtask()
        .args(["verify", "--gate", "format"])
        .env("CAPSULET_VERIFY_FORCE_MISSING", "cargo")
        .output()
        .expect("xtask runs");
    assert!(!result.status.success());
    assert!(stderr(&result).contains("missing prerequisite: cargo"));
}

#[test]
fn simultaneous_verification_runs_do_not_share_artifact_logs() {
    let workers = (0..8)
        .map(|_| {
            std::thread::spawn(|| {
                let result = xtask()
                    .args(["verify", "--gate", "format"])
                    .env("CAPSULET_VERIFY_TEST_COMMAND", "fail")
                    .output()
                    .expect("verifier");
                assert!(!result.status.success());
                let diagnostic = stderr(&result);
                diagnostic
                    .split_once("(log: ")
                    .expect("log location")
                    .1
                    .split_once(')')
                    .expect("log suffix")
                    .0
                    .to_string()
            })
        })
        .collect::<Vec<_>>();
    let paths = workers
        .into_iter()
        .map(|worker| worker.join().expect("worker"))
        .collect::<std::collections::BTreeSet<_>>();
    assert_eq!(
        paths.len(),
        8,
        "each verifier execution must retain its own evidence"
    );
}

#[test]
fn propagates_failed_children() {
    let result = xtask()
        .args(["verify", "--gate", "format"])
        .env("CAPSULET_VERIFY_TEST_COMMAND", "fail")
        .output()
        .expect("xtask runs");
    assert!(!result.status.success());
    assert!(stderr(&result).contains("format: failed"));
}

#[test]
fn terminates_timed_out_children() {
    let result = xtask()
        .args(["verify", "--gate", "format"])
        .env("CAPSULET_VERIFY_TEST_COMMAND", "sleep")
        .env("CAPSULET_VERIFY_TEST_TIMEOUT_MS", "20")
        .output()
        .expect("xtask runs");
    assert!(!result.status.success());
    assert!(stderr(&result).contains("format: timed out"));
}

#[test]
fn runs_cleanup_after_failure() {
    let directory = tempfile::tempdir().expect("temp directory");
    let marker = directory.path().join("cleanup.marker");
    let result = xtask()
        .args(["verify", "--gate", "format"])
        .env("CAPSULET_VERIFY_TEST_COMMAND", "fail")
        .env("CAPSULET_VERIFY_TEST_CLEANUP_MARKER", &marker)
        .output()
        .expect("xtask runs");
    assert!(!result.status.success());
    assert!(Path::new(&marker).is_file(), "cleanup hook did not run");
}

#[test]
fn reports_cleanup_failure() {
    let result = xtask()
        .args(["verify", "--gate", "format"])
        .env("CAPSULET_VERIFY_TEST_COMMAND", "fail")
        .env("CAPSULET_VERIFY_TEST_CLEANUP_FAIL", "1")
        .output()
        .expect("xtask runs");
    assert!(!result.status.success());
    assert!(stderr(&result).contains("cleanup failed"));
}

/// What every test binary in a gate reported reaches the CI job summary.
#[test]
fn a_gate_publishes_what_its_test_binaries_reported() {
    let directory = tempfile::tempdir().expect("temp directory");
    let summary = directory.path().join("summary.md");
    let result = xtask()
        .args(["verify", "--gate", "format"])
        .env("CAPSULET_VERIFY_TEST_COMMAND", "report")
        .env("GITHUB_STEP_SUMMARY", &summary)
        .output()
        .expect("xtask runs");
    assert!(result.status.success(), "{}", stderr(&result));
    assert!(
        String::from_utf8_lossy(&result.stdout).contains("(1 test binaries, 3 tests passed)"),
        "{}",
        String::from_utf8_lossy(&result.stdout)
    );
    let table = std::fs::read_to_string(&summary).expect("the job summary was written");
    assert!(
        table.contains("| `format` |") && table.contains("| `tests/fake.rs` | 3 | 0 | 1 | 0 |"),
        "{table}"
    );
}

#[test]
fn cannot_skip_a_required_gate() {
    let result = output(&["verify", "--profile", "full", "--skip", "format"]);
    assert!(!result.status.success());
    assert!(stderr(&result).contains("cannot skip required gate: format"));
}

#[test]
fn required_verifier_self_tests_use_an_independent_build_directory() {
    let result = output(&["verify", "--list", "--format", "json"]);
    assert!(result.status.success());
    let graph: serde_json::Value = serde_json::from_slice(&result.stdout).expect("gate graph");
    let gate = graph
        .as_array()
        .expect("gates")
        .iter()
        .find(|gate| gate["name"] == "verifier")
        .expect("both profiles must execute the verifier's own regression tests");
    assert_eq!(gate["profiles"], serde_json::json!(["fast", "full"]));
    let command = &gate["commands"][0];
    assert_eq!(command["program"], "cargo");
    assert_eq!(
        command["arguments"],
        serde_json::json!([
            "test",
            "-p",
            "capsulet-xtask",
            "--locked",
            "--target-dir",
            ".capsulet/verify/xtask-tests"
        ])
    );
}

#[test]
fn lists_the_executable_gate_graph_as_json() {
    let result = output(&["verify", "--list", "--format", "json"]);
    assert!(result.status.success(), "{}", stderr(&result));
    let graph: serde_json::Value = serde_json::from_slice(&result.stdout).expect("JSON gate graph");
    for gate in [
        "format",
        "lint",
        "unit",
        "api-contracts",
        "authorization",
        "sdk",
        "dashboard",
        "postgres",
        "migrations",
        "security",
        "compose",
        "helm",
        "kind",
    ] {
        assert!(
            graph
                .as_array()
                .expect("gate list")
                .iter()
                .any(|item| item["name"] == gate)
        );
    }
}

/// The Compose stack is torn down by the gate's teardown, which runs after a
/// failure, and not by a last step, which a failure skips.
#[test]
fn the_compose_stack_is_torn_down_even_when_the_smoke_fails() {
    let result = output(&["verify", "--list", "--format", "json"]);
    assert!(result.status.success(), "{}", stderr(&result));
    let graph: serde_json::Value = serde_json::from_slice(&result.stdout).expect("gate graph");
    let compose = graph
        .as_array()
        .expect("gates")
        .iter()
        .find(|gate| gate["name"] == "compose")
        .expect("compose gate");
    let down = serde_json::json!(["compose", "down"]);
    assert!(
        compose["teardown"]
            .as_array()
            .expect("teardown")
            .iter()
            .any(|step| step["program"] == "docker" && step["arguments"] == down),
        "{compose}"
    );
    assert!(
        !compose["commands"]
            .as_array()
            .expect("commands")
            .iter()
            .any(|step| step["arguments"] == down),
        "a `down` among the steps is skipped by the failure it should clean up after"
    );
}

/// A prerequisite probe that guesses `--version` for every tool reports
/// installed tools as missing. `helm` and `kubectl` reject that flag outright,
/// so the doctor would tell an operator to install what they already have and
/// the full profile would stop at a gate that could have run.
#[test]
fn version_probes_use_the_flag_each_tool_actually_accepts() {
    let output = output(&["verify", "doctor"]);
    let report = String::from_utf8_lossy(&output.stdout).into_owned() + &stderr(&output);

    for tool in ["helm", "kubectl"] {
        if which(tool) {
            // `old` is a fair answer for an installed release below the
            // supported minimum; `missing` never is.
            assert!(
                report.contains(&format!("ok      {tool}"))
                    || report.contains(&format!("old     {tool}")),
                "`{tool}` is installed, so the doctor must not report it missing:
{report}"
            );
        }
    }
}

/// The doctor holds each tool to the minimum the documentation supports, and
/// says so for every tool it reports.
#[test]
fn the_doctor_states_the_minimum_it_holds_each_tool_to() {
    let output = output(&["verify", "doctor"]);
    let report = String::from_utf8_lossy(&output.stdout).into_owned();
    for (tool, minimum) in [
        ("cargo", "1.96"),
        ("node", "20"),
        ("python", "3.12"),
        ("helm", "3.18"),
        ("kind", "0.30"),
    ] {
        let line = report
            .lines()
            .find(|line| line.split_whitespace().nth(1) == Some(tool))
            .unwrap_or_else(|| panic!("no line for {tool}:\n{report}"));
        assert!(
            line.starts_with("missing") || line.contains(&format!("(needs {minimum} or newer)")),
            "{line}"
        );
    }
}

/// Whether a tool is on PATH, checked without assuming any version flag.
fn which(tool: &str) -> bool {
    let finder = if cfg!(windows) { "where" } else { "which" };
    Command::new(finder)
        .arg(tool)
        .output()
        .is_ok_and(|found| found.status.success())
}

/// A second version of a crate is a reviewed decision.
///
/// `cargo deny check` fails on a duplicate nobody listed. This holds the list
/// itself to account: each entry gives a reason, names a version the lockfile
/// still has, and is an older version of a crate that really is duplicated, so
/// the newest version is never the one waved through and a stale entry cannot
/// quietly cover whatever version arrives next.
#[test]
fn every_skipped_duplicate_is_reviewed_and_still_in_the_lockfile() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let deny = std::fs::read_to_string(root.join("deny.toml")).expect("deny.toml");
    assert!(
        deny.lines()
            .any(|line| line.trim() == r#"multiple-versions = "deny""#),
        "an unreviewed duplicate must fail the gate, not warn"
    );
    let lockfile = std::fs::read_to_string(root.join("Cargo.lock")).expect("Cargo.lock");
    let versions = registry_versions(&lockfile);

    let skips = deny
        .lines()
        .filter_map(|line| line.trim().strip_prefix("{ crate = \""))
        .collect::<Vec<_>>();
    assert!(
        !skips.is_empty(),
        "no skip entries were read from deny.toml"
    );
    for entry in skips {
        let (spec, rest) = entry.split_once('"').expect("crate spec");
        let (name, version) = spec
            .split_once('@')
            .unwrap_or_else(|| panic!("`{spec}` must name an exact version"));
        let reason = rest
            .split_once("reason = \"")
            .and_then(|(_, reason)| reason.split_once('"'))
            .map_or("", |(reason, _)| reason);
        assert!(
            reason.len() >= 20,
            "`{spec}` is skipped without a reason that says who pulls it in"
        );
        let known = versions
            .get(name)
            .unwrap_or_else(|| panic!("`{spec}` is skipped but `{name}` is not in Cargo.lock"));
        assert!(
            known.iter().any(|candidate| candidate == version),
            "`{spec}` is skipped but Cargo.lock has {name} {known:?}"
        );
        assert!(
            known.len() > 1,
            "`{spec}` is skipped but {name} is no longer duplicated"
        );
        let newest = known
            .iter()
            .max_by_key(|candidate| numeric(candidate))
            .expect("a version");
        assert_ne!(
            newest, version,
            "`{spec}` skips the newest {name}; skip the older versions instead"
        );
    }
}

/// Every version of every registry crate in a lockfile.
fn registry_versions(lockfile: &str) -> std::collections::BTreeMap<String, Vec<String>> {
    let mut versions = std::collections::BTreeMap::<String, Vec<String>>::new();
    for package in lockfile.split("[[package]]").skip(1) {
        let field = |key: &str| {
            package
                .lines()
                .find_map(|line| line.strip_prefix(&format!("{key} = \"")))
                .and_then(|value| value.strip_suffix('"'))
        };
        if let (Some(name), Some(version), Some(_)) =
            (field("name"), field("version"), field("source"))
        {
            versions
                .entry(name.to_string())
                .or_default()
                .push(version.to_string());
        }
    }
    versions
}

/// A version's numeric components, for ordering releases.
fn numeric(version: &str) -> Vec<u64> {
    version
        .split(['.', '-', '+'])
        .map_while(|part| part.parse().ok())
        .collect()
}

/// Security exceptions must be owned and must expire.
///
/// An advisory silenced in `.cargo/audit.toml` with no entry in the exceptions
/// document is an exception nobody owns; one whose review date has passed is
/// permanent by accident. Both fail here rather than in an audit six months
/// from now.
#[test]
fn every_silenced_advisory_is_owned_and_unexpired() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    // Two tools silence advisories, so both configurations are checked. An
    // exception documented for one and forgotten in the other is how a gate
    // ends up disagreeing with itself.
    let audit = std::fs::read_to_string(root.join(".cargo/audit.toml")).unwrap_or_default();
    let deny = std::fs::read_to_string(root.join("deny.toml")).unwrap_or_default();
    let document = std::fs::read_to_string(root.join("docs/contracts/security-exceptions.md"))
        .expect("the exceptions document is readable");
    let registry: serde_json::Value = serde_json::from_str(
        &std::fs::read_to_string(root.join("docs/contracts/security-exceptions.json"))
            .expect("exception registry"),
    )
    .expect("registry JSON");
    let mut registered = registry["exceptions"]
        .as_array()
        .expect("exceptions")
        .iter()
        .filter(|entry| entry["ecosystem"] == "rust")
        .map(|entry| entry["id"].as_str().expect("advisory id").to_string())
        .collect::<Vec<_>>();
    registered.sort();
    let mut ignored = silenced(&audit);
    ignored.sort();
    assert_eq!(
        registered, ignored,
        "Rust tool ignores must exactly match the owned registry"
    );

    let mut advisories = silenced(&audit);
    advisories.extend(silenced(&deny));
    advisories.sort();
    advisories.dedup();
    assert_eq!(
        silenced(&audit),
        silenced(&deny),
        "cargo-audit and cargo-deny must silence the same advisories"
    );

    for advisory in advisories {
        let row = document
            .lines()
            .find(|line| line.contains(&format!("`{advisory}`")) && line.starts_with('|'))
            .unwrap_or_else(|| {
                panic!("`{advisory}` is silenced but not documented as an owned exception")
            });

        let review = review_date(row)
            .unwrap_or_else(|| panic!("the exception for `{advisory}` records no review date"));
        assert!(
            review.as_str() > &today(),
            "the exception for `{advisory}` was due for review on {review}"
        );
    }
}

/// The container scanning policy is an exception too, and expires like one.
///
/// `ignore-unfixed` narrows what fails the build. That is defensible while the
/// unfixed set is genuinely unfixable, and indefensible once it is not, so it
/// carries an owner and a review date in the same document as every silenced
/// advisory — and this fails when the policy is set without one.
#[test]
fn the_container_scan_policy_is_owned_and_unexpired() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let policy = std::fs::read_to_string(root.join("trivy.yaml"))
        .expect("the container scanning policy is readable");
    if !policy.contains("ignore-unfixed: true") {
        return;
    }

    let document = std::fs::read_to_string(root.join("docs/contracts/security-exceptions.md"))
        .expect("the exceptions document is readable");
    let row = document
        .lines()
        .find(|line| line.starts_with('|') && line.contains("`ignore-unfixed` in `trivy.yaml`"))
        .expect("`ignore-unfixed` is set but not documented as an owned exception");

    let review = review_date(row).expect("the container scan exception records no review date");
    assert!(
        review.as_str() > &today(),
        "the container scan exception was due for review on {review}"
    );
}

/// Advisory identifiers listed in the audit configuration's ignore list.
fn silenced(audit: &str) -> Vec<String> {
    audit
        .lines()
        .filter(|line| !line.trim_start().starts_with('#'))
        .flat_map(|line| {
            line.match_indices("RUSTSEC-")
                .map(|(at, _)| {
                    line[at..]
                        .chars()
                        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
                        .collect::<String>()
                })
                .collect::<Vec<_>>()
        })
        .collect()
}

/// The `YYYY-MM-DD` cell of a table row.
fn review_date(row: &str) -> Option<String> {
    row.split('|')
        .map(str::trim)
        .find(|cell| {
            cell.len() == 10
                && cell.as_bytes()[4] == b'-'
                && cell.as_bytes()[7] == b'-'
                && cell.chars().filter(char::is_ascii_digit).count() == 8
        })
        .map(str::to_string)
}

/// Today, as `YYYY-MM-DD`, without pulling in a date library.
fn today() -> String {
    let output = Command::new(if cfg!(windows) { "powershell" } else { "date" })
        .args(if cfg!(windows) {
            vec![
                "-NoProfile",
                "-Command",
                "(Get-Date).ToString('yyyy-MM-dd')",
            ]
        } else {
            vec!["+%Y-%m-%d"]
        })
        .output()
        .expect("the date is readable");
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}
