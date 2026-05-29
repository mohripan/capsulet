# Persistence

Capsulet uses PostgreSQL for durable control-plane metadata.

The first persistence slice stores:

- job definitions
- job runs
- job attempts

Script bundles, logs, and artifacts are intentionally not stored in PostgreSQL. They belong in object storage, with PostgreSQL storing metadata and object keys.

## Local Database

Start PostgreSQL:

```sh
docker compose up -d postgres
```

Default local connection string:

```sh
postgres://capsulet:capsulet@localhost:5432/capsulet
```

Run the persistence tests against Docker:

```powershell
$env:CAPSULET_TEST_DATABASE_URL = "postgres://capsulet:capsulet@localhost:5432/capsulet"
cargo test -p capsulet-postgres
```

## Migrations

SQL migrations live in `migrations/` and are embedded by `capsulet-postgres` using SQLx.

Use timestamped migration names:

```text
migrations/YYYYMMDDHHMMSS_description.sql
```

After a migration has been shared, treat it as immutable. Add a new migration for schema changes.

## Adapter Boundary

`capsulet-core` defines persistence ports and owns domain rules. `capsulet-postgres` implements those ports with SQLx.

This keeps the core crate free of database dependencies while still allowing service crates to depend on a concrete store.
