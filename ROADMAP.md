# Roadmap

Capsulet has crossed the original authoring, workflow, trigger, security, metrics, and local-operability milestones. This file now tracks remaining release gates instead of historical sprint backlog.

## Alpha gate

- Keep Keycloak/OIDC and service-token auth tested in compose and Helm.
- Keep workflow/job/automation management controls conflict-safe.
- Keep workflow-run detail, logs, artifacts, and re-run/resume flows covered by API and dashboard tests.
- Keep scheduler, worker, evaluator, and API metrics exposed and documented.
- Run `scripts/compose-smoke.ps1` and the minikube smoke guide before tagging.
- Validate `/openapi.json` with generated clients.
- Review `docs/security.md` and `docs/operations.md` for each deployment profile.

## Next production hardening items

- Add generated SDK packages from `openapi.json`.
- Add formal load-test result fixtures for published cluster sizes.
- Add optional WASM and direct container-runtime runners behind explicit feature gates.
- Add streaming-log transport over SSE/WebSocket in addition to polling.
- Add signed-image admission examples for common Kubernetes distributions.

## Release checklist

- Rust workspace tests, fmt, clippy pass.
- Dashboard lint, unit tests, build, and browser E2E pass.
- Helm lint/template pass.
- Docker images build for API, worker, scheduler, evaluator, dashboard.
- Compose smoke passes with Keycloak and temporary admin.
- Minikube smoke passes with Kubernetes runner reattachment.
- Documentation links are current.
