<!-- capsulet-claims: CAP-WORKFLOW-001, CAP-SDK-001 -->
# Python workflow examples

The CSV example uses Python decorators to infer a two-step DAG. The first task writes `customers.csv`; the worker stages that file for the dependent task, which writes `customer-summary.csv`.

```powershell
python -m pip install -e sdk/python
cargo run -p capsulet-xtask -- stack up
python examples/workflows/csv_pipeline.py --run
```

The stack publishes PostgreSQL on 55432 and every other port on 127.0.0.1 only; each can be moved
with its variable in `compose.yaml`.

The final CSV is downloaded to `customer-summary.csv`. Both task artifacts remain available through the dashboard and API.
