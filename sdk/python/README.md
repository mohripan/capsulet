# Capsulet Python SDK

Define Python tasks with decorators, infer dependencies from function calls, and deploy the resulting workflow to Capsulet.

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
