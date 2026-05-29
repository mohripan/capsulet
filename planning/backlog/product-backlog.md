# Product Backlog

## Foundation

- Scaffold Rust workspace. Done in Sprint 001.
- Scaffold Next.js dashboard. Done in Sprint 001.
- Create Helm chart skeleton. Done in Sprint 001.
- Add local development guide. Done in Sprint 001.
- Add CI workflow. Done in Sprint 001.

## Core Runtime

- Implement manual job submission. Planned for Sprint 002.
- Persist job runs and attempts. Persistence foundation done in Sprint 002.
- Add worker leasing with stub runner. Planned for Sprint 002.
- Execute jobs through Kubernetes Jobs.
- Store script bundles, logs, and artifacts in object storage.
- Add CLI status and logs commands.

## Automations

- Implement automation model.
- Implement manual trigger.
- Implement scheduled trigger.
- Add simple condition tree.
- Add dashboard automation builder.

## Execution Pools

- Add Helm-defined execution pools.
- Apply pool resource defaults.
- Apply node selectors, tolerations, and affinity.
- Add pool-level concurrency limits.
