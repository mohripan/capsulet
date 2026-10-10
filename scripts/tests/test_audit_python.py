import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

SCRIPT = Path(__file__).resolve().parents[1] / "audit-python.py"


class PythonAuditDeclarationTests(unittest.TestCase):
    def inspect(self, project):
        with tempfile.TemporaryDirectory() as temporary:
            manifest = Path(temporary) / "pyproject.toml"
            manifest.write_text('[build-system]\nrequires = ["hatchling==1.27.0"]\n[project]\n' + project, encoding="utf-8")
            return subprocess.run([sys.executable, str(SCRIPT), "--manifest", str(manifest), "--list-requirements"],
                                  capture_output=True, text=True, check=False)

    def test_future_runtime_and_optional_dependencies_are_not_skipped(self):
        result = self.inspect('dependencies = ["requests==2.32.3"]\n[project.optional-dependencies]\nextra = ["httpx==0.28.1"]\n')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), ["hatchling==1.27.0", "httpx==0.28.1", "requests==2.32.3"])

    def test_dependency_free_sdk_still_includes_its_build_backend(self):
        result = self.inspect('dependencies = []\n')
        self.assertEqual(result.returncode, 0, result.stderr)
        self.assertEqual(json.loads(result.stdout), ["hatchling==1.27.0"])

    def test_dynamic_dependency_declarations_fail_closed(self):
        result = self.inspect('dynamic = ["dependencies"]\n')
        self.assertNotEqual(result.returncode, 0)
        self.assertIn("dynamic SDK dependencies", result.stderr)


if __name__ == "__main__":
    unittest.main()
