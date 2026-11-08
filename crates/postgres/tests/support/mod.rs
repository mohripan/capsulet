use std::sync::atomic::{AtomicU64, Ordering};

mod database;

static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(1);

/// A stable name, unique within this test binary.
///
/// Unique within the binary is enough because the binary has a database of its own;
/// see [`required_database_url`].
pub fn fixture_id(prefix: &str) -> String {
    let sequence = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}_{sequence}")
}

/// This test binary's own database, recreated empty when the process first asks.
pub fn required_database_url() -> String {
    database::isolated_url(concat!(
        env!("CARGO_PKG_NAME"),
        "/",
        env!("CARGO_CRATE_NAME")
    ))
}
