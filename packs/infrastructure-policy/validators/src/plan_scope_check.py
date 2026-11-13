"""The scope scanner: is everything the change touches inside its declared scope?

A capsulet.verifier/v1 validator. It reads the request on standard input, the
change's Terraform plan at /inputs/change/plan.json and the scope its request
declares at /inputs/scope.json, writes an inventory of every resource the plan
touches to /outputs/scope.log, and answers `within-declared-scope`. It reaches
no network and writes nowhere but /outputs.

A scope names resources by address, each with the actions it allows: `create`,
`update`, `delete`, `replace` or `forget`. An address ending in `*` names every
address that starts with what comes before it. A resource the plan leaves as
it is, or only reads, is not touched.
"""

import json
import pathlib
import sys

PROTOCOL = "capsulet.verifier/v1"
OBLIGATION = "within-declared-scope"
ACTIONS = {"create", "update", "delete", "replace", "forget"}


def read(path, what):
    if not path.is_file():
        raise ValueError(f"the change has no {what} at {path.relative_to('/inputs')}")
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except (UnicodeDecodeError, json.JSONDecodeError) as error:
        raise ValueError(f"{path.relative_to('/inputs')} is not JSON: {error}") from None


def touched(plan):
    if not isinstance(plan, dict) or not isinstance(plan.get("format_version"), str):
        raise ValueError("change/plan.json is not a Terraform plan: it has no format_version")
    found = []
    for resource in plan.get("resource_changes") or []:
        change = resource.get("change") if isinstance(resource, dict) else None
        address = resource.get("address") if isinstance(resource, dict) else None
        if not isinstance(change, dict) or not isinstance(address, str):
            raise ValueError("change/plan.json has a resource change with no address or change")
        actions = set(change.get("actions") or [])
        if resource.get("mode") != "managed" or actions <= {"no-op", "read"}:
            continue
        action = "replace" if {"create", "delete"} <= actions else "/".join(sorted(actions))
        found.append((address, action))
    return sorted(found)


def entries(scope):
    listed = scope.get("resources") if isinstance(scope, dict) else None
    if not isinstance(listed, list):
        raise ValueError("scope.json lists no resources")
    for entry in listed:
        if not isinstance(entry, dict):
            raise ValueError("a scope entry is not an object")
        address, actions = entry.get("address"), entry.get("actions")
        if not isinstance(address, str) or not address:
            raise ValueError("a scope entry names no address")
        if not isinstance(actions, list) or not set(actions) <= ACTIONS:
            raise ValueError(f"the scope entry for {address} allows an action that is not one of {', '.join(sorted(ACTIONS))}")
    return listed


def counted(number, noun):
    return f"{number} {noun}" if number == 1 else f"{number} {noun}s"


def names(entry, address):
    pattern = entry["address"]
    if pattern.endswith("*"):
        return address.startswith(pattern[:-1])
    return address == pattern


def placed(address, action, scope):
    naming = [entry for entry in scope if names(entry, address)]
    if not naming:
        return "the scope does not name it"
    allowed = sorted({allowed for entry in naming for allowed in entry["actions"]})
    if action not in allowed:
        return f"the scope allows only {', '.join(allowed) or 'no action'}"
    return None


def within_declared_scope():
    try:
        changes = touched(read(pathlib.Path("/inputs/change/plan.json"), "plan"))
        scope = entries(read(pathlib.Path("/inputs/scope.json"), "declared scope"))
    except ValueError as error:
        pathlib.Path("/outputs/scope.log").write_text(f"{error}\n", encoding="utf-8")
        return "cannot_decide", str(error)
    log, outside = [], []
    for address, action in changes:
        why = placed(address, action, scope)
        if why is None:
            log.append(f"{action} {address}: in scope")
        else:
            log.append(f"{action} {address}: OUTSIDE, {why}")
            outside.append(f"{action} {address}: {why}")
    if not changes:
        log.append("the change touches no resource")
    pathlib.Path("/outputs/scope.log").write_text("\n".join(log) + "\n", encoding="utf-8")
    if outside:
        return "does_not_hold", (
            f"{len(outside)} of {counted(len(changes), 'resource change')} "
            f"{'is' if len(outside) == 1 else 'are'} outside the declared scope: "
            f"{'; '.join(outside)}; the inventory is in scope.log"
        )
    if not changes:
        return "holds", "the change touches no resource"
    return "holds", f"every resource it touches is inside the declared scope ({counted(len(changes), 'resource change')})"


request = json.load(sys.stdin)
results = []
for obligation in request["obligations"]:
    if obligation["id"] == OBLIGATION:
        outcome, reason = within_declared_scope()
    else:
        outcome, reason = "cannot_decide", f"this validator decides only `{OBLIGATION}`"
    results.append({
        "obligation": obligation["id"],
        "contract": obligation["contract"],
        "outcome": outcome,
        "reason": reason,
    })
json.dump({"protocol": PROTOCOL, "results": results}, sys.stdout)
