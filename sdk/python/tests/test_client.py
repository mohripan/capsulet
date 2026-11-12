import json
import os
import sys
import tempfile
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from unittest import mock

sys.path.insert(0, str(Path(__file__).parents[1] / "src"))

from capsulet import CapsuletClient, task, workflow


ARTIFACT_PATH = "/v1/jobs/runs/run_alpha/artifacts/artifact_alpha"
MOVED_ARTIFACT_PATH = "/v1/jobs/runs/run_alpha/artifacts/artifact_moved"
ARTIFACT_BYTES = b"customer,revenue\nGrace,810\n"


class RecordingHandler(BaseHTTPRequestHandler):
    requests = []
    project_ids = []
    authorizations = []

    def record(self, body):
        self.requests.append((self.path, body))
        self.project_ids.append(self.headers.get("x-capsulet-project-id"))
        self.authorizations.append(self.headers.get("authorization"))

    def do_POST(self):
        size = int(self.headers.get("content-length", "0"))
        body = json.loads(self.rfile.read(size) or b"{}")
        self.record(body)
        response = body | ({"status": "enabled"} if self.path == "/v1/workflows" else {})
        encoded = json.dumps(response).encode()
        self.send_response(201)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def do_GET(self):
        self.record(None)
        if self.path == MOVED_ARTIFACT_PATH:
            self.send_response(302)
            self.send_header("location", ARTIFACT_PATH)
            self.send_header("content-length", "0")
            self.end_headers()
            return
        self.send_response(200)
        self.send_header("content-type", "application/octet-stream")
        self.send_header("content-length", str(len(ARTIFACT_BYTES)))
        self.end_headers()
        self.wfile.write(ARTIFACT_BYTES)

    def log_message(self, *_args):
        return


class ClientTests(unittest.TestCase):
    def setUp(self):
        RecordingHandler.requests = []
        RecordingHandler.project_ids = []
        RecordingHandler.authorizations = []
        environment = mock.patch.dict(os.environ)
        environment.start()
        self.addCleanup(environment.stop)
        os.environ.pop("CAPSULET_API_TOKEN", None)
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), RecordingHandler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()

    def client(self, **options):
        return CapsuletClient(f"http://127.0.0.1:{self.server.server_port}", **options)

    def download(self, client, artifact_id):
        with tempfile.TemporaryDirectory() as directory:
            return client.download_artifact("run_alpha", artifact_id, Path(directory) / "summary.csv").read_bytes()

    def test_deploy_creates_jobs_before_workflow(self):
        @task(outputs=["raw.csv"])
        def produce():
            print("produce")

        @task
        def consume(raw):
            print(raw)

        @workflow(name="Deploy me")
        def pipeline():
            consume(produce())

        client = CapsuletClient(f"http://127.0.0.1:{self.server.server_port}")
        deployed = client.deploy(pipeline)

        self.assertEqual(
            ["/v1/job-definitions", "/v1/job-definitions", "/v1/workflows"],
            [path for path, _ in RecordingHandler.requests],
        )
        workflow_body = RecordingHandler.requests[-1][1]
        self.assertEqual(1, len(workflow_body["dependencies"]))
        self.assertEqual("deploy-me", deployed["id"])

    def test_project_context_and_manual_trigger_shape_are_explicit(self):
        client = CapsuletClient(
            f"http://127.0.0.1:{self.server.server_port}",
            project_id="project_alpha",
        )

        client.create_automation("workflow_alpha", automation_id="automation_alpha")

        self.assertEqual(["project_alpha"], RecordingHandler.project_ids)
        self.assertEqual(
            {
                "name": "Run workflow_alpha",
                "workflow_id": "workflow_alpha",
                "status": "enabled",
                "job_input": {},
                "triggers": [{"name": "manual", "kind": "manual", "config": {}}],
                "condition": {"trigger": "manual"},
                "id": "automation_alpha",
            },
            RecordingHandler.requests[0][1],
        )

    def test_explicit_token_is_sent_as_a_bearer_credential(self):
        self.client(token="token-explicit").create_automation("workflow_alpha")

        self.assertEqual(["Bearer token-explicit"], RecordingHandler.authorizations)

    def test_token_falls_back_to_the_environment(self):
        os.environ["CAPSULET_API_TOKEN"] = "token-environment"

        self.client().create_automation("workflow_alpha")

        self.assertEqual(["Bearer token-environment"], RecordingHandler.authorizations)

    def test_explicit_token_wins_over_the_environment(self):
        os.environ["CAPSULET_API_TOKEN"] = "token-environment"

        self.client(token="token-explicit").create_automation("workflow_alpha")

        self.assertEqual(["Bearer token-explicit"], RecordingHandler.authorizations)

    def test_no_token_sends_no_authorization_header(self):
        self.client().create_automation("workflow_alpha")
        os.environ["CAPSULET_API_TOKEN"] = "token-environment"
        self.client(token="").create_automation("workflow_alpha")

        self.assertEqual([None, None], RecordingHandler.authorizations)

    def test_artifact_download_sends_the_token_and_project(self):
        client = self.client(project_id="project_alpha", token="token-explicit")

        self.assertEqual(ARTIFACT_BYTES, self.download(client, "artifact_alpha"))

        self.assertEqual([ARTIFACT_PATH], [path for path, _ in RecordingHandler.requests])
        self.assertEqual(["Bearer token-explicit"], RecordingHandler.authorizations)
        self.assertEqual(["project_alpha"], RecordingHandler.project_ids)

    def test_token_is_not_forwarded_when_a_download_is_redirected(self):
        client = self.client(token="token-explicit")

        self.assertEqual(ARTIFACT_BYTES, self.download(client, "artifact_moved"))

        self.assertEqual([MOVED_ARTIFACT_PATH, ARTIFACT_PATH], [path for path, _ in RecordingHandler.requests])
        self.assertEqual(["Bearer token-explicit", None], RecordingHandler.authorizations)

    def test_token_is_kept_out_of_repr(self):
        client = self.client(project_id="project_alpha", token="token-explicit")

        self.assertNotIn("token-explicit", repr(client))
        self.assertNotIn("token-explicit", str(client))
        self.assertIn("token=<set>", repr(client))
        self.assertIn("token=None", repr(self.client()))


if __name__ == "__main__":
    unittest.main()
