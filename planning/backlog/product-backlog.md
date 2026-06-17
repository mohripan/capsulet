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

- Implement job definition CRUD API and dashboard authoring. Done in Sprint 009.
- Implement execution pool list API and dashboard wiring. Done in Sprint 009.
- Design workflow and automation MVP. Done in Sprint 008.
- Implement linear workflow definition CRUD. Done in Sprint 009.
- Implement automation CRUD with manual trigger. Done in Sprint 009.
- Implement workflow run orchestration for sequential steps. Done in Sprint 009.
- Add dashboard workflow builder for linear workflows. Done in Sprint 009.
- Add dashboard automation builder for manual automations. Done in Sprint 009.
- Implement scheduled interval trigger. Done in Sprint 009.
- Add workflow run detail page with step-level job links. Planned for Sprint 010.
- Add edit/delete management controls for authoring resources. Planned for Sprint 010.
- Add simple condition tree. Deferred until after linear workflow MVP.
- Add workflow DAG graph engine with cycle detection, topological sort, and ready-step discovery. Deferred until after linear workflow MVP. See `docs/design/workflow-dag-graph-plan.md`.

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
- Add bundled PostgreSQL and MinIO chart dependencies. Done in Sprint 007.
- Add dashboard API URL configuration. Done in Sprint 006.
- Fix dashboard long-value layout and remove fake global create controls. Done in Sprint 008.
- Remove misleading hard-coded automation, workflow, and execution-pool dashboard state. Done in Sprint 009.
- Add repeatable Compose MVP smoke script. Planned for Sprint 010.
- Add minikube MVP smoke guide. Planned for Sprint 010.
- Add metrics for queue depth and worker outcomes.
- Add lease expiry recovery. Done in Sprint 004.
- Add Kubernetes Job cleanup policy. Done in Sprint 004.

## Release

- Build container images on version tags. Deferred until after Sprint 010 MVP hardening.
- Push release images to GHCR. Deferred until after Sprint 010 MVP hardening.
- Package Helm chart artifacts. Deferred until after Sprint 010 MVP hardening.
- Generate release notes. Deferred until after Sprint 010 MVP hardening.
- Publish a Helm repository. Deferred until after release automation.
