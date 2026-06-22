import json
import sys
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path

sys.path.insert(0, str(Path(__file__).parents[1] / "src"))

from capsulet import CapsuletClient, task, workflow


class RecordingHandler(BaseHTTPRequestHandler):
    requests = []

    def do_POST(self):
        size = int(self.headers.get("content-length", "0"))
        body = json.loads(self.rfile.read(size) or b"{}")
        self.requests.append((self.path, body))
        response = body | ({"status": "enabled"} if self.path == "/v1/workflows" else {})
        encoded = json.dumps(response).encode()
        self.send_response(201)
        self.send_header("content-type", "application/json")
        self.send_header("content-length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def log_message(self, *_args):
        return


class ClientTests(unittest.TestCase):
    def setUp(self):
        RecordingHandler.requests = []
        self.server = ThreadingHTTPServer(("127.0.0.1", 0), RecordingHandler)
        self.thread = threading.Thread(target=self.server.serve_forever, daemon=True)
        self.thread.start()

    def tearDown(self):
        self.server.shutdown()
        self.server.server_close()

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


if __name__ == "__main__":
    unittest.main()
