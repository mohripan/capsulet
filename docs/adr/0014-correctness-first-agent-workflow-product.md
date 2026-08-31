# ADR 0014: Correctness-First Agent Workflow Product

Status: Accepted

## Context

Capsulet accumulated several incompatible product centers: Kubernetes job runner, automation
platform, Python workflow SDK, typed agent runtime, and governed AI memory. Each component contains
useful work, but allowing whichever subsystem was most recent to define the product caused public
claims, architecture, clients, and release criteria to drift.

The correctness kernel also exposed a deeper boundary: workflows should not merely execute model
or tool output; they must carry the evidence, assumptions, checks, policy, and verdict needed to
decide whether an important output may cross a protected boundary.

## Decision

Capsulet is a correctness-first AI-agent workflow platform for building and operating workflows
whose important outputs can be inspected, checked, and governed.

The platform has three layers:

- the workflow engine, including current durable jobs and compatibility DAG infrastructure;
- the agent platform, including typed agent behavior, tools, governed memory, budgets, and traces;
- the correctness plane, including proposals, evidence, obligations, deterministic checks,
  declared verifiers, policy, admission, and certificates.

The target architecture uses one versioned, trust-typed workflow IR for deterministic workflows,
agent workflows, and automations. Current DAGs, agent graphs, and memory records remain implemented
compatibility/subsystem models until later milestones introduce explicit adapters. This ADR does
not add the IR or rename persisted statuses.

Models propose; deterministic checkers and declared oracles justify; policy controls release.
Execution status never implies assurance. Governed memory is a major subsystem rather than the
complete product identity. The open-source self-hosted product reaches its maturity gates before a
managed cloud is allowed to redefine the architecture.

## Consequences

- Public claims must be evidence-backed or labeled experimental/planned.
- Compatibility workflows and the Python authoring SDK remain supported infrastructure without
  dictating the target IR.
- Product work follows the dependency-ordered milestones and release gates in the constitution.
- Earlier designs remain useful where retained explicitly; this decision wins on product identity,
  lifecycle vocabulary, assurance semantics, and release gates.
- Implementing the unified IR, durable graph worker, verifier ecosystem, and interpretability
  product remains out of scope for M0.
