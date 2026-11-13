<!-- capsulet-claims: CAP-WORKFLOW-001, CAP-SDK-001, CAP-ASSURANCE-010 -->
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

## Writing a validator

`capsulet.verifier` speaks the verifier protocol, `capsulet.verifier/v1`
([verifier protocol](../../docs/contracts/verifier-protocol.md)). A validator reads one request,
decides each obligation, and writes one response:

```python
from capsulet import verifier

def decide(request, inputs, outputs):
    patch = (inputs / "patch.diff").read_text()
    (outputs / "report.txt").write_text(f"read {len(patch)} characters\n")
    return [
        verifier.result(obligation, verifier.CANNOT_DECIDE, "this example decides nothing")
        for obligation in request["obligations"]
    ]

verifier.serve(decide)
```

A request in another protocol version is refused. A result needs a known outcome (`holds`,
`does_not_hold` or `cannot_decide`) and a reason. What kind of checker the validator is comes from its
registration, not from anything it writes. The SDK is tested against the same protocol fixture as
the Rust host, `crates/verifier/tests/fixtures/protocol-v1.json`.

