//! A database of each test binary's own.
//!
//! Every database-backed test binary gets a database nobody else writes to, named
//! after the binary and recreated empty the first time the process asks for it. Tests
//! inside one binary share it, and keep out of each other's way with ids from a
//! per-process counter; no two binaries ever share one, so those ids can stay stable
//! (`tenant_1`, `run_2`) instead of being made unique by the clock.
//!
//! The name is fixed rather than random so that nothing accumulates on a long-lived
//! server: the next run of the same binary drops it first. Until then the data a
//! failing run left behind is there to look at.
//!
//! Shared by the `PostgreSQL` and graph-worker suites, which include this file, so the
//! rule is written once.

use std::env::VarError;
use std::sync::OnceLock;

/// The server the tests may create databases on.
///
/// # Errors
///
/// Returns an actionable message when neither variable is set, so a suite run outside
/// the verifier fails and says how to run it rather than passing with nothing tested.
pub fn configured_url(
    mut read: impl FnMut(&str) -> Result<String, VarError>,
) -> Result<String, String> {
    read("CAPSULET_TEST_DATABASE_URL")
        .or_else(|_| read("DATABASE_URL"))
        .map_err(|_| {
            "CAPSULET_TEST_DATABASE_URL is required; run `cargo run -p capsulet-xtask --locked -- verify --gate postgres`"
                .to_string()
        })
}

/// This binary's own database, created empty on first use.
///
/// `suite` names the binary, for example `capsulet-postgres/ir_runs`. The role in the
/// configured URL needs to be allowed to create databases; the verifier's disposable
/// server runs as a superuser.
///
/// # Panics
///
/// When no server is configured, or the database cannot be recreated. Either way no
/// test in the binary could mean anything.
pub fn isolated_url(suite: &str) -> String {
    static URL: OnceLock<String> = OnceLock::new();
    URL.get_or_init(|| {
        let server = configured_url(|name| std::env::var(name)).unwrap_or_else(|message| {
            panic!("{message}");
        });
        let name = database_name(suite);
        let url = with_database(&server, &name);
        // On a thread of its own, because this is reached from inside async tests and
        // blocking their runtime is not allowed.
        let created = name.clone();
        std::thread::spawn(move || recreate(&server, &created))
            .join()
            .expect("the thread recreating the test database finished")
            .unwrap_or_else(|error| panic!("could not recreate test database `{name}`: {error}"));
        url
    })
    .clone()
}

fn recreate(server: &str, name: &str) -> Result<(), String> {
    use sqlx::Connection;

    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    runtime.block_on(async {
        let mut connection = sqlx::PgConnection::connect(server)
            .await
            .map_err(|error| error.to_string())?;
        // The name is built from ASCII letters, digits and underscores only, so it is
        // safe to quote into the statement; identifiers cannot be bound parameters.
        for statement in [
            format!("DROP DATABASE IF EXISTS \"{name}\" WITH (FORCE)"),
            format!("CREATE DATABASE \"{name}\""),
        ] {
            sqlx::raw_sql(&statement)
                .execute(&mut connection)
                .await
                .map_err(|error| error.to_string())?;
        }
        connection.close().await.map_err(|error| error.to_string())
    })
}

/// `test_` and the suite, folded to what an unquoted identifier allows and to the
/// 63 bytes `PostgreSQL` keeps.
fn database_name(suite: &str) -> String {
    let mut name: String = format!("test_{suite}")
        .chars()
        .map(|each| {
            if each.is_ascii_alphanumeric() {
                each.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    name.truncate(63);
    name
}

/// The same server URL, naming another database.
fn with_database(url: &str, name: &str) -> String {
    let (scheme, rest) = url.split_once("://").unwrap_or(("postgres", url));
    let (authority, tail) = rest.split_once('/').unwrap_or((rest, ""));
    let query = tail.find('?').map_or("", |at| &tail[at..]);
    format!("{scheme}://{authority}/{name}{query}")
}

#[test]
fn a_suite_name_becomes_a_plain_identifier() {
    assert_eq!(
        database_name("capsulet-graph-worker/agent_runs"),
        "test_capsulet_graph_worker_agent_runs"
    );
    assert_eq!(database_name(&"x".repeat(100)).len(), 63);
}

#[test]
fn only_the_database_in_a_url_changes() {
    assert_eq!(
        with_database(
            "postgres://user:secret@localhost:5432/capsulet?sslmode=disable",
            "test_a"
        ),
        "postgres://user:secret@localhost:5432/test_a?sslmode=disable"
    );
    assert_eq!(
        with_database("postgres://user@db:5432", "test_b"),
        "postgres://user@db:5432/test_b"
    );
}

#[test]
fn missing_database_configuration_is_an_actionable_error() {
    let error = configured_url(|_| Err(VarError::NotPresent)).expect_err("configuration fails");
    assert!(error.contains("CAPSULET_TEST_DATABASE_URL is required"));
    assert!(error.contains("--gate postgres"));
}
