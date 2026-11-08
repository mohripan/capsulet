//! A database of each test's own.
//!
//! Every database-backed test gets a database nobody else writes to, named after its
//! binary and the test, and recreated empty the first time the test asks for it.
//! Parallel tests in one binary therefore never share mutable tables: a worker in one
//! test cannot lease a run another test queued, and no test reads rows another wrote.
//! Fixture ids still come from a per-process counter, which keeps them stable
//! (`tenant_1`, `run_2`) instead of making them unique by the clock.
//!
//! The test is recognised by its thread. libtest runs every test on a thread named
//! after it, with or without `--test-threads=1`, and a current-thread tokio runtime
//! keeps the test's spawned tasks on that thread too. A request from anywhere else
//! (a multi-threaded runtime's worker, a thread the test spawned) fails loudly
//! rather than quietly sharing a database.
//!
//! Names are fixed rather than random so that nothing accumulates on a long-lived
//! server: the next run of the same test drops its database first. Until then the
//! data a failing test left behind is there to look at.
//!
//! Shared by the `PostgreSQL` and graph-worker suites, which include this file, so the
//! rule is written once.

use std::collections::HashMap;
use std::env::VarError;
use std::sync::{Mutex, OnceLock, PoisonError};

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

/// The calling test's own database, created empty on its first request.
///
/// `suite` names the binary, for example `capsulet-postgres/ir_runs`. Later requests
/// from the same test get the same database. The role in the configured URL needs to
/// be allowed to create databases; the verifier's disposable server runs as a
/// superuser.
///
/// # Panics
///
/// When no server is configured, when it is called from a thread that is not a
/// test's own, or when the database cannot be recreated. In each case no test could
/// mean anything.
pub fn isolated_url(suite: &str) -> String {
    static URLS: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    let test = current_test().unwrap_or_else(|message| panic!("{message}"));
    // Held while the database is created, so that two requests from one test cannot
    // both recreate it, and creations in one binary do not race each other.
    let mut urls = URLS
        .get_or_init(Mutex::default)
        .lock()
        .unwrap_or_else(PoisonError::into_inner);
    if let Some(url) = urls.get(&test) {
        return url.clone();
    }
    let server = configured_url(|name| std::env::var(name)).unwrap_or_else(|message| {
        panic!("{message}");
    });
    let name = database_name(suite, &test);
    let url = with_database(&server, &name);
    // On a thread of its own, because this is reached from inside async tests and
    // blocking their runtime is not allowed.
    let created = name.clone();
    std::thread::spawn(move || recreate(&server, &created))
        .join()
        .expect("the thread recreating the test database finished")
        .unwrap_or_else(|error| panic!("could not recreate test database `{name}`: {error}"));
    urls.insert(test, url.clone());
    url
}

/// The test running on this thread, by the name libtest gave the thread.
fn current_test() -> Result<String, String> {
    match std::thread::current().name() {
        // A test's name is a Rust path: never `main`, and never with a hyphen,
        // which is how a tokio worker (`tokio-runtime-worker`) is told apart.
        Some(name) if name != "main" && !name.contains('-') => Ok(name.to_string()),
        other => Err(format!(
            "a test database was requested from thread {other:?}, which is not a test's own; \
             ask for it on the test's thread, before spawning, and pass the URL along"
        )),
    }
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

/// `t_`, the binary and the test, folded to what an unquoted identifier allows, cut
/// to fit `PostgreSQL`'s 63 bytes, and ended with a hash of the whole so that two
/// long names sharing a prefix still differ.
fn database_name(suite: &str, test: &str) -> String {
    let binary = suite.rsplit('/').next().unwrap_or(suite);
    let leaf = test.rsplit("::").next().unwrap_or(test);
    let mut readable: String = format!("t_{binary}_{leaf}")
        .chars()
        .map(|each| {
            if each.is_ascii_alphanumeric() {
                each.to_ascii_lowercase()
            } else {
                '_'
            }
        })
        .collect();
    readable.truncate(63 - 17);
    format!("{readable}_{:016x}", fnv1a(&format!("{suite}/{test}")))
}

/// FNV-1a: stable across Rust releases and platforms, which `std`'s hasher is not.
fn fnv1a(text: &str) -> u64 {
    text.bytes().fold(0xcbf2_9ce4_8422_2325, |hash, byte| {
        (hash ^ u64::from(byte)).wrapping_mul(0x0100_0000_01b3)
    })
}

/// The same server URL, naming another database.
fn with_database(url: &str, name: &str) -> String {
    let (scheme, rest) = url.split_once("://").unwrap_or(("postgres", url));
    let (authority, tail) = rest.split_once('/').unwrap_or((rest, ""));
    let query = tail.find('?').map_or("", |at| &tail[at..]);
    format!("{scheme}://{authority}/{name}{query}")
}

#[test]
fn a_test_name_becomes_a_distinct_plain_identifier() {
    let name = database_name(
        "capsulet-graph-worker/agent_runs",
        "a_run_started_over_http_is_executed_by_the_worker",
    );
    assert!(name.starts_with("t_agent_runs_a_run_started"), "{name}");
    assert!(name.len() <= 63, "{name}");
    assert!(
        name.bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_'),
        "{name}"
    );
    assert_eq!(
        name,
        database_name(
            "capsulet-graph-worker/agent_runs",
            "a_run_started_over_http_is_executed_by_the_worker"
        ),
        "a name is the same every run, so the next run drops it"
    );
    let long = "x".repeat(100);
    assert_ne!(
        database_name("capsulet-postgres/ir_runs", &format!("{long}_one")),
        database_name("capsulet-postgres/ir_runs", &format!("{long}_two")),
        "names cut to the same prefix still differ"
    );
    assert_ne!(
        database_name("capsulet-postgres/ir_runs", "same"),
        database_name("capsulet-postgres/worker", "same"),
        "the same test name in two binaries is two databases"
    );
}

#[test]
fn a_test_is_found_by_its_own_thread_and_nowhere_else() {
    let found = current_test().expect("a test's own thread");
    assert!(
        found.ends_with("a_test_is_found_by_its_own_thread_and_nowhere_else"),
        "{found}"
    );
    for thread in [
        std::thread::Builder::new().name("tokio-runtime-worker".to_string()),
        std::thread::Builder::new(),
    ] {
        let refused = thread
            .spawn(current_test)
            .expect("spawns")
            .join()
            .expect("joins");
        assert!(refused.is_err(), "{refused:?}");
    }
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
