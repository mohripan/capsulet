-- A run remembers the trace it was started in.
--
-- A run outlives the request that started it and is advanced by whichever
-- worker holds its lease, so the only way a worker's spans can join the
-- request's trace is for the run to carry the trace context. This is a W3C
-- `traceparent` and nothing else: no baggage, no tracestate, nothing a caller
-- chose to put in a header. It is NULL when tracing was not exporting.
--
-- Nothing decides from it. It is read only to parent telemetry.

ALTER TABLE ir_runs ADD COLUMN trace_parent TEXT;

ALTER TABLE ir_runs
    ADD CONSTRAINT ir_runs_trace_parent_format
    CHECK (
        trace_parent IS NULL
        OR trace_parent ~ '^[0-9a-f]{2}-[0-9a-f]{32}-[0-9a-f]{16}-[0-9a-f]{2}$'
    );
