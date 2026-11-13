-- The authority a reasoning certificate's readings are reviewed under, when the
-- contract its answer was read under names one (#159). A legal contract can say
-- that only a holder of `review:legal` decides what its clauses mean.
--
-- Null for every certificate written before a contract could name one, and for
-- answers read under a contract that names none: their readings are reviewed
-- under `review:write`, as they always were. Held as a scope, so the check below
-- refuses anything the API could never have recorded.

ALTER TABLE reasoning_certificates
    ADD COLUMN reading_authority TEXT,
    ADD CONSTRAINT reasoning_certificates_reading_authority_is_a_scope
        CHECK (
            reading_authority IS NULL
            OR reading_authority ~ '^[a-z][a-z0-9_-]*:[a-z][a-z0-9_-]*$'
        );
