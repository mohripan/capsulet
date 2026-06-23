# Backup, Restore, and DR Runbook

Capsulet production deployments should use external PostgreSQL and external S3-compatible object storage. The bundled chart dependencies are for development and small trials.

## Backup Policy

- PostgreSQL: enable PITR with continuous WAL archiving and daily full backups.
- Object storage: enable bucket versioning and lifecycle retention that matches audit requirements.
- Kubernetes: back up Helm values, external secret definitions, and the release namespace manifests.

For bundled PostgreSQL in non-production or small single-cluster trials, enable
the chart CronJob:

```yaml
postgresql:
  backup:
    enabled: true
    schedule: "17 2 * * *"
    retentionDays: 14
    persistence:
      enabled: true
      size: 20Gi
```

This produces custom-format `pg_dump` files on a backup PVC. It is a scheduled
logical backup, not PITR, and should not replace managed PostgreSQL WAL archival
for enterprise production environments.

## Restore Drill

1. Provision a clean namespace and point the chart at a restored PostgreSQL instance.
2. Restore object storage to the same bucket prefix used by the restored database.
3. Install Capsulet with `postgresql.mode=external` and `minio.mode=external`.
4. Run `capsulet-api` migrations once with `CAPSULET_MIGRATE_ONLY=true`.
5. Start API, worker, scheduler, evaluator, and dashboard.
6. Verify `/readyz`, `/metrics`, job definition listing, workflow run listing, and artifact download.

For bundled backup restore, copy the selected dump into a restore pod that has
network access to PostgreSQL, then run:

```powershell
pg_restore --clean --if-exists --no-owner --no-privileges --dbname $env:DATABASE_URL .\capsulet-restore.dump
```

## PITR

For point-in-time recovery, restore PostgreSQL to a timestamp before the incident and restore object storage to the closest matching versioned state. If object storage cannot be restored to the exact timestamp, prefer retaining extra objects over deleting objects referenced by restored database rows.

## RTO/RPO Targets

- RPO: 15 minutes with continuous WAL archiving.
- RTO: 60 minutes for a regional restore when container images and external dependencies are available.
