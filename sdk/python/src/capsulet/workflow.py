"""Decorator-based compiler for Capsulet workflow resources."""

from __future__ import annotations

import ast
import inspect
import json
import re
import textwrap
from dataclasses import dataclass
from functools import update_wrapper
from typing import Any, Callable, Generic, ParamSpec, TypeVar

P = ParamSpec("P")
R = TypeVar("R")


def _slug(value: str) -> str:
    slug = re.sub(r"[^a-z0-9]+", "-", value.strip().lower()).strip("-")
    if not slug:
        raise ValueError("name must contain a letter or number")
    return slug


@dataclass(frozen=True)
class WorkflowDependency:
    from_step_id: str
    to_step_id: str


@dataclass(frozen=True)
class CompiledStep:
    id: str
    name: str
    job_definition_id: str
    runtime_image: str
    execution_pool: str
    python_script: str
    outputs: tuple[str, ...]


@dataclass(frozen=True)
class WorkflowSpec:
    id: str
    name: str
    description: str
    steps: tuple[CompiledStep, ...]
    dependencies: tuple[WorkflowDependency, ...]

    def workflow_request(self) -> dict[str, Any]:
        return {
            "id": self.id,
            "name": self.name,
            "description": self.description,
            "steps": [
                {
                    "id": step.id,
                    "name": step.name,
                    "job_definition_id": step.job_definition_id,
                    "execution_pool": step.execution_pool,
                }
                for step in self.steps
            ],
            "dependencies": [
                {
                    "from_step_id": edge.from_step_id,
                    "to_step_id": edge.to_step_id,
                }
                for edge in self.dependencies
            ],
        }


@dataclass(frozen=True)
class TaskResult:
    """A deferred task result used to infer a dependency edge."""

    _build_token: object
    step_id: str
    outputs: tuple[str, ...]


@dataclass
class _Invocation:
    task: "Task[Any, Any]"
    step_id: str
    args: tuple[Any, ...]
    kwargs: dict[str, Any]


class _BuildContext:
    def __init__(self, workflow_id: str) -> None:
        self.workflow_id = workflow_id
        self.token = object()
        self.invocations: list[_Invocation] = []

    def invoke(self, definition: "Task[Any, Any]", args: tuple[Any, ...], kwargs: dict[str, Any]) -> TaskResult:
        index = len(self.invocations) + 1
        step_id = f"{self.workflow_id}-{_slug(definition.name)}-{index}"
        self.invocations.append(_Invocation(definition, step_id, args, kwargs))
        return TaskResult(self.token, step_id, definition.outputs)


_ACTIVE_BUILD: list[_BuildContext] = []


class Task(Generic[P, R]):
    """A Python function plus Capsulet execution metadata."""

    def __init__(
        self,
        function: Callable[P, R],
        *,
        name: str | None = None,
        outputs: tuple[str, ...] = (),
        image: str = "python:3.12-slim",
        pool: str = "mini",
    ) -> None:
        self.function = function
        self.name = name or function.__name__.replace("_", " ").title()
        self.outputs = outputs
        self.image = image
        self.pool = pool
        update_wrapper(self, function)

    def __call__(self, *args: P.args, **kwargs: P.kwargs) -> R | TaskResult:
        if _ACTIVE_BUILD:
            return _ACTIVE_BUILD[-1].invoke(self, args, kwargs)
        return self.function(*args, **kwargs)


class Workflow:
    """A decorated function that can compile and deploy a workflow DAG."""

    def __init__(self, function: Callable[[], Any], *, name: str | None = None, description: str = "") -> None:
        self.function = function
        self.name = name or function.__name__.replace("_", " ").title()
        self.description = description
        self.id = _slug(self.name)
        update_wrapper(self, function)

    def __call__(self) -> WorkflowSpec:
        return self.build()

    def build(self) -> WorkflowSpec:
        context = _BuildContext(self.id)
        _ACTIVE_BUILD.append(context)
        try:
            self.function()
        finally:
            _ACTIVE_BUILD.pop()
        steps = tuple(_compile_invocation(context, invocation) for invocation in context.invocations)
        dependencies: list[WorkflowDependency] = []
        for invocation in context.invocations:
            for result in _task_results((*invocation.args, *invocation.kwargs.values())):
                if result._build_token is not context.token:
                    raise ValueError("task result belongs to a different workflow build")
                edge = WorkflowDependency(result.step_id, invocation.step_id)
                if edge not in dependencies:
                    dependencies.append(edge)
        return WorkflowSpec(self.id, self.name, self.description, steps, tuple(dependencies))


def _task_results(values: tuple[Any, ...]):
    for value in values:
        if isinstance(value, TaskResult):
            yield value
        elif isinstance(value, (list, tuple, set)):
            yield from _task_results(tuple(value))
        elif isinstance(value, dict):
            yield from _task_results(tuple(value.values()))


def _function_source(function: Callable[..., Any]) -> str:
    module = ast.parse(textwrap.dedent(inspect.getsource(function)))
    node = next((item for item in module.body if isinstance(item, (ast.FunctionDef, ast.AsyncFunctionDef))), None)
    if node is None:
        raise ValueError(f"cannot locate source for task {function.__name__}")
    node.decorator_list = []
    ast.fix_missing_locations(node)
    return ast.unparse(node)


def _argument_literal(value: Any, token: object) -> str:
    if isinstance(value, TaskResult):
        if value._build_token is not token:
            raise ValueError("task result belongs to a different workflow build")
        if not value.outputs:
            raise ValueError(f"upstream task {value.step_id} must declare at least one output")
        return repr(f"/capsulet/inputs/{value.step_id}/{value.outputs[0]}")
    try:
        json.dumps(value)
    except TypeError as error:
        raise TypeError(f"task arguments must be JSON-compatible or task results: {value!r}") from error
    return repr(value)


def _compile_invocation(context: _BuildContext, invocation: _Invocation) -> CompiledStep:
    positional = [_argument_literal(value, context.token) for value in invocation.args]
    keyword = [f"{key}={_argument_literal(value, context.token)}" for key, value in invocation.kwargs.items()]
    call = ", ".join([*positional, *keyword])
    script = f"{_function_source(invocation.task.function)}\n\nif __name__ == '__main__':\n    {invocation.task.function.__name__}({call})\n"
    return CompiledStep(
        id=invocation.step_id,
        name=invocation.task.name,
        job_definition_id=f"job-{invocation.step_id}",
        runtime_image=invocation.task.image,
        execution_pool=invocation.task.pool,
        python_script=script,
        outputs=invocation.task.outputs,
    )


def task(
    function: Callable[P, R] | None = None,
    *,
    name: str | None = None,
    outputs: list[str] | tuple[str, ...] = (),
    image: str = "python:3.12-slim",
    pool: str = "mini",
):
    """Decorate a function as a reusable Capsulet task."""

    def decorate(target: Callable[P, R]) -> Task[P, R]:
        return Task(target, name=name, outputs=tuple(outputs), image=image, pool=pool)

    return decorate(function) if function is not None else decorate


def workflow(
    function: Callable[[], Any] | None = None,
    *,
    name: str | None = None,
    description: str = "",
):
    """Decorate a function as a compilable Capsulet workflow."""

    def decorate(target: Callable[[], Any]) -> Workflow:
        return Workflow(target, name=name, description=description)

    return decorate(function) if function is not None else decorate
