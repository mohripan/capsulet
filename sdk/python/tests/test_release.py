import re
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parents[1] / "src"))

import capsulet  # noqa: E402

RELEASE = Path(__file__).parents[3] / "release.toml"


def release_value(table: str, key: str) -> str:
    """A string value from release.toml, without a TOML parser on Python 3.10."""
    current = None
    for line in RELEASE.read_text(encoding="utf-8").splitlines():
        header = re.fullmatch(r"\[([^\]]+)\]", line.strip())
        if header:
            current = header.group(1)
            continue
        assignment = re.fullmatch(rf'{re.escape(key)}\s*=\s*"([^"]*)"\s*(#.*)?', line.strip())
        if current == table and assignment:
            return assignment.group(1)
    raise AssertionError(f"release.toml has no {table}.{key}")


class ReleaseIdentityTests(unittest.TestCase):
    def test_the_sdk_is_the_release_version(self):
        self.assertEqual(capsulet.__version__, release_value("product", "version"))

    def test_the_sdk_supports_the_api_versions_the_release_names(self):
        self.assertEqual(capsulet.SUPPORTED_API_VERSIONS, release_value("compatibility", "api"))


if __name__ == "__main__":
    unittest.main()
