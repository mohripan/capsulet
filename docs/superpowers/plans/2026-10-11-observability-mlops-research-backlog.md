# Observability, interpretability, MLOps and search research backlog

Requested by the maintainer on 2026-10-11. This is a triaged backlog, not an approved
implementation design or a completion claim. Commits must not contain co-author trailers.

GitHub overview: [#117](https://github.com/mohripan/capsulet/issues/117).
Current prerequisite: [M1 engineering integrity #104](https://github.com/mohripan/capsulet/issues/104).
The existing product constitution reserves interpretability for M5; research tracks do not renumber it.

## Milestones

- [M5 - Interpretability and agent observability](https://github.com/mohripan/capsulet/milestone/3)
- [MLOps - Evaluation and reproducible experiments](https://github.com/mohripan/capsulet/milestone/4)
- [Research - Harness-1 and state-externalizing search](https://github.com/mohripan/capsulet/milestone/5)

## Work items

| Issue | Milestone |
| --- | --- |
+| [Instrument agent runs with correlated OpenTelemetry GenAI traces](https://github.com/mohripan/capsulet/issues/106) | M5 - Interpretability and agent observability |
| [Evaluate and integrate one reusable agent observability backend](https://github.com/mohripan/capsulet/issues/105) | M5 - Interpretability and agent observability |
| [Explain model results with inspectable evidence and verification lineage](https://github.com/mohripan/capsulet/issues/107) | M5 - Interpretability and agent observability |
| [Protect model telemetry with project isolation, redaction and retention](https://github.com/mohripan/capsulet/issues/109) | M5 - Interpretability and agent observability |
| [Add versioned evaluation datasets and quality-cost-latency regression gates](https://github.com/mohripan/capsulet/issues/110) | MLOps - Evaluation and reproducible experiments |
| [Track reproducible model, prompt, retrieval and experiment lineage](https://github.com/mohripan/capsulet/issues/108) | MLOps - Evaluation and reproducible experiments |
| [Add evidence-linked human feedback and failure triage without automatic retraining](https://github.com/mohripan/capsulet/issues/113) | MLOps - Evaluation and reproducible experiments |
| [Investigate Harness-1 upstream compatibility, licensing and reproducibility](https://github.com/mohripan/capsulet/issues/111) | Research - Harness-1 and state-externalizing search |
| [Prototype durable externalized search state with an upstream-compatible adapter](https://github.com/mohripan/capsulet/issues/112) | Research - Harness-1 and state-externalizing search |
| [Measure budget-aware context curation and evidence-preserving compression](https://github.com/mohripan/capsulet/issues/115) | Research - Harness-1 and state-externalizing search |
| [Run controlled search-harness ablations and decide whether RL is justified](https://github.com/mohripan/capsulet/issues/116) | Research - Harness-1 and state-externalizing search |
| [Provide redacted run-debug bundles and a one-command local trace/replay workflow](https://github.com/mohripan/capsulet/issues/114) | M5 - Interpretability and agent observability |

## Recommended sequence and boundaries

1. Finish IAM isolation and real verification under M1.
2. Instrument observed agent/model/retrieval/tool actions and enforce telemetry privacy.
3. Evaluate one optional reusable backend, then connect explanations and local debugging to
   evidence and certificate lineage.
4. Establish versioned evaluation baselines, experiment lineage and reviewed feedback.
5. Evaluate upstream Harness-1 fit, then run a bounded state-externalizing prototype and ablations.
   Propose RL only if simpler upstream/model reuse is insufficient and measurements justify it.

The current shared observability crate already supports JSON logs, Prometheus and OTLP.
Preserve that boundary instead of building a general-purpose telemetry platform. Results should
be interpretable through observed evidence and checks; generated explanations and judge scores
are not correctness proofs, and private chain-of-thought is not a logging requirement.

Candidate reuse sources: [OpenTelemetry GenAI conventions](https://opentelemetry.io/docs/specs/semconv/gen-ai/),
[Langfuse](https://github.com/langfuse/langfuse), [MLflow](https://github.com/mlflow/mlflow), and
[Phoenix](https://github.com/Arize-ai/phoenix). The backend ADR must review current feature and
license boundaries: Langfuse's MIT core excludes enterprise directories; MLflow is Apache-2.0;
Phoenix uses Elastic License 2.0, a source-available license requiring a deliberate policy decision.
Do not install all candidates or make a hosted service mandatory.

[Harness-1](https://arxiv.org/abs/2606.02373) has [published upstream code](https://github.com/pat-jj/harness-1).
Review code, checkpoint and dataset terms independently. Its documented local serving path
requires CUDA/vLLM; creating this research milestone does not authorize model downloads,
GPU spending, paid accounts or training. Reproduction limits and service dependencies belong in
the initial ADR, and research findings must not become product claims without executable evidence.

Use GitHub labels for tracks/types: `track:platform`, `track:developer-experience`, `track:mlops`,
`track:research`, `bug`, `enhancement`, `research`, and applicable `area:*` labels.
Use annotated Git tags only for identified code checkpoints, not unbuilt backlog features.
