use std::{
    env::VarError,
    sync::atomic::{AtomicU64, Ordering},
};

static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(1);

pub fn fixture_id(prefix: &str) -> String {
    let sequence = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}_{sequence}")
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
