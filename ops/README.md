# Operations Assets

This directory contains production-readiness assets that can run against local,
staging, or production-like clusters.

## Admission Policies

The Helm chart can manage equivalent native `ValidatingAdmissionPolicy`
resources through `admissionPolicies.enabled=true`. Use the standalone manifest
when admission policy is managed outside the application release:

```powershell
kubectl apply -f ops/admission/capsulet-execution-policies.yaml
```

The pod-security policy denies unsafe execution pods. The digest image policy is
in `Audit` mode by default; change its binding to `Deny` after all approved
runtime images are pinned by digest. The Helm-managed allowlist policy can also
derive accepted image patterns from `executionPools.*.policy.images.allowed`.

## Load Smoke

Run read-only API load:

```powershell
docker run --rm -i grafana/k6 run -e CAPSULET_API_BASE_URL=http://host.docker.internal:8080 -e CAPSULET_API_TOKEN=$env:CAPSULET_TEMP_ADMIN_API_TOKEN - < ops/load/capsulet-smoke.js
```

Run submit-path load against a seeded job definition:

```powershell
docker run --rm -i grafana/k6 run -e CAPSULET_LOAD_SUBMIT=true -e CAPSULET_LOAD_JOB_DEFINITION_ID=hello_python -e CAPSULET_API_BASE_URL=http://host.docker.internal:8080 -e CAPSULET_API_TOKEN=$env:CAPSULET_TEMP_ADMIN_API_TOKEN - < ops/load/capsulet-smoke.js
```

Copy `ops/load/capacity-fixtures.example.json` for each certified cluster size
and fill in observed p95/p99 latency, failure rate, and sustained job submit
capacity.

## Observability

Starter PrometheusRule and Grafana dashboard assets live in
`ops/observability/`. The Helm chart can render equivalent resources with
`prometheusRules.enabled=true` and `grafanaDashboards.enabled=true`.
