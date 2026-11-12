<!-- capsulet-claims: CAP-PRODUCT-001, CAP-MEMORY-001, CAP-MEMORY-002, CAP-MEMORY-003, CAP-MEMORY-004, CAP-MEMORY-005, CAP-MEMORY-006, CAP-MEMORY-007, CAP-JOB-001, CAP-JOB-002, CAP-JOB-003, CAP-JOB-004, CAP-JOB-005, CAP-JOB-006, CAP-JOB-007, CAP-JOB-008, CAP-JOB-009, CAP-WORKFLOW-001, CAP-AUTOMATION-001, CAP-AUTOMATION-002, CAP-AUTOMATION-003, CAP-AUTOMATION-004, CAP-AUTOMATION-005, CAP-AUTOMATION-006, CAP-AUTOMATION-007, CAP-AUTOMATION-008, CAP-IAM-001, CAP-IAM-002, CAP-IAM-003, CAP-IAM-004, CAP-IAM-005, CAP-IAM-006, CAP-IAM-007, CAP-IAM-009, CAP-DASHBOARD-001, CAP-DASHBOARD-002 -->
# Capsulet Dashboard

This experimental Next.js dashboard covers the current workflow engine, agent-platform subsystems,
and operations. Its API transport is handwritten. Settings and fallback overview data are visibly
demonstrative; graph execution, assurance, and certificate views planned by the constitution are
not yet implemented.

The handwritten transport is experimental and publishes `DASHBOARD_OPERATIONS` in
`app/lib/api.ts`. Tests compare every declared method, path template, `operationId`, project-header
rule, and request/response schema to the checked generated OpenAPI document. This is an M0
conformance boundary, not a published generated TypeScript SDK.

## Requirements

- Node.js 20.x
- npm 10.x

The current local environment uses Node.js 20.15.1 and npm 10.7.0.

## Commands

Install dependencies:

```sh
npm install
```

Run the development server:

```sh
npm run dev
```

Point the dashboard at a local stack's API, signing in as its admin. The stack's
credentials are generated for it by `cargo run -p capsulet-xtask -- stack up`, which
writes them to `.capsulet/stack/capsulet-local/fixture.json`:

```powershell
$fixture = Get-Content -Raw ../.capsulet/stack/capsulet-local/fixture.json | ConvertFrom-Json
$env:CAPSULET_DASHBOARD_API_URL = $fixture.api_url
$env:CAPSULET_TEMP_ADMIN_USERNAME = $fixture.dashboard_login.username
$env:CAPSULET_TEMP_ADMIN_PASSWORD = $fixture.dashboard_login.password
$env:CAPSULET_TEMP_ADMIN_API_TOKEN = $fixture.identities.admin.token
npm run dev
```

The end-to-end suite runs against such a stack: `npm run test:e2e`. See
[test identities](../docs/development/test-identities.md).

For Keycloak login, set:

```powershell
$env:CAPSULET_DASHBOARD_OIDC_PUBLIC_ISSUER = "http://localhost:18080/realms/capsulet"
$env:CAPSULET_DASHBOARD_OIDC_INTERNAL_ISSUER = "http://localhost:18080/realms/capsulet"
$env:CAPSULET_DASHBOARD_OIDC_CLIENT_ID = "capsulet-dashboard"
```

Type-check the dashboard:

```sh
npx tsc --noEmit
```

Production build:

```sh
npm run build
```

Run dashboard tests:

```sh
npm test
```

## Routes

- `/`: overview
- `/automations`: automation catalog and trigger builder
- `/workflows`: workflow definitions and lineage graph
- `/runs`: live run queue, seeded job/script submission, and links to run detail
- `/runs/[id]`: live run status, logs, cancellation, artifacts, and artifact download
- `/execution-pools`: pool routing and node placement
- `/artifacts`: object storage and retention view
- `/security`: pod security, network policy, webhook auth, and service accounts
- `/settings`: platform defaults and future configuration surfaces

## Current Build Caveat

`npx tsc --noEmit` passes and all routes serve successfully in development. During recent local testing on Windows, `next build` intermittently hung in Next.js build worker processes after the dashboard was expanded to multiple routes.

For Sprint 001, CI should run:

```sh
npm ci
npx tsc --noEmit
```

The production build issue should be revisited when the frontend stack is hardened. It may be resolved by a Node.js patch update, a Next.js patch update, or reducing build worker concurrency if needed.

## Data Boundary

Mock data lives in `app/mock-data.ts`. API integration keeps route components focused on presentation and moves data fetching behind typed client functions in `app/lib/api.ts`.

The dashboard uses the same-origin route `app/api/capsulet/[...path]/route.ts` as a server-side proxy. Configure the upstream API with `CAPSULET_DASHBOARD_API_URL`; it defaults to `http://127.0.0.1:8080` for local development.
