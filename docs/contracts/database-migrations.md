# Database Migration Policy

Database migrations are forward-only, immutable after release, and applied in deterministic file
order. Correcting a released migration requires a new migration; editing history would make two
installations with the same reported version structurally different.

## Upgrade sequence

Schema changes use expand/migrate/contract:

1. **Expand:** add compatible tables, columns, indexes, or dual-read/write support. Old and new
   application versions must coexist for the documented rollout window.
2. **Migrate:** backfill or transform data through restartable, observable, bounded work. Record
   progress and validate counts/invariants before proceeding.
3. **Contract:** remove deprecated reads, writes, or schema only after every supported source
   version has passed through the compatible release and the deprecation obligation is satisfied.

Release documentation names the oldest supported source version and tests every supported upgrade
path. Skipping releases is supported only when explicitly listed; otherwise operators upgrade
through the documented intermediate releases.

## Backup, restore, and rollback

Before an upgrade, operators create a database backup and corresponding object-store/configuration
snapshot, record the product/schema versions, and verify that the backup can be read. Release
qualification performs an actual restore into an isolated environment and checks migrations,
readiness, representative reads, artifact references, and ownership boundaries.

Rollback is a planned operational procedure, not an assumption that down migrations are safe:

- before irreversible data migration, the previous application may be redeployed only while the
  expanded schema remains backward compatible;
- after transformation or contract, rollback normally restores the pre-upgrade database and
  object/configuration snapshot together;
- forward repair may be safer than restore when new writes have occurred, and the runbook must name
  that decision point; and
- SQL rollback scripts do not claim to reverse destructive or semantic data transformations.

Migration jobs must fail visibly and prevent incompatible application startup. They may not mark an
upgrade successful after skipped validation or partial backfill.
