# Verifier Protocol

Status: experimental. The contract between a host and a validator, `capsulet.verifier/v1`, as
[ADR 0021](../adr/0021-the-verifier-protocol-boundary.md#4-what-a-validator-receives-and-returns)
decided it and `crates/verifier` implements it. The protocol version is governed by the
[stability policy](stability-and-versioning.md): a field added is compatible, and any other change is
`v2`.

## One run

1. The host checks the validator's **registration** before anything runs.
2. It writes the **inputs** into `inputs/`, read-only, and digests each one. An input's name is a
   relative path, so a code change can be two trees, `base/calc.py` beside `change/calc.py`. No part
   of a path may be empty, `.`, `..` or hidden, and none may hold `\` or `:`, so nothing reaches
   outside the inputs.
3. It starts the validator under its limits, with one **request** on standard input.
4. The validator decides each obligation, writes anything it produces under `outputs/`, and writes one
   **response** on standard output.
5. The host checks the inputs are still the bytes it wrote, reads the response, holds every requested
   obligation to exactly one result, digests the outputs, and builds the verifier record.

A validator finds its directories in `CAPSULET_VERIFIER_INPUTS` and `CAPSULET_VERIFIER_OUTPUTS`, or at
`/inputs` and `/outputs` in a container. `capsulet_verifier::sdk::serve` does the reading and writing
for a validator written in Rust, and `capsulet.verifier.serve` for one in Python. Both are tested
against the same protocol fixture, `crates/verifier/tests/fixtures/protocol-v1.json`.

## Request

```json
{
  "protocol": "capsulet.verifier/v1",
  "obligations": [
    { "id": "builds", "contract": "patch-compiles", "statement": "the patch compiles" }
  ],
  "parameters": { "profile": "release" },
  "inputs": [
    { "name": "patch.diff", "digest": "sha256:…", "media_type": "text/x-diff" }
  ]
}
```

A validator receives nothing else: no credentials, no database, and no network unless its
registration grants it.

## Response

```json
{
  "protocol": "capsulet.verifier/v1",
  "results": [
    { "obligation": "builds", "contract": "patch-compiles", "outcome": "holds", "reason": "built clean" }
  ]
}
```

`outcome` is `holds`, `does_not_hold` or `cannot_decide`. A `cannot_decide` stays residual and is never
read as either answer. A response is at most 256 KiB. Anything larger belongs in the outputs, where
the host digests it.

## Registration

| Field | Says |
| --- | --- |
| `basis` | `replayable_tool`, or `model_judgment` with its `family`, whether it is `independent_of_proposer`, and its `calibration` record |
| `environment` | `container` with an `image` pinned by `@sha256:` digest, or `process` for the development harness only |
| `capabilities` | `network`, `model`. A replayable tool may hold neither, and a model must hold `model` |
| `limits` | `timeout_ms`, `memory_mb`, and `max_output_bytes` for the outputs |

The basis comes from the registration and never from the validator's own response.

## What a result does to an obligation

| Validator | `holds` | `does_not_hold` | `cannot_decide` |
| --- | --- | --- | --- |
| replayable tool, isolated | discharged, basis `replayable_tool`, resting on the outputs | failed, owned as the obligation says | residual |
| model, isolated | endorsed | disputed | residual |
| any, unisolated (`process`) | residual, saying it ran unisolated | residual | residual |

The verifier record's trust is the registration's: a declared oracle for a tool, saying whether it ran
isolated, and `model_judgment` for a model. The seal refuses a certificate in which a model discharges
anything ([IR and certificates](ir-and-certificates.md)).

## Failures

Each failure is typed, owned by `verifier`, and decides nothing.

| Code | When |
| --- | --- |
| `registration_refused` | the registration cannot be run |
| `bad_input` | an input name is not a relative path inside the inputs, or two inputs collide |
| `transport_failed` | the validator could not be started |
| `timed_out` | it ran past `timeout_ms` and was stopped |
| `crashed` | it exited non-zero |
| `response_too_large` | the response is over 256 KiB |
| `malformed_response` | the response is not a protocol response |
| `protocol_mismatch` | it answered in another protocol version |
| `missing_result`, `unexpected_result`, `duplicate_result` | it did not answer each requested obligation exactly once |
| `inputs_modified` | an input changed, or a file appeared beside them |
| `outputs_too_large` | the outputs exceed `max_output_bytes` |

## Isolation

`ContainerTransport` runs the image the registration pins by digest, optionally with a `command`, as
`docker run` with:

- `--network none`, unless the registration grants `network`, which a replayable tool never has;
- `--read-only` and a 64 MiB `/tmp`;
- the inputs bind-mounted read-only at `/inputs`, and only `/outputs` writable;
- `--cap-drop ALL`, `no-new-privileges`, `--memory` with no swap beyond it, `--pids-limit 128` and one
  CPU;
- a name of its own, so a run past its time limit is killed, and `--rm` either way.

Exit codes 125 to 127 are Docker's own failures, such as no image or no daemon, and are reported as
`transport_failed`, never as the validator crashing. A validator killed for its memory exits 137 and is
`crashed`.

The `verifier-isolation` gate checks all of this from inside a real container, with a probe in a
pinned busybox image that tries the network, its inputs and its root and reports what happened. It
also checks that a validator past its memory or its time is stopped, that no container is left
behind, and that a second run reaches the same outputs.

## Running a tool again

`rerun` runs an isolated replayable tool again with the same inputs and compares the new record with
the one a certificate holds: identity, environment, inputs, outputs by digest, and verdict. A model is
not re-run, since its word is not re-derivable. Neither is an unisolated run, which decided nothing to
compare.

The host keeps each output's bytes beside its digest, so a certificate's bundle can carry what a tool
produced: the compiler's error, the failing test's traceback. `capsulet pack replay <bundle> <pack>`
runs every replayable tool a certificate records again, from its bundle, in the container its record
pins. The inputs are the evidence the certificate cites under `inputs/`, in the order it cites them,
and what each tool is asked is what its pack says it decides. It reports each tool as agreeing with its
record or differing, says whether the certificate's verdict reproduces offline, and exits non-zero
when either fails. It is a command of its own, beside `capsulet-replay`, because it needs Docker and
`capsulet-replay` reaches no daemon.

## The development harness

`capsulet verifier test <registration.json> <cases.json>` runs one validator against cases, without a
stack. It uses the transport its registration names: a `container` image runs isolated, and a `process`
runs unisolated, which the report says.

```json
{
  "cases": [
    {
      "name": "builds",
      "obligations": [{ "id": "builds", "contract": "patch-compiles", "statement": "the patch compiles" }],
      "inputs": [{ "name": "patch.diff", "path": "fixtures/patch.diff", "media_type": "text/x-diff" }],
      "expect": { "builds": "holds" }
    },
    {
      "name": "a slow build is stopped",
      "obligations": [{ "id": "builds", "contract": "patch-compiles", "statement": "the patch compiles" }],
      "inputs": [{ "name": "patch.diff", "text": "--- a\n+++ b\n" }],
      "expect_failure": "timed_out"
    }
  ]
}
```

An input comes from exactly one of `path`, which resolves beside the cases file, and `text`. A case
passes when every expected outcome comes back, or when the run fails with the expected failure code.
The command prints one line per case and exits non-zero when any fails.

## Not yet

