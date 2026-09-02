# IR Adapter Coverage

Status: experimental. This report says what `capsulet-ir-adapters` can carry from today's models
into the M2 verified-computation IR, and what it cannot.

## Why the report exists

A translation that loses information quietly is worse than no translation, because whoever reads the
result trusts it. Every construct below is classified, and the classification is generated from the
adapters themselves and compared against them by `crates/ir-adapters/tests/differential.rs`. A
construct that starts losing information has to be declared here first, in the same commit.

Three classifications:

- **full** — expressible in the IR with nothing lost.
- **with recorded loss** — expressible, but something the source model knew is not carried. The
  translated result carries an `AdaptationNote` saying what, so the loss travels with the data
  rather than living only in this document.
- **unsupported** — not expressible yet. The adapter refuses rather than approximating.

## Nothing is wired into execution

These adapters are additive. The scheduler still runs workflows the way it always has and the agent
runtime still runs graphs the way it always has; what is new is that both can be described in one
representation. A test asserts that no execution crate depends on this one, so wiring the IR into a
runtime stays a deliberate M3 decision rather than something that happens by accident.

## Coverage

<!-- generated: adapter coverage -->

| Construct | Carries | Notes |
| --- | --- | --- |
| workflow step | with recorded loss | Becomes an effect node. The job it runs is an external side effect whose inputs and outputs are not modelled, so its ports are opaque. |
| workflow step dependency (hard) | full | Becomes a control edge. |
| workflow step dependency (soft, always) | with recorded loss | Becomes a control edge. The IR has no concept yet for continuing past a failed predecessor, so the distinction is recorded rather than expressed. |
| workflow step timeout and deadline | full | Become node and definition wall-clock budgets. |
| graph node kind | full | Maps onto IR node kinds, with validators becoming verifiers and model, retrieval, and ranking nodes becoming proposers. |
| graph port type | with recorded loss | Maps through the published alias table. The `json` tag has no structure to carry, and its opacity is recorded on the value. |
| graph hyperedge | full | Becomes a hyperedge whose combination names every contributing source. |
| graph state field endpoint | with recorded loss | Becomes a graph-level input or output. Shared mutable agent state is not a value the IR models, so the sharing is not carried. |
| agent budget | full | Becomes the loop region's iteration, token, time, and cost bounds. |
| agent termination condition | with recorded loss | Becomes repair routes and stop reasons. The current runtime decides continuation outside the graph, so a placeholder condition node stands in for the port a loop needs. |
| governed memory claim | with recorded loss | Becomes evidence plus a grounding obligation. Confidence is carried as text because a float has no canonical encoding. |
| governed memory write | full | Becomes a protected boundary over a memory-write effect. |
| stored reasoning certificate | with recorded loss | Wraps into a platform certificate without re-deciding it. The original verdict is carried as a declared oracle, because the snapshot it was decided against is not stored with it. |
| workflow run and step run state | unsupported | Execution history belongs to the durable runtime, which is M3. Only definitions are translated here. |
| automation trigger | unsupported | Triggers bind a schedule or an event to a workflow. The IR describes the workflow; binding is control-plane concern, not M2. |
