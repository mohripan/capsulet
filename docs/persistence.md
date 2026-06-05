# Persistence

Capsulet uses PostgreSQL for durable control-plane metadata.

The first persistence slice stores:

- job definitions
- job runs
- job attempts
- inline log previews
- artifact metadata

Script bundles, full large logs, and artifact bytes are intentionally not stored in PostgreSQL. They belong in object storage, with PostgreSQL storing metadata and object keys.

Sprint 005 object keys use run-scoped prefixes:

- `bundles/<run-id>/main.py` for submitted single-file Python scripts
- `logs/<run-id>/stdout.log` for large stdout offload
- `artifacts/<run-id>/<name>` for published job artifacts

Small logs remain inline through the existing log repository. Logs larger than 64 KiB keep an inline preview and also create a `stdout.log` artifact record.

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
