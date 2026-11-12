<!-- capsulet-claims: CAP-WORKFLOW-001, CAP-SDK-001 -->
# Python workflow examples

The CSV example uses Python decorators to infer a two-step DAG. The first task writes `customers.csv`; the worker stages that file for the dependent task, which writes `customer-summary.csv`.

The stack's API requires a bearer token. `stack up` seeds identities and writes their tokens to
`.capsulet/stack/capsulet-local/fixture.json` (see
[test identities](../../docs/development/test-identities.md)). The example deploys job
definitions, a workflow and an automation, which need the `project_admin` role, so it runs as the
seeded `admin` in the `e2e-alpha` project. The `operator` may only trigger runs and read their
results, and its deploy fails with `permission_denied ... jobs:write`.

```powershell
python -m pip install -e sdk/python
cargo run -p capsulet-xtask -- stack up
$fixture = Get-Content .capsulet/stack/capsulet-local/fixture.json -Raw | ConvertFrom-Json
$env:CAPSULET_API_TOKEN = $fixture.identities.admin.token
python examples/workflows/csv_pipeline.py --api-url $fixture.api_url --project $fixture.projects.alpha.id --run
```

The script reads its token from `CAPSULET_API_TOKEN` unless you pass `--token`; the variable keeps
the token out of your shell history. `--project` sets the `x-capsulet-project-id` header, which the
API needs when the identity can reach more than one project. Seeded tokens expire four hours after
the stack's secrets were generated; `stack down` and `stack up` again to get new ones.

The stack publishes PostgreSQL on 55432 and every other port on 127.0.0.1 only; each can be moved
with its variable in `compose.yaml`.

The final CSV is downloaded to `customer-summary.csv`. Both task artifacts remain available through the dashboard and API.

`stack down` removes the stack, its data and its secrets:

```powershell
cargo run -p capsulet-xtask -- stack down
```
