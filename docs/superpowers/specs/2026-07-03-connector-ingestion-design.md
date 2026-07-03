# Connector Ingestion Design

## Status

Approved direction for implementation planning.

## Context

Capsulet already has the first memory substrate: sources, evidence, entities, claims, events, relationships, memory contracts, nested subgraphs, canonical identities, summary traces, and a Memory Studio frontend. The missing layer is the system that turns external data into candidate graph memory.

The next milestone should not be framed as a local file upload feature. The product abstraction is connector ingestion. A local deterministic connector is only the first connector adapter because it is testable, local-first, and does not require an LLM provider.

## Product Goal

Capsulet should ingest external knowledge sources through governed connectors, normalize source content, create evidence, propose candidate memory, and expose ingestion runs in Memory Studio.

The first implementation proves the full lifecycle:

1. Configure a connector.
2. Run ingestion.
3. Normalize documents.
4. Chunk content into evidence.
5. Propose candidate entities and claims.
6. Inspect run results and generated memory records.

Candidate memory must remain reviewable. Ingestion proposes memory; it does not silently turn extracted statements into trusted active facts.

## First Connector

The first connector type is a local deterministic connector.

Supported modes:

- Inline text or Markdown content supplied through the API.
- Local directory scanning can be added after the inline/local-text path is stable.

Supported content types for the first implementation:

- `text/plain`
- `text/markdown`
- optionally `application/json` if the parser can treat it as text safely.

The connector should be represented as a normal connector configuration, not as a special upload endpoint. This keeps the product model ready for future connectors such as GitHub, Slack, Notion, Google Drive, Jira, Confluence, and databases.

## Domain Model

### Ingestion Connector

An `IngestionConnector` defines a configured source of external knowledge.

Fields:

- `id`
- `tenant_id`
- `project_id`
- `name`
- `kind`
- `config`
- `enabled`
- `created_at`
- `updated_at`

Initial `kind` values:

- `local_text`
- later: `local_directory`, `github`, `slack`, `notion`, `google_drive`, `jira`, `database`

The connector `config` is JSON so each connector kind can evolve independently. The local text connector config should include at least:

- `title`
- `content`
- `content_type`
- optional `uri`
- optional `authority`

### Ingestion Run

An `IngestionRun` records one execution of a connector.

Fields:

- `id`
- `tenant_id`
- `project_id`
- `connector_id`
- `status`
- `started_at`
- `finished_at`
- `error`
- `source_count`
- `evidence_count`
- `entity_count`
- `claim_count`
- `event_count`
- `relationship_count`

Initial statuses:

- `queued`
- `running`
- `succeeded`
- `failed`

The first implementation can execute synchronously inside the API request while still storing a run record. The domain model should not assume synchronous execution forever.

### Ingested Document

An ingested document is the normalized unit emitted by a connector before extraction.

Fields:

- `id`
- `connector_id`
- `run_id`
- `title`
- `uri`
- `content_type`
- `content_sha256`
- `observed_at`
- `metadata`

The first implementation may not need a separate persisted table if `Source` already captures enough metadata, but the design should keep this concept explicit.

### Extraction Candidate

Extraction candidates represent proposed memory before governance actions.

For the first implementation, candidates can be persisted as normal memory records with candidate statuses where the existing model supports it:

- `Claim` uses `status = candidate`.
- `Entity` has no status today, so deterministic extraction may create entities directly, while the review workflow is added later.
- Evidence always points back to a source.

If entity/event/relationship review needs independent candidate state later, add a dedicated candidate table instead of overloading active memory records.

## Pipeline

The ingestion pipeline is:

```text
connector config
  -> connector adapter
  -> normalized documents
  -> parser/chunker
  -> source records
  -> evidence records
  -> extractor
  -> candidate entities
  -> candidate claims
  -> ingestion run summary
```

The deterministic extractor should be intentionally simple:

- Treat headings as possible entities.
- Recognize simple claim lines such as `Subject: predicate = object`.
- Recognize Markdown bullets containing `is`, `has`, `owns`, `depends on`, `approved`, or `blocked by` as candidate claims.
- Use conservative confidence such as `0.55`.
- Use source authority from connector config, defaulting to `medium`.

The first extractor should favor traceability and predictability over cleverness.

## API Surface

Add endpoints:

- `GET /v1/ingestion/connectors`
- `POST /v1/ingestion/connectors`
- `GET /v1/ingestion/connectors/{id}`
- `POST /v1/ingestion/connectors/{id}/runs`
- `GET /v1/ingestion/runs`
- `GET /v1/ingestion/runs/{id}`

Run responses should include:

- the run record
- generated source IDs
- generated evidence IDs
- generated entity IDs
- generated claim IDs
- errors, if any

The API should enforce project scope and require at least project operator role for connector creation and run execution.

## Storage

Add PostgreSQL tables:

- `ingestion_connectors`
- `ingestion_runs`
- optionally `ingestion_run_outputs`

`ingestion_run_outputs` should store generated memory IDs by kind:

- `run_id`
- `kind`
- `memory_id`

This avoids adding many nullable columns to `ingestion_runs` and makes the run detail page easy to build.

## Memory Studio

Add primary navigation items:

- Connectors
- Ingestion Runs

Pages:

- `/memory/connectors`
- `/memory/ingestion-runs`

The connectors page should:

- list connector configurations
- create a local text connector
- show enabled/disabled state
- trigger a run

The ingestion runs page should:

- list recent runs
- show status and counts
- show generated sources, evidence, entities, and claims
- show errors

The UI should keep the flat dark Docker-blue Memory Studio style.

## Error Handling

Connector execution should fail the run cleanly when:

- connector config is invalid
- content is empty
- unsupported content type is used
- generated memory records fail validation
- storage fails

Failed runs should retain the error message and any successfully generated outputs only if the implementation can guarantee consistency. The first implementation can run inside a database transaction to avoid partial memory creation.

## Testing

Tests should prove:

- connector config validation rejects empty or unsupported local text content
- triggering a local text connector creates an ingestion run
- the run creates a source and evidence records
- deterministic extraction creates candidate claims with evidence
- generated output IDs are attached to the run
- project scoping is enforced
- PostgreSQL persistence round-trips connector and run records
- Memory Studio can render connectors and ingestion runs

End-to-end smoke test:

1. Create a local text connector with Markdown content.
2. Trigger a run.
3. Assert run status is `succeeded`.
4. Assert at least one source, evidence record, entity, and candidate claim were created.
5. Open Memory Studio ingestion pages and verify the run appears.

## Deferred

The following are future work, not part of the first connector ingestion slice:

- asynchronous worker-backed ingestion queue
- external connector credentials
- GitHub/Slack/Notion/Jira/Google Drive connectors
- LLM-backed extraction
- vector embeddings
- human approval workflow
- contradiction detection
- source diffing and incremental sync

These are important roadmap goals, but the first implementation should establish the connector/run/pipeline contract without expanding the blast radius.
