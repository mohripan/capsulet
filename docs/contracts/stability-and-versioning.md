# Stability and Versioning Policy

This policy implements [ADR 0016](../adr/0016-public-contract-stability-and-versioning.md).

## Stability classes

Every published API operation and schema, SDK symbol, workflow IR schema, certificate schema, and
domain-pack interface has one class:

- `stable`: supported for the declared release window; compatibility and deprecation rules apply;
- `experimental`: usable but expected to evolve; changes still require release notes and migration
  guidance when persisted data or common authoring paths are affected;
- `internal`: not a public contract and may change within the repository without compatibility.

Before public alpha, every surface defaults to `experimental` unless an accepted ADR explicitly
promotes it. Omitted labels never mean stable. Stability attaches to a specific operation, schema,
or symbol, not to an entire component by implication.

## Deprecation and incompatible change

A stable alpha surface may be removed or changed incompatibly only after at least one published
alpha minor release includes all of:

1. a machine-visible deprecation marker where the format supports one;
2. release notes naming affected operations, schemas, or symbols;
3. a working replacement or migration path; and
4. a removal release no earlier than the next alpha minor after notice.

Urgent security removal may bypass the window only when a published advisory identifies the risk,
affected versions, replacement or mitigation, and any data/operational action required. This
exception is not available for ordinary maintenance.

Experimental changes may happen without the stable window, but incompatible persisted-format or
common authoring changes still require deterministic migration guidance. Internal changes must not
silently alter a stable or experimental public wire contract.

## Coordinated release version

Each product release has one version. Release automation propagates it to:

- all published container image tags and provenance;
- Helm chart `appVersion` and release notes;
- CLI `--version` output;
- dashboard build/about metadata;
- SDK compatibility metadata; and
- served API/OpenAPI build metadata.

Crate, npm, Python, or chart package versions may differ for ecosystem packaging only when release
documentation explains the mapping to the product release. A component version cannot claim
compatibility with a product release that its tests did not exercise.

## Versioned persisted formats and readers

Future workflow IR, certificates, and domain packs must include an explicit schema-version field
from their first public form. The field identifies the document schema, not merely the producing
binary version. Readers:

- accept every schema version in the documented support window;
- preserve unknown extension fields when a round trip promises preservation;
- migrate deterministically to a supported internal representation or reject explicitly;
- never guess a version from missing fields after a version becomes required; and
- report the unsupported version and supported range in the error.

Domain packs additionally declare compatible runtime and protocol ranges. Certificates pin the IR,
policy, verifier, evidence, and schema versions needed for replay. These are M2/M4 requirements;
this policy does not create placeholder runtime models in M0.
