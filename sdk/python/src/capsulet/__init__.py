"""Capsulet's dependency-free Python workflow authoring API."""

from ._version import SUPPORTED_API_VERSIONS, __version__
from .client import CLIENT_OPERATIONS, CapsuletApiError, CapsuletClient
from .workflow import TaskResult, Workflow, WorkflowSpec, task, workflow

__all__ = [
    "SUPPORTED_API_VERSIONS",
    "__version__",
    "CapsuletApiError",
    "CapsuletClient",
    "CLIENT_OPERATIONS",
    "TaskResult",
    "Workflow",
    "WorkflowSpec",
    "task",
    "workflow",
]
