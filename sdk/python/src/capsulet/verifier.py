"""Writing a validator in Python, for the ``capsulet.verifier/v1`` protocol.

A validator reads one request on standard input, decides each obligation, writes
anything it produced under the outputs directory, and writes one response on
standard output (``docs/contracts/verifier-protocol.md``). :func:`serve` does the
reading and writing; the validator supplies the deciding::

    from capsulet import verifier

    def decide(request, inputs, outputs):
        return [
            verifier.result(obligation, verifier.CANNOT_DECIDE, "this validator decides nothing")
            for obligation in request["obligations"]
        ]

    verifier.serve(decide)

What a validator says is its own: the host digests its inputs and outputs, and
its registration, not its response, says what kind of checker it is.
"""

import json
import os
import sys
from pathlib import Path

PROTOCOL = "capsulet.verifier/v1"

HOLDS = "holds"
DOES_NOT_HOLD = "does_not_hold"
CANNOT_DECIDE = "cannot_decide"
OUTCOMES = (HOLDS, DOES_NOT_HOLD, CANNOT_DECIDE)


class ProtocolError(ValueError):
    """The request is not one this protocol version can answer."""


def inputs_directory():
    """Where the inputs are: ``CAPSULET_VERIFIER_INPUTS``, or ``/inputs`` in a container."""
    return Path(os.environ.get("CAPSULET_VERIFIER_INPUTS", "/inputs"))


def outputs_directory():
    """Where outputs go: ``CAPSULET_VERIFIER_OUTPUTS``, or ``/outputs`` in a container."""
    return Path(os.environ.get("CAPSULET_VERIFIER_OUTPUTS", "/outputs"))


def read_request(stream=None):
    """Reads the request a host sent, and refuses one in another protocol."""
    stream = sys.stdin if stream is None else stream
    try:
        request = json.loads(stream.read())
    except json.JSONDecodeError as error:
        raise ProtocolError(f"the request is not JSON: {error}") from error
    if not isinstance(request, dict) or request.get("protocol") != PROTOCOL:
        found = request.get("protocol") if isinstance(request, dict) else None
        raise ProtocolError(f"this validator speaks {PROTOCOL}, not {found!r}")
    for field in ("obligations", "inputs"):
        if not isinstance(request.get(field), list):
            raise ProtocolError(f"the request has no {field} list")
    return request


def result(obligation, outcome, reason):
    """One result for one requested obligation, which must say why."""
    if outcome not in OUTCOMES:
        raise ValueError(f"an outcome is one of {', '.join(OUTCOMES)}, not {outcome!r}")
    if not str(reason).strip():
        raise ValueError("a result has to say why")
    return {
        "obligation": obligation["id"],
        "contract": obligation["contract"],
        "outcome": outcome,
        "reason": str(reason),
    }


def respond(results, stream=None):
    """Writes the response for ``results``."""
    stream = sys.stdout if stream is None else stream
    stream.write(json.dumps({"protocol": PROTOCOL, "results": list(results)}))
    stream.flush()


def serve(decide):
    """Reads the request, decides it with ``decide(request, inputs, outputs)``, and
    writes the response. A request that cannot be read exits with status 2, which
    the host reports as a crash rather than a verdict."""
    try:
        request = read_request()
    except ProtocolError as error:
        print(error, file=sys.stderr)
        sys.exit(2)
    respond(decide(request, inputs_directory(), outputs_directory()))
