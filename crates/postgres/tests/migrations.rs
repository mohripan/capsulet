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
