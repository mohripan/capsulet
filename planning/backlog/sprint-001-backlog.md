# Sprint 001 Backlog

This is the working backlog for Sprint 001: Foundation.

Status legend:

- `todo`: not started
- `doing`: in progress
- `blocked`: waiting on a decision or prerequisite
- `done`: complete

## Backend Foundation

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S1-BE-001 | done | Add root Rust workspace `Cargo.toml` | Workspace includes all crates under `crates/` |
| S1-BE-002 | done | Add `Cargo.toml` for each crate | `cargo metadata` succeeds |
| S1-BE-003 | done | Add minimal `core` library | `cargo test -p capsulet-core` passes |
| S1-BE-004 | done | Add minimal service binaries | API, worker, scheduler, evaluator, runner, and CLI compile |
| S1-BE-005 | done | Add baseline Rust checks | `cargo fmt --check`, `cargo clippy`, and `cargo test` are documented |

## Dashboard Foundation

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S1-FE-001 | done | Create multi-page visual dashboard prototype | Routes exist for overview, automations, workflows, runs, pools, artifacts, security, settings |
| S1-FE-002 | todo | Add dashboard README | README lists setup commands and routes |
| S1-FE-003 | todo | Track build caveat | Windows `next build` hang is documented or resolved |
| S1-FE-004 | todo | Keep mock data isolated | Mock data lives outside route components |

## Helm Foundation

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S1-HELM-001 | todo | Add `Chart.yaml` | Chart metadata is valid |
| S1-HELM-002 | todo | Add `values.yaml` | Includes image, service, dashboard, persistence, and execution pool defaults |
| S1-HELM-003 | todo | Add `values.schema.json` | Basic values validation exists |
| S1-HELM-004 | todo | Add minimal workload templates | API, worker, scheduler, evaluator, and dashboard render |
| S1-HELM-005 | todo | Add chart smoke test template | `helm template` renders test resources |

## Documentation

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S1-DOC-001 | todo | Add `docs/development.md` | Explains required tools and local commands |
| S1-DOC-002 | todo | Add `docs/installation.md` | Explains intended Helm install flow |
| S1-DOC-003 | todo | Add `docs/helm-values.md` stub | Documents initial values sections |
| S1-DOC-004 | todo | Update root README commands | README points to dashboard, architecture, roadmap, and dev docs |

## CI

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S1-CI-001 | todo | Add Rust CI workflow | Runs fmt, clippy, test |
| S1-CI-002 | todo | Add dashboard CI workflow | Runs install and typecheck/build command |
| S1-CI-003 | todo | Add Helm CI workflow | Runs helm lint and helm template |

## ADRs

| ID | Status | Task | Acceptance |
| --- | --- | --- | --- |
| S1-ADR-001 | done | ADR for Rust workspace layout | Documents crate boundaries |
| S1-ADR-002 | todo | ADR for Next.js dashboard | Documents frontend choice |
| S1-ADR-003 | todo | ADR for Helm-first distribution | Documents chart-as-product decision |
| S1-ADR-004 | todo | ADR for object storage usage | Documents scripts/logs/artifacts storage decision |
| S1-ADR-005 | todo | ADR for Kafka target event channel | Documents long-term eventing direction |

## Sprint Risks

- Helm chart work can expand too quickly. Keep templates minimal.
- Dashboard polish can consume time. Treat current UI as good enough for Sprint 001.
- Rust crate boundaries can be over-designed. Add only enough structure for Sprint 002.
- Kafka should stay architectural in this sprint. Do not integrate it yet.

## Sprint Exit

Move any unfinished `todo` items back into `planning/backlog/product-backlog.md` or into Sprint 002 planning.
