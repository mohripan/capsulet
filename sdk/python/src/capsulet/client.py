"""HTTP client for deploying and operating Capsulet workflows."""

from __future__ import annotations

import json
import time
import urllib.error
import urllib.parse
import urllib.request
from pathlib import Path
from typing import Any

from .workflow import Workflow, WorkflowSpec


# Experimental handwritten transport map. OpenAPI remains the authority.
CLIENT_OPERATIONS: dict[str, dict[str, Any]] = {
    "deploy_job_definition": {"method": "POST", "path": "/v1/job-definitions", "operation_id": "createJobDefinition", "request_schema": "CreateJobDefinitionRequest", "response_schema": "JobDefinitionResponse", "project_context": True},
    "deploy_workflow": {"method": "POST", "path": "/v1/workflows", "operation_id": "createWorkflow", "request_schema": "CreateWorkflowRequest", "response_schema": "WorkflowResponse", "project_context": True},
    "trigger": {"method": "POST", "path": "/v1/automations/{id}/trigger", "operation_id": "triggerAutomation", "request_schema": None, "response_schema": "WorkflowRunResponse", "project_context": True},
    "create_automation": {"method": "POST", "path": "/v1/automations", "operation_id": "createAutomation", "request_schema": "CreateAutomationRequest", "response_schema": "AutomationResponse", "project_context": True},
    "workflow_run": {"method": "GET", "path": "/v1/workflow-runs/{id}", "operation_id": "getWorkflowRun", "request_schema": None, "response_schema": "WorkflowRunResponse", "project_context": True},
    "artifacts": {"method": "GET", "path": "/v1/jobs/runs/{id}/artifacts", "operation_id": "listJobArtifacts", "request_schema": None, "response_schema": "ListArtifactsResponse", "project_context": True},
    "download_artifact": {"method": "GET", "path": "/v1/jobs/runs/{id}/artifacts/{artifact_id}", "operation_id": "downloadJobArtifact", "request_schema": None, "response_schema": "BinaryResponse", "project_context": True},
}


class CapsuletApiError(RuntimeError):
    """An error response returned by the Capsulet API."""

    def __init__(self, status: int, message: str, code: str | None = None) -> None:
        super().__init__(f"{code}: {message}" if code else message)
        self.status = status
        self.code = code


class CapsuletClient:
    """Dependency-free synchronous Capsulet API client."""

    def __init__(
        self,
        base_url: str = "http://127.0.0.1:8080",
        *,
        timeout: float = 30.0,
        project_id: str | None = None,
    ) -> None:
        self.base_url = base_url.rstrip("/")
        self.timeout = timeout
        self.project_id = project_id

    def _request(self, method: str, path: str, body: dict[str, Any] | None = None) -> Any:
        data = json.dumps(body).encode() if body is not None else None
        headers = {"content-type": "application/json"} if body is not None else {}
        if self.project_id:
            headers["x-capsulet-project-id"] = self.project_id
        request = urllib.request.Request(
            f"{self.base_url}{path}",
            data=data,
            method=method,
            headers=headers,
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

    def _operation(self, name: str, *, body: dict[str, Any] | None = None, **path_values: str) -> Any:
        contract = CLIENT_OPERATIONS[name]
        encoded = {key: urllib.parse.quote(value, safe="") for key, value in path_values.items()}
        path = contract["path"].format(**encoded)
        return self._request(contract["method"], path, body)

    def deploy(self, workflow: Workflow | WorkflowSpec) -> dict[str, Any]:
        """Compile and upsert every job followed by its workflow definition."""

        spec = workflow.build() if isinstance(workflow, Workflow) else workflow
        for step in spec.steps:
            self._operation(
                "deploy_job_definition",
                body={
                    "id": step.job_definition_id,
                    "name": step.name,
                    "runtime_image": step.runtime_image,
                    "python_script": step.python_script,
                    "retry_max_attempts": 1,
                    "retry_delay_seconds": 0,
                },
            )
        return self._operation("deploy_workflow", body=spec.workflow_request())

    def trigger(self, automation_id: str) -> dict[str, Any]:
        return self._operation("trigger", id=automation_id)

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
            "job_input": input or {},
            "triggers": [{"name": "manual", "kind": "manual", "config": {}}],
            "condition": {"trigger": "manual"},
        }
        if automation_id is not None:
            body["id"] = automation_id
        return self._operation("create_automation", body=body)

    def workflow_run(self, run_id: str) -> dict[str, Any]:
        return self._operation("workflow_run", id=run_id)

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
        return self._operation("artifacts", id=job_run_id)["artifacts"]

    def download_artifact(self, job_run_id: str, artifact_id: str, destination: str | Path) -> Path:
        target = Path(destination)
        contract = CLIENT_OPERATIONS["download_artifact"]
        path = contract["path"].format(
            id=urllib.parse.quote(job_run_id, safe=""),
            artifact_id=urllib.parse.quote(artifact_id, safe=""),
        )
        headers = {"x-capsulet-project-id": self.project_id} if self.project_id else {}
        request = urllib.request.Request(f"{self.base_url}{path}", method=contract["method"], headers=headers)
        try:
            with urllib.request.urlopen(request, timeout=self.timeout) as response:
                target.write_bytes(response.read())
        except urllib.error.HTTPError as error:
            raise CapsuletApiError(error.code, error.reason) from error
        return target
