-- Stage 0 of the correctness architecture: make citations re-derivable.
--
-- Evidence previously stored a free-text locator and excerpt, so nothing could
-- confirm that an excerpt was a faithful transcription of its source. A citation
-- was asserted rather than verified. These tables let the kernel re-derive an
-- excerpt from the exact bytes it was taken from.

-- Immutable stored text of a source, addressed by the digest of its bytes.
-- One row per (source, version): re-ingesting changed text inserts a new row
-- with a new digest rather than mutating the old one, so spans that cited the
-- previous version fail loudly instead of silently repointing.
CREATE TABLE memory_source_contents (
    source_id TEXT NOT NULL REFERENCES memory_sources(id) ON DELETE RESTRICT,
    content_hash TEXT NOT NULL,
    content TEXT NOT NULL,
    byte_length BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (source_id, content_hash)
);

CREATE INDEX memory_source_contents_source_idx
    ON memory_source_contents(source_id, created_at DESC);

-- Byte range into one version of a source's text. Nullable because evidence
-- recorded before this migration has no span; such evidence is retained and
-- readable but cannot ground a claim in the kernel.
ALTER TABLE memory_evidence
    ADD COLUMN span_start BIGINT,
    ADD COLUMN span_end BIGINT,
    ADD COLUMN source_content_hash TEXT;

-- A span is meaningless without all three parts, and an empty or inverted range
-- can never re-derive anything. Enforce both here so a malformed span cannot be
-- persisted even if a caller bypasses the domain constructor.
ALTER TABLE memory_evidence
    ADD CONSTRAINT memory_evidence_span_is_complete CHECK (
        (span_start IS NULL AND span_end IS NULL AND source_content_hash IS NULL)
        OR (span_start IS NOT NULL AND span_end IS NOT NULL AND source_content_hash IS NOT NULL)
    ),
    ADD CONSTRAINT memory_evidence_span_is_forward CHECK (
        span_start IS NULL OR (span_start >= 0 AND span_end > span_start)
    );
