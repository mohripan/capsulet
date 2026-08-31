import json
import re
import sys
import unittest
from pathlib import Path

PYTHON_ROOT = Path(__file__).parents[1]
REPO_ROOT = Path(__file__).parents[3]
sys.path.insert(0, str(PYTHON_ROOT / "src"))

from capsulet.client import CLIENT_OPERATIONS


def schema_name(schema):
    reference = (schema or {}).get("$ref")
    return reference.rsplit("/", 1)[-1] if reference else None


class OpenApiContractTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.document = json.loads((REPO_ROOT / "crates" / "api" / "openapi.json").read_text())

    def test_every_declared_transport_operation_matches_openapi(self):
        self.assertEqual(7, len(CLIENT_OPERATIONS))
        for client_name, expected in CLIENT_OPERATIONS.items():
            operation = self.document["paths"][expected["path"]][expected["method"].lower()]
            self.assertEqual(expected["operation_id"], operation["operationId"], client_name)
            self.assertEqual(expected["project_context"], operation["x-capsulet-project-context"], client_name)
            request = operation.get("requestBody", {}).get("content", {}).get("application/json", {}).get("schema")
            self.assertEqual(expected["request_schema"], schema_name(request), client_name)
            success = next(value for status, value in operation["responses"].items() if status.startswith("2"))
            content = next(iter(success.get("content", {}).values()), {})
            self.assertEqual(expected["response_schema"], schema_name(content.get("schema")), client_name)
            if expected["project_context"]:
                self.assertTrue(any(
                    parameter["name"] == "x-capsulet-project-id" and parameter["in"] == "header"
                    for parameter in operation["parameters"]
                ), client_name)

    def test_generated_names_do_not_collide_after_python_normalization(self):
        operation_ids = []
        for path in self.document["paths"].values():
            operation_ids.extend(
                item["operationId"] for method, item in path.items()
                if method in {"get", "post", "put", "delete", "patch"}
            )
        normalized = [re.sub(r"(?<!^)(?=[A-Z])", "_", value).lower() for value in operation_ids]
        self.assertEqual(len(normalized), len(set(normalized)))
        schemas = list(self.document["components"]["schemas"])
        normalized_schemas = [re.sub(r"[^a-z0-9]", "", value.lower()) for value in schemas]
        self.assertEqual(len(normalized_schemas), len(set(normalized_schemas)))


if __name__ == "__main__":
    unittest.main()
