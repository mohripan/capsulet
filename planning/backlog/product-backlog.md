# Product Backlog

## Foundation

- Scaffold Rust workspace. Done in Sprint 001.
- Scaffold Next.js dashboard. Done in Sprint 001.
- Create Helm chart skeleton. Done in Sprint 001.
- Add local development guide. Done in Sprint 001.
- Add CI workflow. Done in Sprint 001.

## Core Runtime

- Implement manual job submission. API foundation done in Sprint 002.
- Persist job runs and attempts. Persistence foundation done in Sprint 002.
- Add worker leasing with stub runner. Done in Sprint 002.
- Execute jobs through Kubernetes Jobs. Done in Sprint 003.
- Capture bounded run logs. Done in Sprint 003.
- Store script bundles, large logs, and artifacts in object storage. Done in Sprint 005.
- Add CLI status and logs commands. Done in Sprint 003.
- Add run cancellation. Done in Sprint 004.
- Add retry policy and timeout hardening. Done in Sprint 004.
- Add artifact listing and download through API and CLI. Done in Sprint 005.
- Connect dashboard to live run APIs. Done in Sprint 006.
- Add dashboard run submission. Done in Sprint 006.
- Add dashboard run detail, logs, artifacts, cancellation, and artifact download. Done in Sprint 006.

## Automations

- Implement automation model.
- Implement manual trigger.
- Implement scheduled trigger.
- Add simple condition tree.
- Add dashboard automation builder.

## Execution Pools

- Add Helm-defined execution pools. Static Helm values and runner application done in Sprint 003.
- Apply pool resource defaults. Done in Sprint 003.
- Apply node selectors and tolerations. Done in Sprint 003.
- Apply affinity.
- Add pool-level concurrency limits.

## Install and Operability

- Add local Kubernetes runner guide. Done in Sprint 003.
- Make the chart support local runner configuration. Done in Sprint 003.
- Add local object storage support for evaluation installs. Done in Sprint 005 through Docker Compose MinIO and chart S3 values.
- Add external S3-compatible object storage configuration. Done in Sprint 005.
- Add bundled PostgreSQL and MinIO chart dependencies.
- Add dashboard API URL configuration. Done in Sprint 006.
- Add metrics for queue depth and worker outcomes.
- Add lease expiry recovery. Done in Sprint 004.
- Add Kubernetes Job cleanup policy. Done in Sprint 004.
