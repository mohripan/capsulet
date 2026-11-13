import io
import json
import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parents[1] / "src"))

from capsulet import verifier

# The same protocol fixture the Rust SDK and host are tested against, so the two
# SDKs cannot drift apart without one of them failing.
FIXTURE = Path(__file__).parents[3] / "crates" / "verifier" / "tests" / "fixtures" / "protocol-v1.json"


class VerifierProtocolTests(unittest.TestCase):
    def setUp(self):
        self.fixture = json.loads(FIXTURE.read_text(encoding="utf-8"))

    def test_reads_the_shared_request_and_writes_the_shared_response(self):
        request = verifier.read_request(io.StringIO(json.dumps(self.fixture["request"])))
        self.assertEqual(self.fixture["request"], request)

        builds, tests = request["obligations"]
        results = [
            verifier.result(builds, verifier.HOLDS, "built clean"),
            verifier.result(
                tests,
                verifier.CANNOT_DECIDE,
                "the test runner needs a database this run was not given",
            ),
        ]
        written = io.StringIO()
        verifier.respond(results, written)
        self.assertEqual(self.fixture["response"], json.loads(written.getvalue()))

    def test_refuses_a_request_in_another_protocol_or_not_a_request_at_all(self):
        for text in [
            json.dumps({**self.fixture["request"], "protocol": "capsulet.verifier/v0"}),
            json.dumps({"protocol": verifier.PROTOCOL}),
            "not json",
        ]:
            with self.assertRaises(verifier.ProtocolError, msg=text):
                verifier.read_request(io.StringIO(text))

    def test_a_result_has_a_known_outcome_and_says_why(self):
        obligation = self.fixture["request"]["obligations"][0]
        with self.assertRaises(ValueError):
            verifier.result(obligation, "probably", "it looks fine")
        with self.assertRaises(ValueError):
            verifier.result(obligation, verifier.HOLDS, "   ")


if __name__ == "__main__":
    unittest.main()
