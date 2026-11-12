# Test identities and the stack fixture

Every local and verifier stack is seeded with the same logical fixture, and none of
its credentials is checked in. `crates/xtask/src/fixtures.rs` defines it;
`crates/xtask/src/verify/compose.rs` starts the stacks that use it.

## Starting a stack

```powershell
cargo run -p capsulet-xtask -- stack up            # every service
cargo run -p capsulet-xtask -- stack up dashboard  # a service and what it depends on
cargo run -p capsulet-xtask -- stack down          # containers, volumes, images and secrets
```

`stack up` runs Compose project `capsulet-local` on the ports `compose.yaml` documents, all
bound to `127.0.0.1`. It writes three files to `.capsulet/stack/capsulet-local/`:

- `stack.env`: the environment file Compose reads its secrets and ports from
- `secrets.json`: the same secrets, reused when `stack up` runs again (PostgreSQL only reads its
  password when its volume is created)
- `fixture.json`: what a test needs, described below

`stack down` deletes all three. Plain `docker compose up` refuses to start without them and says
how to create them.

## What is seeded

Seeding goes through the API, never the database. The admin identity provisions
the projects with `PUT /v1/projects/{project_id}`, creates the resources below,
and creates the service account with `POST /v1/service-accounts`. Every step
creates a resource or fills it in again, so seeding a seeded stack leaves the
same graph.

| Alias | Id | Name |
| --- | --- | --- |
| tenant | `default` | the tenant the migrations create |
| `alpha` | `e2e-alpha` | E2E Alpha |
| `beta` | `e2e-beta` | E2E Beta |

Each project gets a job definition `e2e_<alias>_hello`, a memory subgraph `e2e_<alias>_memory`,
a canonical entity `e2e_<alias>_customer`, a three-node graph `e2e_<alias>_graph` that needs no
model, and an agent `e2e_<alias>_agent` over it. Their names are in `fixture.json`.

| Alias | Principal | Kind | Role | Default project | Comes from |
| --- | --- | --- | --- | --- | --- |
| `admin` | `e2e-admin` | user | admin (platform admin) | `default` | `CAPSULET_API_TOKENS` |
| `operator` | `e2e-operator` | user | operator | `e2e-alpha` | `CAPSULET_API_TOKENS` |
| `viewer` | `e2e-viewer` | user | viewer | `e2e-alpha` | `CAPSULET_API_TOKENS` |
| `service` | `e2e-service` | service account | operator | `e2e-alpha` | `POST /v1/service-accounts` |

Only the admin can reach beta. Operator, viewer and service account belong to alpha alone,
which is what the isolation flows rely on.

Every credential expires four hours after its stack's secrets were generated. Each seed revokes
the previous service account and issues a fresh one, because its token is only ever shown once.
The dashboard's temporary login is the admin (`dashboard_login` in `fixture.json`), and the
Keycloak realm's `admin` user gets a generated password too. So do SeaweedFS's S3 key and its
volume-server and filer signing keys (`CAPSULET_OBJECT_STORE_*` in `stack.env`).

## `fixture.json`

```json
{
  "api_url": "http://127.0.0.1:8080",
  "dashboard_url": "http://127.0.0.1:3000",
  "tenant": "default",
  "projects": { "alpha": { "id": "e2e-alpha", "name": "E2E Alpha" }, "beta": { "id": "…", "name": "…" } },
  "resources": { "alpha": { "job_definition": { "id": "e2e_alpha_hello", "name": "E2E Alpha hello" }, "…": {} } },
  "identities": {
    "admin": { "principal": "e2e-admin", "kind": "user", "role": "admin", "project": "default", "token": "…", "expires_at_unix": 0 }
  },
  "dashboard_login": { "username": "e2e-admin", "password": "…" }
}
```

Playwright reads it through `dashboard/tests/e2e/fixture.ts`, and the smoke scripts through
`scripts/stack-fixture.ps1`. Both look for `CAPSULET_FIXTURE_FILE` first, and fall back to the
local stack's file. Specs take ids and names from it rather than repeating the seed's literals.

## The verifier's stacks

The `compose` and `dashboard-e2e` gates each declare a stack. For each run, the verifier:

1. Names a Compose project `capsulet-verify-<gate>-<pid>-<millis>`, finds free ports, and writes
   fresh secrets under `.capsulet/stack/<project>/`.
2. Builds the local images and waits on every service's health check (`up --wait`), with no
   fixed sleeps.
3. Seeds twice and fails if the two snapshots of the fixture graph differ. A snapshot covers each
   identity's principal, role and memberships, the projects, the active service accounts, and the
   id and name of every seeded resource. It leaves out timestamps, generated ids and tokens.
4. Runs the gate's commands with `COMPOSE_PROJECT_NAME`, `COMPOSE_ENV_FILES`,
   `CAPSULET_FIXTURE_FILE`, `CAPSULET_API_URL` and `PLAYWRIGHT_BASE_URL` set.
5. When they pass, seeds again and fails unless the snapshot still matches. A suite may add
   things, but it must not change the fixture.
6. On any failure, appends the services' logs to the gate log.
7. Runs `docker compose down --volumes --remove-orphans --rmi local` for that project only, which
   removes exactly what Compose labelled with it, and then deletes the secrets.

Nothing a run leaves can reach the next run: each run has its own volumes. Against a
long-lived local stack, the specs still create only fixed ids, and they clean up or reuse what
they create, so running them twice gives the same results.

Playwright's global teardown deletes the saved sessions in `dashboard/playwright/.auth/`.
Failure traces in `dashboard/test-results/` can contain tokens, but those tokens stop working
when their stack is removed, and four hours after they were issued in any case.
