"""The Kind smoke: a Capsulet install exercised from inside its own cluster.

`capsulet-xtask`'s `kind` gate runs this as a Job in the release's namespace, in
the Capsulet API image, which carries Python. Every request goes to a Service by
its cluster DNS name, so what passes is the Kubernetes deployment: Services,
probes, the worker's Kubernetes runner, the graph worker, object storage and the
database the chart installed. Nothing here can reach a Compose stack.

Credentials and expectations come from the environment, set from a Secret the
gate generated for this cluster alone:

    CAPSULET_SMOKE_RELEASE          the Helm release name
    CAPSULET_SMOKE_VERSION          the version release.toml names
    CAPSULET_SMOKE_ADMIN_TOKEN      a platform administrator
    CAPSULET_SMOKE_OPERATOR_TOKEN   an operator of kind-alpha only
    CAPSULET_SMOKE_VIEWER_TOKEN     a viewer of kind-alpha only
    CAPSULET_SMOKE_DASHBOARD_USER   the dashboard's bootstrap login
    CAPSULET_SMOKE_DASHBOARD_PASSWORD
"""

import json
import os
import sys
import time
import urllib.error
import urllib.request

RELEASE = os.environ["CAPSULET_SMOKE_RELEASE"]
VERSION = os.environ["CAPSULET_SMOKE_VERSION"]
ADMIN = os.environ["CAPSULET_SMOKE_ADMIN_TOKEN"]
OPERATOR = os.environ["CAPSULET_SMOKE_OPERATOR_TOKEN"]
VIEWER = os.environ["CAPSULET_SMOKE_VIEWER_TOKEN"]
DASHBOARD_USER = os.environ["CAPSULET_SMOKE_DASHBOARD_USER"]
DASHBOARD_PASSWORD = os.environ["CAPSULET_SMOKE_DASHBOARD_PASSWORD"]

API = f"http://{RELEASE}-api"
DASHBOARD = f"http://{RELEASE}-dashboard"
ALPHA = "kind-alpha"
BETA = "kind-beta"


def call(method, url, token=None, project=None, body=None, cookie=None, expect=(200,)):
    """One request; returns (status, parsed body, headers). Fails on any other status."""
    data = json.dumps(body).encode() if body is not None else None
    request = urllib.request.Request(url, data=data, method=method)
    if data is not None:
        request.add_header("content-type", "application/json")
    if token:
        request.add_header("authorization", f"Bearer {token}")
    if project:
        request.add_header("x-capsulet-project-id", project)
    if cookie:
        request.add_header("cookie", cookie)
    try:
        with urllib.request.urlopen(request, timeout=30) as response:
            status, raw, headers = response.status, response.read(), response.headers
    except urllib.error.HTTPError as error:
        status, raw, headers = error.code, error.read(), error.headers
    text = raw.decode("utf-8", "replace")
    try:
        parsed = json.loads(text) if text.strip() else None
    except ValueError:
        parsed = text
    if status not in expect:
        raise AssertionError(f"{method} {url} answered {status}, wanted {list(expect)}: {text[:500]}")
    return status, parsed, headers


def api(method, path, token=ADMIN, project=None, body=None, expect=(200,)):
    return call(method, API + path, token=token, project=project, body=body, expect=expect)[1]


def wait_for(description, read, done, timeout=300):
    deadline = time.monotonic() + timeout
    while True:
        value = read()
        if done(value):
            return value
        if time.monotonic() > deadline:
            raise AssertionError(f"{description} did not finish within {timeout}s: {value}")
        time.sleep(2)


def passed(what):
    print(f"PASS {what}", flush=True)


def health():
    for service, path in ((API, "/healthz"), (API, "/readyz"), (DASHBOARD, "/healthz")):
        body = call("GET", service + path)[1]
        if body.get("version") != VERSION:
            raise AssertionError(f"{service}{path} reports version {body.get('version')}, release.toml says {VERSION}")
    passed(f"health: the API and the dashboard are ready and serve {VERSION}")


def bootstrap():
    """The supported bootstrap path: the administrator from the API's token
    Secret provisions the projects. The operator's and viewer's credentials
    already name kind-alpha, which makes them its members."""
    for project, name in ((ALPHA, "Kind Alpha"), (BETA, "Kind Beta")):
        api("PUT", f"/v1/projects/{project}", project=project, body={"name": name})
    me = api("GET", "/v1/auth/me", token=OPERATOR, project=ALPHA)
    if me.get("role") != "operator":
        raise AssertionError(f"the operator credential resolves to {me}")
    passed("bootstrap: the administrator provisioned kind-alpha and kind-beta through the API")


def dashboard_login():
    status, principal, headers = call(
        "POST",
        DASHBOARD + "/api/auth/login",
        body={"username": DASHBOARD_USER, "password": DASHBOARD_PASSWORD},
    )
    cookie = next(
        (value.split(";", 1)[0] for key, value in headers.items() if key.lower() == "set-cookie" and value.startswith("capsulet_session=")),
        None,
    )
    if not cookie:
        raise AssertionError("the dashboard signed in without setting a session cookie")
    me = call("GET", DASHBOARD + "/api/capsulet/v1/auth/me", cookie=cookie)[1]
    if me.get("name") != principal.get("name"):
        raise AssertionError(f"the dashboard session is {me}, not the {principal} it signed in")
    call("POST", DASHBOARD + "/api/auth/login", body={"username": DASHBOARD_USER, "password": "wrong"}, expect=(401,))
    passed(f"dashboard login: signed in as {me.get('name')} and refused a wrong password")


def job_run():
    """A project-scoped workflow operation: the administrator defines a job in
    kind-alpha, its operator runs it, and the worker executes it as a Kubernetes
    Job whose logs and artifact come back through object storage."""
    script = "\n".join(
        [
            "from pathlib import Path",
            "Path('/capsulet/artifacts').mkdir(parents=True, exist_ok=True)",
            "Path('/capsulet/artifacts/report.txt').write_text('hello from kind-alpha')",
            "print('hello from kind-alpha')",
        ]
    )
    definition = {"id": "kind_smoke_job", "name": "Kind smoke job", "python_script": script}
    exists = call("GET", API + "/v1/job-definitions/kind_smoke_job", token=ADMIN, project=ALPHA, expect=(200, 404))[0]
    if exists == 404:
        api("POST", "/v1/job-definitions", project=ALPHA, body=definition, expect=(201,))
    run = api(
        "POST",
        "/v1/jobs/runs",
        token=OPERATOR,
        project=ALPHA,
        body={"job_definition_id": "kind_smoke_job", "execution_pool": "mini"},
        expect=(200, 201, 202),
    )
    finished = wait_for(
        f"job run {run['id']}",
        lambda: api("GET", f"/v1/jobs/runs/{run['id']}", token=OPERATOR, project=ALPHA),
        lambda current: current["status"] in ("succeeded", "failed", "cancelled", "timed_out"),
    )
    if finished["status"] != "succeeded":
        raise AssertionError(f"job run {run['id']} ended {finished['status']}: {finished}")
    logs = api("GET", f"/v1/jobs/runs/{run['id']}/logs", token=OPERATOR, project=ALPHA)
    if "hello from kind-alpha" not in (logs.get("logs") or ""):
        raise AssertionError(f"job run {run['id']} logged {logs}")
    artifacts = api("GET", f"/v1/jobs/runs/{run['id']}/artifacts", token=OPERATOR, project=ALPHA)["artifacts"]
    report = next((artifact for artifact in artifacts if artifact["name"].endswith("report.txt")), None)
    if report is None:
        raise AssertionError(f"job run {run['id']} recorded no report.txt: {artifacts}")
    content = call("GET", API + f"/v1/jobs/runs/{run['id']}/artifacts/{report['id']}", token=OPERATOR, project=ALPHA)[1]
    if "hello from kind-alpha" not in str(content):
        raise AssertionError(f"artifact {report['id']} is {content!r}")
    passed(f"job run: {run['id']} ran as a Kubernetes Job in kind-alpha, with its logs and artifact")
    return run["id"]


def port(identifier, direction, value_type):
    return {"id": identifier, "direction": direction, "value_type": value_type}


def edge(identifier, source, target, value_type):
    return {"id": identifier, "sources": [dict(source, value_type=value_type)], "targets": [dict(target, value_type=value_type)]}


def agent_run():
    """A correctness-kernel operation: the graph worker compiles an agent's graph
    to a verified-computation IR definition and advances it under the kernel,
    which admits every step and has the validator decide the outcome. Nothing
    here needs a model."""
    graph = {
        "id": "kind_smoke_graph",
        "name": "Kind smoke graph",
        "transition_policy": {"mode": "static", "cycles_allowed": False},
        "nodes": [
            {"id": "normalize", "name": "Normalize", "kind": "query_normalizer",
             "ports": [port("in", "input", "user_query"), port("out", "output", "normalized_query")]},
            {"id": "prompt", "name": "Prompt", "kind": "prompt_builder",
             "ports": [port("query", "input", "normalized_query"), port("text", "output", "prompt")],
             "config": {"template": "Answer this: {{query}}"}},
            {"id": "check", "name": "Check", "kind": "validator",
             "ports": [port("text", "input", "prompt"), port("result", "output", "validation_result")],
             "config": {"rule.contains": "Answer this"}},
        ],
        "hyperedges": [
            edge("query-in", {"kind": "state_field", "field": "query"}, {"kind": "port", "node_id": "normalize", "port_id": "in"}, "user_query"),
            edge("normal", {"kind": "port", "node_id": "normalize", "port_id": "out"}, {"kind": "port", "node_id": "prompt", "port_id": "query"}, "normalized_query"),
            edge("checked", {"kind": "port", "node_id": "prompt", "port_id": "text"}, {"kind": "port", "node_id": "check", "port_id": "text"}, "prompt"),
            edge("written", {"kind": "port", "node_id": "prompt", "port_id": "text"}, {"kind": "state_field", "field": "prompt"}, "prompt"),
        ],
    }
    if call("GET", API + "/v1/graphs/kind_smoke_graph", token=ADMIN, project=ALPHA, expect=(200, 404))[0] == 404:
        api("POST", "/v1/graphs", project=ALPHA, body=graph, expect=(200, 201))
    if call("GET", API + "/v1/agents/kind_smoke_agent", token=ADMIN, project=ALPHA, expect=(200, 404))[0] == 404:
        api("POST", "/v1/agents", project=ALPHA, expect=(200, 201), body={
            "id": "kind_smoke_agent",
            "name": "Kind smoke agent",
            "graph_id": "kind_smoke_graph",
            "budget": {"max_steps": 3, "max_tokens": 1000, "max_seconds": 300, "max_cost_micros": 1000},
            "termination_conditions": ["validator_pass"],
        })
    run = api("POST", "/v1/agents/kind_smoke_agent/runs", token=OPERATOR, project=ALPHA,
              body={"initial_state": {"query": "  what   is capsulet? "}}, expect=(200, 201, 202))
    finished = wait_for(
        f"agent run {run['id']}",
        lambda: api("GET", f"/v1/agent-runs/{run['id']}", token=OPERATOR, project=ALPHA),
        lambda current: current["status"] in ("succeeded", "failed", "stopped", "cancelled"),
    )
    if finished["status"] != "succeeded":
        raise AssertionError(f"agent run {run['id']} ended {finished['status']}: {finished.get('execution')}")
    if finished["state"].get("prompt") != "Answer this: what is capsulet?":
        raise AssertionError(f"agent run {run['id']} produced {finished['state']}")
    digest = finished["execution"]["definition_digest"]
    definition = api("GET", f"/v1/ir/definitions/{digest}", token=OPERATOR, project=ALPHA)
    if definition.get("digest") != digest:
        raise AssertionError(f"the IR definition the run executed is not readable by its digest: {definition}")
    kinds = [event["kind"] for event in api("GET", f"/v1/agent-runs/{run['id']}/events", token=OPERATOR, project=ALPHA)["events"]]
    if not kinds:
        raise AssertionError(f"agent run {run['id']} logged no events")
    passed(f"kernel: agent run {run['id']} executed IR definition {digest[:19]} under the graph worker ({len(kinds)} events)")


def isolation(alpha_run):
    """Cross-project denial: alpha's members cannot select beta, cannot read
    beta's resources from alpha, and a viewer cannot start work."""
    beta = {"id": "kind_beta_job", "name": "Kind beta job", "python_script": "print('beta')"}
    if call("GET", API + "/v1/job-definitions/kind_beta_job", token=ADMIN, project=BETA, expect=(200, 404))[0] == 404:
        api("POST", "/v1/job-definitions", project=BETA, body=beta, expect=(201,))
    call("GET", API + "/v1/job-definitions", token=OPERATOR, project=BETA, expect=(403,))
    call("GET", API + "/v1/job-definitions/kind_beta_job", token=OPERATOR, project=ALPHA, expect=(404,))
    call("GET", API + f"/v1/jobs/runs/{alpha_run}", token=ADMIN, project=BETA, expect=(404,))
    call("POST", API + "/v1/jobs/runs", token=VIEWER, project=ALPHA, expect=(403,),
         body={"job_definition_id": "kind_smoke_job", "execution_pool": "mini"})
    passed("isolation: kind-alpha's members cannot reach kind-beta, beta cannot see alpha's run, and a viewer cannot run a job")


def main():
    health()
    bootstrap()
    dashboard_login()
    alpha_run = job_run()
    agent_run()
    isolation(alpha_run)
    print("PASS kind smoke", flush=True)


if __name__ == "__main__":
    try:
        main()
    except AssertionError as error:
        print(f"FAIL {error}", file=sys.stderr, flush=True)
        sys.exit(1)
