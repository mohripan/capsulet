#![allow(
    dead_code,
    reason = "each test binary compiles this module and uses a different subset"
)]

//! What every `PostgreSQL` suite shares: its own database, stable fixture ids, a
//! migrated store, and the IR definition the IR suites run.

use std::sync::atomic::{AtomicU64, Ordering};

use capsulet_postgres::PostgresStore;

mod database;
pub mod ir;

static NEXT_FIXTURE_ID: AtomicU64 = AtomicU64::new(1);

/// A stable name, unique within this test binary.
///
/// Each test has a database of its own (see [`required_database_url`]), so these
/// only need to be distinct within one test; unique within the binary is more than
/// enough, and keeps the ids readable.
pub fn fixture_id(prefix: &str) -> String {
    let sequence = NEXT_FIXTURE_ID.fetch_add(1, Ordering::Relaxed);
    format!("{prefix}_{sequence}")
}

/// The calling test's own database, created empty on its first request.
///
/// Ask for it on the test's own thread: a worker thread of a multi-threaded runtime
/// is refused rather than handed somebody else's database.
pub fn required_database_url() -> String {
    database::isolated_url(concat!(
        env!("CARGO_PKG_NAME"),
        "/",
        env!("CARGO_CRATE_NAME")
    ))
}

/// A store on this test's own database, migrated to the current schema.
pub async fn store() -> PostgresStore {
    let store = PostgresStore::connect(&required_database_url())
        .await
        .expect("connect to postgres");
    store.migrate().await.expect("run migrations");
    store
}
