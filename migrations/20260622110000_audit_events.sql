CREATE TABLE audit_events (
    id bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    principal text NOT NULL,
    role text NOT NULL,
    method text NOT NULL,
    path text NOT NULL,
    status_code integer NOT NULL CHECK (status_code BETWEEN 100 AND 599),
    request_id text,
    user_agent text,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX audit_events_created_at_idx ON audit_events (created_at DESC);
CREATE INDEX audit_events_principal_created_at_idx ON audit_events (principal, created_at DESC);
