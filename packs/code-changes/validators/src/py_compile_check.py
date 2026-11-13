"""The compiler: does every Python file of the change compile?

A capsulet.verifier/v1 validator. It reads the request on standard input, the
change's files under /inputs/change, writes what the compiler said to
/outputs/compile.log, and answers `builds`. It reaches no network and writes
nowhere but /outputs and /tmp.
"""

import json
import pathlib
import py_compile
import sys

PROTOCOL = "capsulet.verifier/v1"

request = json.load(sys.stdin)
change = pathlib.Path("/inputs/change")
sources = sorted(change.rglob("*.py")) if change.is_dir() else []
failed = []
log = []
for index, path in enumerate(sources):
    name = path.relative_to(change).as_posix()
    try:
        py_compile.compile(
            str(path), cfile=f"/tmp/compiled-{index}.pyc", dfile=name, doraise=True
        )
        log.append(f"ok {name}")
    except py_compile.PyCompileError as error:
        failed.append(name)
        log.append(f"FAILED {name}\n{error.msg.strip()}")
pathlib.Path("/outputs/compile.log").write_text("\n".join(log) + "\n", encoding="utf-8")


def builds():
    if not sources:
        return "cannot_decide", "the change holds no Python file to compile"
    if failed:
        return "does_not_hold", (
            f"{len(failed)} of {len(sources)} Python files do not compile: {', '.join(failed)}; "
            "the compiler's output is in compile.log"
        )
    return "holds", f"every one of the change's {len(sources)} Python files compiles"


results = []
for obligation in request["obligations"]:
    if obligation["id"] == "builds":
        outcome, reason = builds()
    else:
        outcome, reason = "cannot_decide", "this validator decides only `builds`"
    results.append({
        "obligation": obligation["id"],
        "contract": obligation["contract"],
        "outcome": outcome,
        "reason": reason,
    })
json.dump({"protocol": PROTOCOL, "results": results}, sys.stdout)
