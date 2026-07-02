CREATE TABLE graph_definitions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    transition_mode TEXT NOT NULL,
    cycles_allowed BOOLEAN NOT NULL DEFAULT false,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE TABLE graph_nodes (
    graph_id TEXT NOT NULL REFERENCES graph_definitions(id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    position INTEGER NOT NULL,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    PRIMARY KEY (graph_id, id),
    UNIQUE (graph_id, position)
);

CREATE TABLE graph_ports (
    graph_id TEXT NOT NULL,
    node_id TEXT NOT NULL,
    id TEXT NOT NULL,
    position INTEGER NOT NULL,
    direction TEXT NOT NULL,
    value_type TEXT NOT NULL,
    PRIMARY KEY (graph_id, node_id, id),
    FOREIGN KEY (graph_id, node_id) REFERENCES graph_nodes(graph_id, id) ON DELETE CASCADE,
    UNIQUE (graph_id, node_id, position)
);

CREATE TABLE graph_hyperedges (
    graph_id TEXT NOT NULL REFERENCES graph_definitions(id) ON DELETE CASCADE,
    id TEXT NOT NULL,
    position INTEGER NOT NULL,
    PRIMARY KEY (graph_id, id),
    UNIQUE (graph_id, position)
);

CREATE TABLE graph_hyperedge_endpoints (
    graph_id TEXT NOT NULL,
    hyperedge_id TEXT NOT NULL,
    role TEXT NOT NULL,
    position INTEGER NOT NULL,
    endpoint_kind TEXT NOT NULL,
    node_id TEXT,
    port_id TEXT,
    state_field TEXT,
    value_type TEXT,
    PRIMARY KEY (graph_id, hyperedge_id, role, position),
    FOREIGN KEY (graph_id, hyperedge_id) REFERENCES graph_hyperedges(graph_id, id) ON DELETE CASCADE,
    CHECK (role IN ('source', 'target')),
    CHECK (endpoint_kind IN ('port', 'state_field')),
    CHECK (
        (endpoint_kind = 'port' AND node_id IS NOT NULL AND port_id IS NOT NULL AND state_field IS NULL AND value_type IS NULL)
        OR
        (endpoint_kind = 'state_field' AND node_id IS NULL AND port_id IS NULL AND state_field IS NOT NULL AND value_type IS NOT NULL)
    )
);

CREATE TABLE graph_transition_actions (
    graph_id TEXT NOT NULL REFERENCES graph_definitions(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    node_id TEXT NOT NULL,
    PRIMARY KEY (graph_id, position),
    FOREIGN KEY (graph_id, node_id) REFERENCES graph_nodes(graph_id, id) ON DELETE CASCADE
);

CREATE TABLE agent_definitions (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    graph_id TEXT NOT NULL REFERENCES graph_definitions(id) ON DELETE RESTRICT,
    budget_max_steps INTEGER NOT NULL,
    budget_max_tokens BIGINT NOT NULL,
    budget_max_seconds BIGINT NOT NULL,
    budget_max_cost_micros BIGINT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (budget_max_steps > 0),
    CHECK (budget_max_tokens > 0),
    CHECK (budget_max_seconds > 0),
    CHECK (budget_max_cost_micros > 0)
);

CREATE TABLE agent_termination_conditions (
    agent_id TEXT NOT NULL REFERENCES agent_definitions(id) ON DELETE CASCADE,
    position INTEGER NOT NULL,
    condition TEXT NOT NULL,
    PRIMARY KEY (agent_id, position)
);

CREATE TABLE agent_runs (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES agent_definitions(id) ON DELETE RESTRICT,
    status TEXT NOT NULL,
    state_version BIGINT NOT NULL,
    state_json TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    CHECK (state_version >= 0)
);

CREATE TABLE agent_state_snapshots (
    agent_run_id TEXT NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,
    version BIGINT NOT NULL,
    state_json TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (agent_run_id, version)
);

CREATE TABLE agent_trace_events (
    agent_run_id TEXT NOT NULL REFERENCES agent_runs(id) ON DELETE CASCADE,
    sequence BIGINT NOT NULL,
    event_type TEXT NOT NULL,
    payload_json TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (agent_run_id, sequence)
);

CREATE INDEX graph_nodes_graph_position_idx ON graph_nodes(graph_id, position);
CREATE INDEX graph_ports_node_position_idx ON graph_ports(graph_id, node_id, position);
CREATE INDEX graph_hyperedges_graph_position_idx ON graph_hyperedges(graph_id, position);
CREATE INDEX graph_hyperedge_endpoints_order_idx ON graph_hyperedge_endpoints(graph_id, hyperedge_id, role, position);
CREATE INDEX agent_runs_agent_status_idx ON agent_runs(agent_id, status);
