use std::{
    env::VarError,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(1);

/// A name no other run against the same database can collide with.
///
/// A counter alone repeats in every process, and a process id repeats too: Windows
/// hands the same one out again within seconds. Against a database that outlives the
/// run, either collision makes a test fail on somebody else's row. The moment this
/// process first asked, in nanoseconds, does not repeat.
pub fn fixture_id(prefix: &str) -> String {
    static STARTED: std::sync::OnceLock<u128> = std::sync::OnceLock::new();
    let started = STARTED.get_or_init(|| {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos())
    });
    let sequence = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}_{started:x}_{sequence}")
}

pub fn required_database_url() -> String {
    database_url_from(|name| std::env::var(name)).unwrap_or_else(|message| panic!("{message}"))
}

fn database_url_from(
    mut read: impl FnMut(&str) -> Result<String, VarError>,
) -> Result<String, String> {
    read("CAPSULET_TEST_DATABASE_URL")
        .or_else(|_| read("DATABASE_URL"))
        .map_err(|_| {
            "CAPSULET_TEST_DATABASE_URL is required; run `cargo run -p capsulet-xtask --locked -- verify --gate postgres`"
                .to_string()
        })
}

#[test]
fn missing_database_configuration_is_an_actionable_error() {
    let error = database_url_from(|_| Err(VarError::NotPresent)).expect_err("configuration fails");
    assert!(error.contains("CAPSULET_TEST_DATABASE_URL is required"));
    assert!(error.contains("--gate postgres"));
}
