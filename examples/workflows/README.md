# Python workflow examples

The CSV example uses Python decorators to infer a two-step DAG. The first task writes `customers.csv`; the worker stages that file for the dependent task, which writes `customer-summary.csv`.

```powershell
python -m pip install -e sdk/python
docker compose up -d --build
python examples/workflows/csv_pipeline.py --run
```

If port 5432 is already occupied, use `docker compose -f compose.yaml -f compose.e2e.yaml up -d --build`; the override publishes PostgreSQL on 55432 without changing service-to-service connections.

The final CSV is downloaded to `customer-summary.csv`. Both task artifacts remain available through the dashboard and API.
