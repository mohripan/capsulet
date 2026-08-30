<!-- capsulet-claims: CAP-PRODUCT-001 -->
# Documentation

Capsulet is a correctness-first AI-agent workflow platform. This directory contains its current
product, architecture, operations, security, contract, and historical design documentation.

Use [Public contracts](contracts/README.md) for claim maturity and evidence, and the
[product constitution](superpowers/specs/2026-08-30-correctness-first-agent-workflow-platform-design.md)
for target direction. Current behavior has three explicit layers: an implemented compatibility
workflow engine, an experimental agent platform (including governed memory), and an implemented
kernel slice within a broader planned correctness plane.

Suggested layout:

- `adr/`: architecture decision records
- `design/`: focused design notes before implementation
- `operations/`: installation, upgrades, observability, and troubleshooting
- `security/`: sandboxing, threat model, and hardening guidance
- `contracts/`: normative claim, lifecycle, stability, migration, and SDK policies

Current docs:

- [Development](development.md)
- [Installation](installation.md)
- [Architecture](architecture.md)
- [Detailed repository architecture](../ARCHITECTURE.md)
- [Correctness architecture](design/correctness-architecture.md) — partially implemented historical
  foundation; superseded by the constitution where they conflict
- [API](api.md)
- [Helm values](helm-values.md)
- [Persistence](persistence.md)
- [Worker and runner](worker-runner.md)
- [Local Kubernetes runner](local-kubernetes-runner.md)
- [Troubleshooting](troubleshooting.md)
