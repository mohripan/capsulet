# Code changes

A reference domain pack ([ADR 0021](../../docs/adr/0021-the-verifier-protocol-boundary.md),
[ADR 0022](../../docs/adr/0022-signed-versioned-domain-packs.md)) for a workload where most
obligations are decided by tools: does a change compile, do the base revision's tests still pass,
and do the tests it adds pass on it and fail without it. Whether it does what was asked is left to a
person.

| Path | Holds |
| --- | --- |
| `obligations/code-changes.json` | the obligations, and what may discharge each |
| `validators/py-compile.json` | the compiler: a replayable tool in a pinned Python image |
| `validators/py-unittest.json` | the test runner: a replayable tool in the same image |
| `validators/src/` | the two validators' scripts, as each registration runs them |
| `cases/` | the cases that show each validator works |
| `fixtures/` | a small project in four changes, written for this pack |

## The obligations

| Obligation | Decided by | If it does not hold |
| --- | --- | --- |
| `builds` | `py-compile`: every Python file of the change compiles | the change is `rejected`, owned by the proposer |
| `existing-tests-pass` | `py-unittest`: every test of the base revision passes on the change | `rejected`, owned by the proposer |
| `new-tests-pass-on-change` | `py-unittest`: every test the change adds passes on it | `rejected`, owned by the proposer |
| `new-tests-fail-on-base` | `py-unittest`: every test the change adds fails on the base revision, so it shows what the change does | `rejected`, owned by the proposer |
| `does-what-was-asked` | nobody: it stays residual for a holder of `review:code` | — |

The tool obligations accept only `replayable_tool`. `does-what-was-asked` accepts only
`human_review` under `review:code`, so a change that passes every tool is `conditional` until
someone decides it.

## The inputs

A change is two trees, at their paths: the base revision under `base/` and the change under
`change/`, for example `base/calc.py` and `change/test_calc.py`. Tests are `unittest` tests in
files named `test_*.py`. A test the change adds is one whose id is not in the base revision.

## What a check leaves behind

Each validator runs in a container from `python@sha256:9ef1a599…`, with no network, a read-only
root and inputs, and only `/outputs` writable. What the tools said is in their outputs:
`compile.log`, and `tests-existing.log`, `tests-new-on-change.log` and `tests-new-on-base.log`. The
host digests each one into the tool's record, and the certificate cites it as evidence, so a
`rejected` change carries the compiler's error or the failing test's traceback. The logs leave out
timings, so a second run produces the same bytes.

## Replaying

```sh
capsulet pack replay certificate-bundle.json packs/code-changes
```

This runs each tool the certificate records again, in the container its record pins, with the
inputs the bundle carries. It reports whether each tool reaches its record (the same inputs,
environment, outputs and verdict), and replays the certificate offline. It exits non-zero when
anything differs. It needs Docker.

## Using it

```sh
capsulet pack test packs/code-changes
capsulet pack sign packs/code-changes --key publisher.key.json
```

The pack is committed unsigned, like the other reference packs.
