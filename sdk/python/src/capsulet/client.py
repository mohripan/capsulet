"""HTTP client for deploying and operating Capsulet workflows."""

from __future__ import annotations

import json
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any

from .workflow import Workflow, WorkflowSpec


class CapsuletApiError(RuntimeError):
    """An error response returned by the Capsulet API."""

    def __init__(self, status: int, message: str, code: str | None = None) -> None:
        super().__init__(f"{code}: {message}" if code else message)
        self.status = status
        self.code = code


class CapsuletClient:
    """Dependency-free synchronous Capsulet API client."""

    def __init__(self, base_url: str = "http://127.0.0.1:8080", *, timeout: float = 30.0) -> None:
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout

    def _request(self, method: str, path: str, body: dict[str, Any] | None = None) -> Any:
        data = json.dumps(body).encode() if body is not None else None
        request = urllib.request.Request(
            f"{self.base_url}{path}",
            data=data,
            method=method,
            headers={"content-type": "application/json"},
        )
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                payload = response.read()
                return json.loads(payload) if payload else None
        except urllib.error.HTTPError as error:
            try:
                payload = json.loads(error.read())
            except (json.JSONDecodeError, UnicodeDecodeError):
                payload = {"message": error.reason}
            raise CapsuletApiError(error.code, payload.get("message", error.reason), payload.get("code")) from error

    def deploy(self, workflow: Workflow | WorkflowSpec) -> dict[str, Any]:
        """Compile and upsert every job followed by its workflow definition."""

        spec = workflow.build() if isinstance(workflow, Workflow) else workflow
        for step in spec.steps:
            self._request(
                "POST",
                "/v1/job-definitions",
                {
                    "id": step.job_definition_id,
                    "name": step.name,
                    "runtime_image": step.runtime_image,
                    "python_script": step.python_script,
                    "retry_max_attempts": 1,
                    "retry_delay_seconds": 0,
                },
            )
        return self._request("POST", "/v1/workflows", spec.workflow_request())

    def trigger(self, automation_id: str) -> dict[str, Any]:
        return self._request("POST", f"/v1/automations/{automation_id}/trigger", {})

    def create_automation(
        self,
        workflow_id: str,
        *,
        name: str | None = None,
        automation_id: str | None = None,
        input: dict[str, Any] | None = None,
    ) -> dict[str, Any]:
        """Create an enabled manual automation for a deployed workflow."""

        body: dict[str, Any] = {
            "name": name or f"Run {workflow_id}",
            "workflow_id": workflow_id,
            "status": "enabled",
            "trigger_kind": "manual",
            "job_input": input or {},
        }
        if automation_id is not None:
            body["id"] = automation_id
        return self._request("POST", "/v1/automations", body)

    def workflow_run(self, run_id: str) -> dict[str, Any]:
        return self._request("GET", f"/v1/workflow-runs/{run_id}")

    def wait_for_workflow_run(self, run_id: str, *, timeout: float = 300.0, interval: float = 1.0) -> dict[str, Any]:
        deadline = time.monotonic() + timeout
        terminal = {"succeeded", "failed", "cancelled", "timed_out", "removed"}
        while time.monotonic() < deadline:
            run = self.workflow_run(run_id)
            if run["status"] in terminal:
                return run
            time.sleep(interval)
        raise TimeoutError(f"workflow run {run_id} did not finish within {timeout:g}s")

    def artifacts(self, job_run_id: str) -> list[dict[str, Any]]:
        return self._request("GET", f"/v1/jobs/runs/{job_run_id}/artifacts")["artifacts"]

    def download_artifact(self, job_run_id: str, artifact_id: str, destination: str | Path) -> Path:
        target = Path(destination)
        request = urllib.request.Request(
            f"{self.base_url}/v1/jobs/runs/{job_run_id}/artifacts/{artifact_id}", method="GET"
        )
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                target.write_bytes(response.read())
        except urllib.error.HTTPError as error:
            raise CapsuletApiError(error.code, error.reason) from error
        return target
