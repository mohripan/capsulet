//! Purity, enforced rather than promised.
//!
//! Offline replay is only meaningful if the crate that decides a verdict cannot
//! reach a database, a network, a model, a clock, or a random number generator.
//! Reviewers cannot see a transitive dependency, so this test looks for one.

use std::process::Command;

/// Crate names that would make a verdict depend on something a replayer cannot
/// reproduce. Matched against the resolved dependency closure, so an indirect
/// pull-in fails just as loudly as a direct one.
const FORBIDDEN: &[&str] = &[
    "axum",
    "capsulet-api",
    "capsulet-postgres",
    "capsulet-storage",
    "chrono",
    "getrandom",
    "hyper",
    "rand",
    "rand_core",
    "reqwest",
    "sqlx",
    "sqlx-core",
    "tokio",
    "ureq",
];

#[test]
fn the_dependency_closure_stays_offline() {
    let output = Command::new(env!("CARGO"))
        .args([
            "tree",
            "--locked",
            "--package",
            "capsulet-ir",
            "--edges",
            "normal",
            "--prefix",
            "none",
            "--no-dedupe",
        ])
        .current_dir(env!("CARGO_MANIFEST_DIR"))
        .output()
        .expect("cargo tree runs");

    assert!(
        output.status.success(),
        "cargo tree failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let tree = String::from_utf8(output.stdout).expect("cargo tree prints UTF-8");
    let mut found: Vec<String> = Vec::new();
    for line in tree.lines() {
        let name = line.split_whitespace().next().unwrap_or_default();
        if FORBIDDEN.contains(&name) {
            found.push(line.trim().to_string());
        }
    }
    found.sort();
    found.dedup();

    assert!(
        found.is_empty(),
        "capsulet-ir must stay offline and deterministic, but its dependency closure contains:\n{}",
        found.join("\n")
    );
}
