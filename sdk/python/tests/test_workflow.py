import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parents[1] / "src"))

from capsulet import task, workflow


class WorkflowCompilerTests(unittest.TestCase):
    def test_infers_dependency_and_compiles_artifact_path(self):
        @task(outputs=["customers.csv"], image="python:3.12-slim", pool="mini")
        def generate_csv():
            from pathlib import Path
            Path("/capsulet/artifacts/customers.csv").write_text("name,total\nAda,3\n")

        @task(outputs=["summary.csv"])
        def summarize_csv(source):
            from pathlib import Path
            Path("/capsulet/artifacts/summary.csv").write_text(Path(source).read_text())

        @workflow(name="CSV pipeline", description="Creates and summarizes CSV data")
        def pipeline():
            raw = generate_csv()
            summarize_csv(raw)

        spec = pipeline.build()

        self.assertEqual("csv-pipeline", spec.id)
        self.assertEqual(2, len(spec.steps))
        self.assertEqual(
            [(spec.steps[0].id, spec.steps[1].id)],
            [(edge.from_step_id, edge.to_step_id) for edge in spec.dependencies],
        )
        self.assertIn(
            f"/capsulet/inputs/{spec.steps[0].id}/customers.csv",
            spec.steps[1].python_script,
        )
        self.assertEqual("python:3.12-slim", spec.steps[0].runtime_image)
        self.assertEqual("mini", spec.steps[0].execution_pool)

    def test_repeated_build_has_stable_ids(self):
        @task
        def hello():
            print("hello")

        @workflow(name="Stable IDs")
        def pipeline():
            hello()
            hello()

        first = pipeline.build()
        second = pipeline.build()
        self.assertEqual(
            [step.id for step in first.steps],
            [step.id for step in second.steps],
        )
        self.assertNotEqual(first.steps[0].id, first.steps[1].id)

    def test_task_executes_normally_outside_workflow_build(self):
        @task
        def add(left, right):
            return left + right

        self.assertEqual(5, add(2, 3))

    def test_rejects_result_from_another_build(self):
        captured = []

        @task(outputs=["value.txt"])
        def source():
            return None

        @task
        def sink(value):
            return value

        @workflow(name="First")
        def first():
            captured.append(source())

        first.build()

        @workflow(name="Second")
        def second():
            sink(captured[0])

        with self.assertRaisesRegex(ValueError, "different workflow build"):
            second.build()


if __name__ == "__main__":
    unittest.main()
