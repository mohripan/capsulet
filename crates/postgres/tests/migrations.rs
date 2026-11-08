use std::{collections::BTreeSet, path::Path};

use capsulet_postgres::{PostgresPoolConfig, PostgresStore};

mod support;

#[test]
fn migration_files_are_forward_only_and_uniquely_ordered() {
    assert_eq!(support::fixture_id("migration"), "migration_1");
    let directory = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    let mut names = std::fs::read_dir(directory)
        .expect("read migrations")
        .map(|entry| {
            entry
                .expect("migration entry")
                .file_name()
                .to_string_lossy()
                .into_owned()
        })
        .filter(|name| {
            Path::new(name)
                .extension()
                .is_some_and(|extension| extension.eq_ignore_ascii_case("sql"))
        })
        .collect::<Vec<_>>();
    names.sort();
    let versions = names
        .iter()
        .map(|name| {
            name.split('_')
                .next()
                .expect("version prefix")
                .parse::<u64>()
                .expect("numeric migration version")
        })
        .collect::<Vec<_>>();
    assert!(
        versions.windows(2).all(|pair| pair[0] < pair[1]),
        "migration versions must move strictly forward"
    );
    let unique_versions = versions.iter().collect::<BTreeSet<_>>();
    assert_eq!(
        unique_versions.len(),
        names.len(),
        "migration versions must be unique"
    );
    assert!(names.iter().all(|name| !name.contains("down")));
    // sqlx runs a migration outside a transaction only when its first line asks
    // for it. None may: a migration that fails halfway must leave nothing behind.
    for name in &names {
        let sql = std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR"))
                .join("../../migrations")
                .join(name),
        )
        .expect("read migration");
        assert!(
            !sql.trim_start().starts_with("-- no-transaction"),
            "{name} opts out of its transaction"
        );
    }
}

#[tokio::test]
async fn migrates_the_empty_supported_snapshot_repeatably_and_detects_tampering() {
    let database_url = support::required_database_url();
    let store = PostgresStore::connect(&database_url)
        .await
        .expect("connect");
    sqlx::raw_sql(include_str!("fixtures/v0_empty.sql"))
        .execute(store.pool())
        .await
        .expect("load v0 fixture");
    store.migrate().await.expect("migrate v0 to current");
    store.migrate().await.expect("repeat migration detection");

    let applied: i64 = sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE success")
        .fetch_one(store.pool())
        .await
        .expect("count migrations");
    let expected =
        std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations"))
            .expect("read migrations")
            .filter_map(Result::ok)
            .filter(|entry| {
                entry
                    .path()
                    .extension()
                    .is_some_and(|extension| extension == "sql")
            })
            .count();
    assert_eq!(
        usize::try_from(applied).expect("non-negative count"),
        expected
    );
    let recorded: i64 =
        sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations WHERE success")
            .fetch_one(store.pool())
            .await
            .expect("read the schema version");
    assert_eq!(
        recorded,
        newest_migration_version(),
        "the schema is recorded at the newest migration"
    );

    sqlx::query("UPDATE _sqlx_migrations SET checksum = decode('00', 'hex') WHERE version = (SELECT max(version) FROM _sqlx_migrations)")
        .execute(store.pool())
        .await
        .expect("tamper migration checksum");
    assert!(store.migrate().await.is_err(), "checksum drift must fail");
}

#[tokio::test]
async fn malformed_and_unavailable_connections_fail() {
    assert!(PostgresStore::connect("not-a-postgres-url").await.is_err());
    let config = PostgresPoolConfig {
        acquire_timeout: std::time::Duration::from_millis(100),
        ..PostgresPoolConfig::default()
    };
    assert!(
        PostgresStore::connect_with_config("postgres://postgres:x@127.0.0.1:1/unavailable", config)
            .await
            .is_err()
    );
}

/// A migration that fails partway leaves nothing behind.
///
/// The real migrations are copied beside one that creates a table and then fails.
/// Running them must fail, the table must not exist afterwards, and no success may
/// be recorded for the failed version, while every real migration stays applied.
#[tokio::test]
async fn a_migration_that_fails_partway_leaves_nothing_behind() {
    let store = PostgresStore::connect(&support::required_database_url())
        .await
        .expect("connect");
    store.migrate().await.expect("migrate to current");

    let directory = Scratch::new("failing-migration");
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations");
    for entry in std::fs::read_dir(&source).expect("read migrations") {
        let entry = entry.expect("migration entry");
        std::fs::copy(entry.path(), directory.0.join(entry.file_name())).expect("copy migration");
    }
    let failing = newest_migration_version() + 1;
    std::fs::write(
        directory.0.join(format!("{failing}_fails_partway.sql")),
        "CREATE TABLE partial_migration_probe (id integer);\nSELECT 1 / 0;\n",
    )
    .expect("write the failing migration");

    let migrator = sqlx::migrate::Migrator::new(directory.0.as_path())
        .await
        .expect("read the copied migrations");
    assert!(
        migrator.run(store.pool()).await.is_err(),
        "division by zero must fail the migration"
    );

    let probe: Option<String> =
        sqlx::query_scalar("SELECT to_regclass('partial_migration_probe')::text")
            .fetch_one(store.pool())
            .await
            .expect("look for the table");
    assert_eq!(probe, None, "the failed migration's table was rolled back");
    let recorded: i64 =
        sqlx::query_scalar("SELECT count(*) FROM _sqlx_migrations WHERE version = $1 AND success")
            .bind(failing)
            .fetch_one(store.pool())
            .await
            .expect("read the migration record");
    assert_eq!(
        recorded, 0,
        "no success is recorded for the failed migration"
    );
    let current: i64 =
        sqlx::query_scalar("SELECT max(version) FROM _sqlx_migrations WHERE success")
            .fetch_one(store.pool())
            .await
            .expect("read the schema version");
    assert_eq!(current, newest_migration_version());
}

/// The version of the newest migration file.
fn newest_migration_version() -> i64 {
    std::fs::read_dir(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations"))
        .expect("read migrations")
        .filter_map(Result::ok)
        .filter_map(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .split('_')
                .next()
                .and_then(|version| version.parse().ok())
        })
        .max()
        .expect("at least one migration")
}

/// A temporary directory, removed when the test ends however it ends.
struct Scratch(std::path::PathBuf);

impl Scratch {
    fn new(purpose: &str) -> Self {
        let path = std::env::temp_dir().join(format!(
            "capsulet-{purpose}-{}-{}",
            std::process::id(),
            support::fixture_id("scratch")
        ));
        std::fs::create_dir_all(&path).expect("create the scratch directory");
        Self(path)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}
