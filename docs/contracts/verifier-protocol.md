# Verifier Protocol

Status: experimental. The contract between a host and a validator, `capsulet.verifier/v1`, as
[ADR 0021](../adr/0021-the-verifier-protocol-boundary.md#4-what-a-validator-receives-and-returns)
decided it and `crates/verifier` implements it. The protocol version is governed by the
[stability policy](stability-and-versioning.md): a field added is compatible, and any other change is
`v2`.

## One run

1. The host checks the validator's **registration** before anything runs.
2. It writes the **inputs** into `inputs/`, read-only, and digests each one.
3. It starts the validator under its limits, with one **request** on standard input.
4. The validator decides each obligation, writes anything it produces under `outputs/`, and writes one
   **response** on standard output.
5. The host checks the inputs are still the bytes it wrote, reads the response, holds every requested
   obligation to exactly one result, digests the outputs, and builds the verifier record.

A validator finds its directories in `CAPSULET_VERIFIER_INPUTS` and `CAPSULET_VERIFIER_OUTPUTS`, or at
`/inputs` and `/outputs` in a container. `capsulet_verifier::sdk::serve` does the reading and writing
for a validator written in Rust.

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
| `bad_input` | an input name is not a plain file name |
| `transport_failed` | the validator could not be started |
| `timed_out` | it ran past `timeout_ms` and was stopped |
| `crashed` | it exited non-zero |
| `response_too_large` | the response is over 256 KiB |
| `malformed_response` | the response is not a protocol response |
| `protocol_mismatch` | it answered in another protocol version |
| `missing_result`, `unexpected_result`, `duplicate_result` | it did not answer each requested obligation exactly once |
| `inputs_modified` | an input changed, or a file appeared beside them |
| `outputs_too_large` | the outputs exceed `max_output_bytes` |

## Not yet

- **Container isolation.** A read-only root, no network unless granted, and memory and process limits
  are the container transport's, and only the process transport exists so far. Until the container one
  does, nothing a validator decides discharges an obligation.
- **Re-running a replayable tool on replay.**
- **The Python SDK.**
- **The `capsulet verifier test` harness.**

All four are tracked in #155.
