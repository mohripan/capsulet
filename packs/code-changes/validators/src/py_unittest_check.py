"""The test runner: do the base revision's tests pass on the change, and do the
tests the change adds pass on it and fail without it?

A capsulet.verifier/v1 validator. The base revision's files are under
/inputs/base and the change's under /inputs/change. Tests are unittest tests in
files named test_*.py, found by discovery. Each run's output is written to
/outputs, so a failure carries what the runner said. It reaches no network and
writes nowhere but /outputs and /tmp.
"""

import json
import os
import pathlib
import shutil
import subprocess
import sys

PROTOCOL = "capsulet.verifier/v1"
BASE = pathlib.Path("/inputs/base")
CHANGE = pathlib.Path("/inputs/change")
OUTPUTS = pathlib.Path("/outputs")

request = json.load(sys.stdin)

# Runs in a child interpreter, rooted at the tree it tests: `list` prints every
# test id discovery finds, and `run <id>...` runs those, printing each outcome.
RUNNER = r'''
import json, sys, unittest
mode, tree, wanted = sys.argv[1], sys.argv[2], set(sys.argv[3:])
sys.path.insert(0, tree)
suite = unittest.defaultTestLoader.discover(tree, pattern="test_*.py", top_level_dir=tree)
def tests(s):
    for t in s:
        if isinstance(t, unittest.TestSuite):
            yield from tests(t)
        else:
            yield t
found = list(tests(suite))
if mode == "list":
    print(json.dumps(sorted(t.id() for t in found)))
    sys.exit(0)
outcomes = {}
class Recorded(unittest.TextTestResult):
    def addSuccess(self, test):
        super().addSuccess(test); outcomes[test.id()] = "passed"
    def addFailure(self, test, err):
        super().addFailure(test, err); outcomes[test.id()] = "failed"
    def addError(self, test, err):
        super().addError(test, err); outcomes[test.id()] = "errored"
    def addSkip(self, test, reason):
        super().addSkip(test, reason); outcomes[test.id()] = "skipped"
chosen = unittest.TestSuite(t for t in found if t.id() in wanted)
# Run against the result directly, not a TextTestRunner, which prints how long
# the run took: the log has to be the same bytes every time the tool runs.
result = Recorded(unittest.runner._WritelnDecorator(sys.stderr), True, 2)
chosen.run(result)
result.printErrors()
print(json.dumps(outcomes))
'''

ENVIRONMENT = dict(os.environ, PYTHONDONTWRITEBYTECODE="1", PYTHONHASHSEED="0")


def child(mode, tree, ids=(), log=None):
    run = subprocess.run(
        [sys.executable, "-c", RUNNER, mode, str(tree), *ids],
        capture_output=True, text=True, timeout=40, env=ENVIRONMENT, cwd=str(tree),
        stdin=subprocess.DEVNULL,
    )
    if log is not None:
        (OUTPUTS / log).write_text(run.stderr, encoding="utf-8")
    lines = run.stdout.strip().splitlines()
    return json.loads(lines[-1]) if lines else ({} if mode == "run" else [])


def listed(tree):
    return child("list", tree) if tree.is_dir() else []


base_ids = listed(BASE)
change_ids = listed(CHANGE)
new_ids = [test for test in change_ids if test not in base_ids]

existing = child("run", CHANGE, base_ids, "tests-existing.log") if base_ids else {}
on_change = child("run", CHANGE, new_ids, "tests-new-on-change.log") if new_ids else {}

# The base revision's code, with the change's test files beside it: what the
# new tests find when the change is not there.
probe = pathlib.Path("/tmp/probe")
on_base = {}
if new_ids:
    # Contents only: the inputs are read-only, and the probe is written over.
    shutil.copytree(BASE, probe, copy_function=shutil.copyfile)
    for test_file in CHANGE.rglob("test_*.py"):
        target = probe / test_file.relative_to(CHANGE)
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copyfile(test_file, target)
    on_base = child("run", probe, new_ids, "tests-new-on-base.log")


def counted(count, noun):
    return f"{count} {noun}" if count == 1 else f"{count} {noun}s"


def not_passed(ids, outcomes):
    return [test for test in ids if outcomes.get(test) != "passed"]


def existing_tests_pass():
    if not base_ids:
        return "holds", "the base revision has no tests, so none can break"
    broken = not_passed(base_ids, existing)
    if broken:
        return "does_not_hold", (
            f"{len(broken)} of the base revision's {counted(len(base_ids), 'test')} do not pass "
            f"on the change: {', '.join(broken)}; the runner's output is in tests-existing.log"
        )
    return "holds", f"the base revision's {counted(len(base_ids), 'test')} pass on the change"


def new_tests_pass_on_change():
    if not new_ids:
        return "does_not_hold", "the change adds no test"
    broken = not_passed(new_ids, on_change)
    if broken:
        return "does_not_hold", (
            f"{len(broken)} of the {counted(len(new_ids), 'test')} the change adds do not pass on it: "
            f"{', '.join(broken)}; the runner's output is in tests-new-on-change.log"
        )
    return "holds", f"the {counted(len(new_ids), 'test')} the change adds pass on it"


def new_tests_fail_on_base():
    if not new_ids:
        return "does_not_hold", "the change adds no test"
    passing = [test for test in new_ids if on_base.get(test) == "passed"]
    if passing:
        return "does_not_hold", (
            f"{', '.join(passing)} passes on the base revision too, so it does not show what "
            "the change does; the runner's output is in tests-new-on-base.log"
        )
    return "holds", f"the {counted(len(new_ids), 'test')} the change adds fail on the base revision"


DECIDES = {
    "existing-tests-pass": existing_tests_pass,
    "new-tests-pass-on-change": new_tests_pass_on_change,
    "new-tests-fail-on-base": new_tests_fail_on_base,
}
results = []
for obligation in request["obligations"]:
    decide = DECIDES.get(obligation["id"])
    if decide is None:
        outcome, reason = "cannot_decide", "this validator decides only the test obligations"
    else:
        outcome, reason = decide()
    results.append({
        "obligation": obligation["id"],
        "contract": obligation["contract"],
        "outcome": outcome,
        "reason": reason,
    })
json.dump({"protocol": PROTOCOL, "results": results}, sys.stdout)
