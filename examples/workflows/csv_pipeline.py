"""Deploy and optionally run a two-step CSV artifact workflow."""

from __future__ import annotations

import argparse
from pathlib import Path

from capsulet import CapsuletClient, task, workflow


@task(outputs=["customers.csv"])
def generate_csv():
    import csv
    from pathlib import Path

    output = Path("/capsulet/artifacts/customers.csv")
    output.parent.mkdir(parents=True, exist_ok=True)
    with output.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=["customer", "orders", "revenue"])
        writer.writeheader()
        writer.writerows([
            {"customer": "Ada", "orders": 3, "revenue": 420},
            {"customer": "Grace", "orders": 5, "revenue": 810},
            {"customer": "Linus", "orders": 2, "revenue": 190},
        ])
    print(f"wrote {output}")


@task(outputs=["customer-summary.csv"])
def summarize_csv(source):
    import csv
    from pathlib import Path

    output = Path("/capsulet/artifacts/customer-summary.csv")
    with Path(source).open(newline="") as handle:
        rows = sorted(csv.DictReader(handle), key=lambda row: int(row["revenue"]), reverse=True)
    with output.open("w", newline="") as handle:
        writer = csv.DictWriter(handle, fieldnames=["customer", "revenue"])
        writer.writeheader()
        writer.writerows({"customer": row["customer"], "revenue": row["revenue"]} for row in rows)
    print(f"wrote {output}")


@workflow(name="CSV artifact pipeline", description="Creates a CSV and passes it to a dependent task.")
def csv_pipeline():
    summarize_csv(generate_csv())


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--api-url", default="http://127.0.0.1:8080")
    parser.add_argument("--run", action="store_true", help="create a manual automation and run it")
    parser.add_argument("--output", type=Path, default=Path("customer-summary.csv"))
    args = parser.parse_args()

    client = CapsuletClient(args.api_url)
    deployed = client.deploy(csv_pipeline)
    print(f"deployed workflow {deployed['id']}")
    if not args.run:
        return
    automation = client.create_automation(
        deployed["id"], name="Run CSV artifact pipeline", automation_id="csv-artifact-pipeline-manual"
    )
    run = client.wait_for_workflow_run(client.trigger(automation["id"])["id"])
    if run["status"] != "succeeded":
        raise RuntimeError(f"workflow finished with {run['status']}")
    final_job_run = run["step_runs"][-1]["job_run_id"]
    artifact = next(item for item in client.artifacts(final_job_run) if item["name"] == "customer-summary.csv")
    client.download_artifact(final_job_run, artifact["id"], args.output)
    print(f"downloaded {args.output}")


if __name__ == "__main__":
    main()
