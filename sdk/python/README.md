<!-- capsulet-claims: CAP-WORKFLOW-001, CAP-SDK-001 -->
# Capsulet Python SDK

**Experimental compatibility SDK.** Define Python tasks with decorators, infer dependencies from
function calls, and deploy the resulting compatibility workflow to Capsulet. This authoring layer
does not yet target the planned unified verified-computation IR, and its transport is handwritten.
The transport publishes `CLIENT_OPERATIONS`, an explicit experimental method/path/operation/schema
map checked against `crates/api/openapi.json`. It is not a generated or compatibility-stable SDK.

```python
from capsulet import CapsuletClient, task, workflow

@task(outputs=["raw.csv"])
def extract():
    ...

@task(outputs=["summary.csv"])
def transform(raw_csv):
    ...

@workflow(name="Daily report")
def daily_report():
    transform(extract())

CapsuletClient().deploy(daily_report)
```

A downstream task-result argument compiles to the staged path of the upstream task's first declared output. Tasks can still be called normally outside a workflow build, which keeps their business logic directly testable.

Run its unit and OpenAPI conformance tests from the repository root:

```sh
python -m unittest discover -s sdk/python/tests -p "test_*.py"
```
