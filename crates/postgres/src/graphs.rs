use async_trait::async_trait;
use capsulet_application::GraphRepository;
use capsulet_core::{
    GraphDefinition, GraphHyperedge, GraphId, GraphNode, GraphPort, GraphTransitionMode,
    GraphTransitionPolicy, HyperedgeEndpoint, HyperedgeId, NodeId, NodeKind, PortDirection, PortId,
    PortValueType,
};
use sqlx::Row;

use crate::{PostgresStore, PostgresStoreError};

impl PostgresStore {
    /// Saves or replaces a typed graph definition and all owned graph parts.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when persistence fails.
    pub async fn upsert_graph(&self, graph: &GraphDefinition) -> Result<(), PostgresStoreError> {
        let mut tx = self.pool.begin().await?;
        upsert_graph_header(&mut tx, graph).await?;
        clear_graph_parts(&mut tx, graph.id()).await?;
        insert_graph_nodes(&mut tx, graph).await?;
        insert_transition_actions(&mut tx, graph).await?;
        insert_hyperedges(&mut tx, graph).await?;
        tx.commit().await?;
        Ok(())
    }

    /// Finds one typed graph definition.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn find_graph(
        &self,
        id: &GraphId,
    ) -> Result<Option<GraphDefinition>, PostgresStoreError> {
        let Some(row) = sqlx::query(
            r"
            SELECT id, name, transition_mode, cycles_allowed
            FROM graph_definitions
            WHERE id = $1
            ",
        )
        .bind(id.as_str())
        .fetch_optional(&self.pool)
        .await?
        else {
            return Ok(None);
        };

        let graph_id = GraphId::new(row.try_get::<String, _>("id")?)
            .map_err(PostgresStoreError::InvalidPersistedValue)?;
        let nodes = self.load_graph_nodes(&graph_id).await?;
        let hyperedges = self.load_graph_hyperedges(&graph_id).await?;
        let transition_policy = self.load_transition_policy(&row, &graph_id).await?;
        Ok(Some(GraphDefinition::new(
            graph_id,
            row.try_get::<String, _>("name")?,
            nodes,
            hyperedges,
            transition_policy,
        )?))
    }

    /// Lists typed graph definitions by most recent update.
    ///
    /// # Errors
    ///
    /// Returns [`PostgresStoreError`] when lookup fails or persisted values are invalid.
    pub async fn list_graphs(
        &self,
        limit: i64,
    ) -> Result<Vec<GraphDefinition>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id
            FROM graph_definitions
            ORDER BY updated_at DESC, id ASC
            LIMIT $1
            ",
        )
        .bind(limit)
        .fetch_all(&self.pool)
        .await?;
        let mut graphs = Vec::with_capacity(rows.len());
        for row in rows {
            let graph_id = GraphId::new(row.try_get::<String, _>("id")?)
                .map_err(PostgresStoreError::InvalidPersistedValue)?;
            let Some(graph) = self.find_graph(&graph_id).await? else {
                continue;
            };
            graphs.push(graph);
        }
        Ok(graphs)
    }

    async fn load_graph_nodes(
        &self,
        graph_id: &GraphId,
    ) -> Result<Vec<GraphNode>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, name, kind
            FROM graph_nodes
            WHERE graph_id = $1
            ORDER BY position ASC
            ",
        )
        .bind(graph_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        let mut nodes = Vec::with_capacity(rows.len());
        for row in rows {
            let node_id = NodeId::new(row.try_get::<String, _>("id")?)
                .map_err(PostgresStoreError::InvalidPersistedValue)?;
            nodes.push(GraphNode::new(
                node_id.clone(),
                row.try_get::<String, _>("name")?,
                parse_node_kind(&row.try_get::<String, _>("kind")?)?,
                self.load_graph_ports(graph_id, &node_id).await?,
            ));
        }
        Ok(nodes)
    }

    async fn load_graph_ports(
        &self,
        graph_id: &GraphId,
        node_id: &NodeId,
    ) -> Result<Vec<GraphPort>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id, direction, value_type
            FROM graph_ports
            WHERE graph_id = $1 AND node_id = $2
            ORDER BY position ASC
            ",
        )
        .bind(graph_id.as_str())
        .bind(node_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        rows.iter()
            .map(|row| {
                Ok(GraphPort::new(
                    PortId::new(row.try_get::<String, _>("id")?)
                        .map_err(PostgresStoreError::InvalidPersistedValue)?,
                    parse_port_direction(&row.try_get::<String, _>("direction")?)?,
                    parse_port_value_type(&row.try_get::<String, _>("value_type")?)?,
                ))
            })
            .collect()
    }

    async fn load_graph_hyperedges(
        &self,
        graph_id: &GraphId,
    ) -> Result<Vec<GraphHyperedge>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT id
            FROM graph_hyperedges
            WHERE graph_id = $1
            ORDER BY position ASC
            ",
        )
        .bind(graph_id.as_str())
        .fetch_all(&self.pool)
        .await?;
        let mut hyperedges = Vec::with_capacity(rows.len());
        for row in rows {
            let hyperedge_id = HyperedgeId::new(row.try_get::<String, _>("id")?)
                .map_err(PostgresStoreError::InvalidPersistedValue)?;
            hyperedges.push(GraphHyperedge::new(
                hyperedge_id.clone(),
                self.load_hyperedge_endpoints(graph_id, &hyperedge_id, "source")
                    .await?,
                self.load_hyperedge_endpoints(graph_id, &hyperedge_id, "target")
                    .await?,
            ));
        }
        Ok(hyperedges)
    }

    async fn load_hyperedge_endpoints(
        &self,
        graph_id: &GraphId,
        hyperedge_id: &HyperedgeId,
        role: &str,
    ) -> Result<Vec<HyperedgeEndpoint>, PostgresStoreError> {
        let rows = sqlx::query(
            r"
            SELECT endpoint_kind, node_id, port_id, state_field, value_type
            FROM graph_hyperedge_endpoints
            WHERE graph_id = $1 AND hyperedge_id = $2 AND role = $3
            ORDER BY position ASC
            ",
        )
        .bind(graph_id.as_str())
        .bind(hyperedge_id.as_str())
        .bind(role)
        .fetch_all(&self.pool)
        .await?;
        rows.iter().map(row_to_endpoint).collect()
    }

    async fn load_transition_policy(
        &self,
        row: &sqlx::postgres::PgRow,
        graph_id: &GraphId,
    ) -> Result<GraphTransitionPolicy, PostgresStoreError> {
        let cycles_allowed = row.try_get::<bool, _>("cycles_allowed")?;
        let mode = row.try_get::<String, _>("transition_mode")?;
        let policy = match mode.as_str() {
            "static" => GraphTransitionPolicy::static_acyclic(),
            "planner" => {
                let action_rows = sqlx::query(
                    r"
                    SELECT node_id
                    FROM graph_transition_actions
                    WHERE graph_id = $1
                    ORDER BY position ASC
                    ",
                )
                .bind(graph_id.as_str())
                .fetch_all(&self.pool)
                .await?;
                GraphTransitionPolicy::planner(
                    action_rows
                        .iter()
                        .map(|action| {
                            NodeId::new(action.try_get::<String, _>("node_id")?)
                                .map_err(PostgresStoreError::InvalidPersistedValue)
                        })
                        .collect::<Result<Vec<_>, _>>()?,
                )
            }
            value => {
                return Err(PostgresStoreError::InvalidPersistedValue(format!(
                    "unknown graph transition mode {value}"
                )));
            }
        };
        Ok(policy.with_cycles_allowed(cycles_allowed))
    }
}

#[async_trait]
impl GraphRepository for PostgresStore {
    type Error = PostgresStoreError;

    async fn save_graph(&self, graph: &GraphDefinition) -> Result<(), Self::Error> {
        self.upsert_graph(graph).await
    }
}

async fn upsert_graph_header(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    graph: &GraphDefinition,
) -> Result<(), PostgresStoreError> {
    let (transition_mode, cycles_allowed) = transition_policy_parts(graph.transition_policy());
    sqlx::query(
        r"
        INSERT INTO graph_definitions (id, name, transition_mode, cycles_allowed, updated_at)
        VALUES ($1, $2, $3, $4, now())
        ON CONFLICT (id) DO UPDATE SET
            name = EXCLUDED.name,
            transition_mode = EXCLUDED.transition_mode,
            cycles_allowed = EXCLUDED.cycles_allowed,
            updated_at = now()
        ",
    )
    .bind(graph.id().as_str())
    .bind(graph.name())
    .bind(transition_mode)
    .bind(cycles_allowed)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn clear_graph_parts(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    graph_id: &GraphId,
) -> Result<(), PostgresStoreError> {
    sqlx::query("DELETE FROM graph_transition_actions WHERE graph_id = $1")
        .bind(graph_id.as_str())
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM graph_hyperedges WHERE graph_id = $1")
        .bind(graph_id.as_str())
        .execute(&mut **tx)
        .await?;
    sqlx::query("DELETE FROM graph_nodes WHERE graph_id = $1")
        .bind(graph_id.as_str())
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn insert_graph_nodes(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    graph: &GraphDefinition,
) -> Result<(), PostgresStoreError> {
    for (position, node) in graph.nodes().iter().enumerate() {
        insert_graph_node(tx, graph.id(), position, node).await?;
        for (port_position, port) in node.ports().iter().enumerate() {
            insert_graph_port(tx, graph.id(), node.id(), port_position, port).await?;
        }
    }
    Ok(())
}

async fn insert_graph_node(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    graph_id: &GraphId,
    position: usize,
    node: &GraphNode,
) -> Result<(), PostgresStoreError> {
    sqlx::query(
        r"
        INSERT INTO graph_nodes (graph_id, id, position, name, kind)
        VALUES ($1, $2, $3, $4, $5)
        ",
    )
    .bind(graph_id.as_str())
    .bind(node.id().as_str())
    .bind(to_i32(position)?)
    .bind(node.name())
    .bind(node.kind().to_string())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_graph_port(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    graph_id: &GraphId,
    node_id: &NodeId,
    position: usize,
    port: &GraphPort,
) -> Result<(), PostgresStoreError> {
    sqlx::query(
        r"
        INSERT INTO graph_ports (graph_id, node_id, id, position, direction, value_type)
        VALUES ($1, $2, $3, $4, $5, $6)
        ",
    )
    .bind(graph_id.as_str())
    .bind(node_id.as_str())
    .bind(port.id().as_str())
    .bind(to_i32(position)?)
    .bind(port.direction().to_string())
    .bind(port.value_type().to_string())
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_transition_actions(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    graph: &GraphDefinition,
) -> Result<(), PostgresStoreError> {
    if let GraphTransitionMode::Planner { actions } = graph.transition_policy().mode() {
        for (position, action) in actions.iter().enumerate() {
            sqlx::query(
                r"
                INSERT INTO graph_transition_actions (graph_id, position, node_id)
                VALUES ($1, $2, $3)
                ",
            )
            .bind(graph.id().as_str())
            .bind(to_i32(position)?)
            .bind(action.as_str())
            .execute(&mut **tx)
            .await?;
        }
    }
    Ok(())
}

async fn insert_hyperedges(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    graph: &GraphDefinition,
) -> Result<(), PostgresStoreError> {
    for (position, hyperedge) in graph.hyperedges().iter().enumerate() {
        sqlx::query(
            r"
            INSERT INTO graph_hyperedges (graph_id, id, position)
            VALUES ($1, $2, $3)
            ",
        )
        .bind(graph.id().as_str())
        .bind(hyperedge.id().as_str())
        .bind(to_i32(position)?)
        .execute(&mut **tx)
        .await?;
        for (endpoint_position, endpoint) in hyperedge.sources().iter().enumerate() {
            insert_endpoint(
                tx,
                graph.id(),
                hyperedge.id(),
                "source",
                endpoint_position,
                endpoint,
            )
            .await?;
        }
        for (endpoint_position, endpoint) in hyperedge.targets().iter().enumerate() {
            insert_endpoint(
                tx,
                graph.id(),
                hyperedge.id(),
                "target",
                endpoint_position,
                endpoint,
            )
            .await?;
        }
    }
    Ok(())
}

async fn insert_endpoint(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    graph_id: &GraphId,
    hyperedge_id: &HyperedgeId,
    role: &str,
    position: usize,
    endpoint: &HyperedgeEndpoint,
) -> Result<(), PostgresStoreError> {
    let (endpoint_kind, node_id, port_id, state_field, value_type) = match endpoint {
        HyperedgeEndpoint::Port { node_id, port_id } => (
            "port",
            Some(node_id.as_str()),
            Some(port_id.as_str()),
            None,
            None,
        ),
        HyperedgeEndpoint::StateField { field, value_type } => (
            "state_field",
            None,
            None,
            Some(field.as_str()),
            Some(value_type.to_string()),
        ),
    };
    sqlx::query(
        r"
        INSERT INTO graph_hyperedge_endpoints (
            graph_id, hyperedge_id, role, position, endpoint_kind, node_id, port_id, state_field, value_type
        )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        ",
    )
    .bind(graph_id.as_str())
    .bind(hyperedge_id.as_str())
    .bind(role)
    .bind(to_i32(position)?)
    .bind(endpoint_kind)
    .bind(node_id)
    .bind(port_id)
    .bind(state_field)
    .bind(value_type)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn to_i32(value: usize) -> Result<i32, PostgresStoreError> {
    i32::try_from(value)
        .map_err(|error| PostgresStoreError::InvalidPersistedValue(error.to_string()))
}

fn transition_policy_parts(policy: &GraphTransitionPolicy) -> (&'static str, bool) {
    let mode = match policy.mode() {
        GraphTransitionMode::Static => "static",
        GraphTransitionMode::Planner { .. } => "planner",
    };
    (mode, policy.cycles_allowed())
}

fn row_to_endpoint(row: &sqlx::postgres::PgRow) -> Result<HyperedgeEndpoint, PostgresStoreError> {
    let kind = row.try_get::<String, _>("endpoint_kind")?;
    match kind.as_str() {
        "port" => Ok(HyperedgeEndpoint::port(
            NodeId::new(row.try_get::<String, _>("node_id")?)
                .map_err(PostgresStoreError::InvalidPersistedValue)?,
            PortId::new(row.try_get::<String, _>("port_id")?)
                .map_err(PostgresStoreError::InvalidPersistedValue)?,
        )),
        "state_field" => Ok(HyperedgeEndpoint::state_field(
            row.try_get::<String, _>("state_field")?,
            parse_port_value_type(&row.try_get::<String, _>("value_type")?)?,
        )),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown hyperedge endpoint kind {value}"
        ))),
    }
}

fn parse_node_kind(value: &str) -> Result<NodeKind, PostgresStoreError> {
    match value {
        "planner" => Ok(NodeKind::Planner),
        "query_normalizer" => Ok(NodeKind::QueryNormalizer),
        "embedding" => Ok(NodeKind::Embedding),
        "retriever" => Ok(NodeKind::Retriever),
        "reranker" => Ok(NodeKind::Reranker),
        "prompt_builder" => Ok(NodeKind::PromptBuilder),
        "llm" => Ok(NodeKind::Llm),
        "validator" => Ok(NodeKind::Validator),
        "memory_read" => Ok(NodeKind::MemoryRead),
        "memory_write" => Ok(NodeKind::MemoryWrite),
        "return" => Ok(NodeKind::Return),
        "job" => Ok(NodeKind::Job),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown node kind {value}"
        ))),
    }
}

fn parse_port_direction(value: &str) -> Result<PortDirection, PostgresStoreError> {
    match value {
        "input" => Ok(PortDirection::Input),
        "output" => Ok(PortDirection::Output),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown port direction {value}"
        ))),
    }
}

pub(crate) fn parse_port_value_type(value: &str) -> Result<PortValueType, PostgresStoreError> {
    match value {
        "user_query" => Ok(PortValueType::UserQuery),
        "conversation_context" => Ok(PortValueType::ConversationContext),
        "normalized_query" => Ok(PortValueType::NormalizedQuery),
        "embedding_vector" => Ok(PortValueType::EmbeddingVector),
        "retrieved_documents" => Ok(PortValueType::RetrievedDocuments),
        "ranked_documents" => Ok(PortValueType::RankedDocuments),
        "prompt" => Ok(PortValueType::Prompt),
        "model_response" => Ok(PortValueType::ModelResponse),
        "validation_result" => Ok(PortValueType::ValidationResult),
        "final_answer" => Ok(PortValueType::FinalAnswer),
        "json" => Ok(PortValueType::Json),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown port value type {value}"
        ))),
    }
}
