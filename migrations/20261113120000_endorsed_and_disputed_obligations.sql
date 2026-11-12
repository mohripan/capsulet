-- A model-backed verifier endorses or disputes an obligation and never discharges it
-- (ADR 0021), so the projection learns two states. Both stay outstanding: an
-- endorsement certifies nothing, and a model's dispute is a reason to look first,
-- not a refutation.
ALTER TABLE assurance_obligations DROP CONSTRAINT assurance_obligations_state_is_known;
ALTER TABLE assurance_obligations ADD CONSTRAINT assurance_obligations_state_is_known
    CHECK (state IN (
        'discharged', 'assumed', 'waived', 'residual', 'failed', 'endorsed', 'disputed'
    ));
