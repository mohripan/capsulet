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


def path_parameters(path):
    return sorted(re.findall(r"\{([^}]+)\}", path))


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
            parameters = operation.get("parameters", [])
            declared_path_parameters = sorted(
                parameter["name"] for parameter in parameters
                if parameter["in"] == "path" and parameter["required"] is True
            )
            self.assertEqual(path_parameters(expected["path"]), declared_path_parameters, client_name)
            self.assertTrue(any(
                parameter["name"] == "x-request-id" and parameter["in"] == "header"
                for parameter in parameters
            ), client_name)
            request = operation.get("requestBody", {}).get("content", {}).get("application/json", {}).get("schema")
            self.assertEqual(expected["request_schema"], schema_name(request), client_name)
            self.assertEqual(expected["request_schema"] is not None, "requestBody" in operation, client_name)
            if expected["request_schema"]:
                self.assertIs(operation["requestBody"]["required"], True, client_name)
                self.assert_component_shape(expected["request_schema"], client_name)
            success = next(value for status, value in operation["responses"].items() if status.startswith("2"))
            content = next(iter(success.get("content", {}).values()), {})
            self.assertEqual(expected["response_schema"], schema_name(content.get("schema")), client_name)
            expected_content_type = (
                "application/octet-stream" if expected["response_schema"] == "BinaryResponse"
                else "text/event-stream" if expected["response_schema"] == "EventStreamResponse"
                else "application/json" if expected["response_schema"] else None
            )
            self.assertEqual([expected_content_type] if expected_content_type else [], list(success.get("content", {})), client_name)
            if expected["response_schema"]:
                self.assert_component_shape(expected["response_schema"], client_name)
            error_schema = operation["responses"]["default"]["content"]["application/json"]["schema"]
            self.assertEqual("Error", schema_name(error_schema), client_name)
            if expected["project_context"]:
                self.assertTrue(any(
                    parameter["name"] == "x-capsulet-project-id" and parameter["in"] == "header"
                    for parameter in operation["parameters"]
                ), client_name)
            self.assertEqual(expected["project_context"], any(
                parameter["name"] == "x-capsulet-project-id" and parameter["in"] == "header"
                for parameter in parameters
            ), client_name)

    def assert_component_shape(self, name, client_name):
        schema = self.document["components"]["schemas"][name]
        if schema.get("type") != "object":
            return
        properties = schema.get("properties")
        self.assertIsInstance(properties, dict, f"{client_name}: {name} properties")
        for field in schema.get("required", []):
            self.assertIn(field, properties, f"{client_name}: {name}.{field}")
        for field, property_schema in properties.items():
            property_types = property_schema.get("type")
            property_types = property_types if isinstance(property_types, list) else [property_types]
            if "null" in property_types:
                self.assertTrue(any(value != "null" for value in property_types), f"{client_name}: {name}.{field}")

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
