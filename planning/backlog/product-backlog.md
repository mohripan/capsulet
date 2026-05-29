# Product Backlog

## Foundation

- Scaffold Rust workspace.
- Scaffold Next.js dashboard.
- Create Helm chart skeleton.
- Add local development guide.
- Add CI workflow.

## Core Runtime

- Implement manual job submission.
- Persist job runs and attempts.
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

