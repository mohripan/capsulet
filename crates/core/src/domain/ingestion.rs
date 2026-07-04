use std::fmt::{self, Display};

use thiserror::Error;

use super::{
    Authority, Claim, Confidence, Entity, Evidence, IngestionConnectorId, IngestionRunId,
    MemoryError, MemoryScope, Source, SourceId,
};
use crate::{ClaimId, EntityId, EvidenceId};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestionConnectorKind {
    LocalText,
}

impl Display for IngestionConnectorKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::LocalText => "local_text",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestionConnectorConfig {
    title: String,
    content: String,
    content_type: String,
    uri: Option<String>,
    authority: Authority,
}

impl IngestionConnectorConfig {
    #[must_use]
    pub fn local_text(
        title: impl Into<String>,
        content: impl Into<String>,
        content_type: impl Into<String>,
        uri: Option<String>,
        authority: Authority,
    ) -> Self {
        Self {
            title: title.into(),
            content: content.into(),
            content_type: content_type.into(),
            uri,
            authority,
        }
    }

    #[must_use]
    pub fn title(&self) -> &str {
        &self.title
    }

    #[must_use]
    pub fn content(&self) -> &str {
        &self.content
    }

    #[must_use]
    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    #[must_use]
    pub fn uri(&self) -> Option<&str> {
        self.uri.as_deref()
    }

    #[must_use]
    pub const fn authority(&self) -> Authority {
        self.authority
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestionConnector {
    id: IngestionConnectorId,
    scope: MemoryScope,
    name: String,
    kind: IngestionConnectorKind,
    config: IngestionConnectorConfig,
    enabled: bool,
}

impl IngestionConnector {
    pub fn new(
        id: IngestionConnectorId,
        scope: MemoryScope,
        name: impl Into<String>,
        kind: IngestionConnectorKind,
        config: IngestionConnectorConfig,
        enabled: bool,
    ) -> Result<Self, IngestionError> {
        let name = non_empty(name.into(), "connector name")?;
        validate_config(kind, &config)?;
        Ok(Self {
            id,
            scope,
            name,
            kind,
            config,
            enabled,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &IngestionConnectorId {
        &self.id
    }

    #[must_use]
    pub const fn scope(&self) -> &MemoryScope {
        &self.scope
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub const fn kind(&self) -> IngestionConnectorKind {
        self.kind
    }

    #[must_use]
    pub const fn config(&self) -> &IngestionConnectorConfig {
        &self.config
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IngestionRunStatus {
    Queued,
    Running,
    Succeeded,
    Failed,
}

impl Display for IngestionRunStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Queued => "queued",
            Self::Running => "running",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestionRun {
    id: IngestionRunId,
    scope: MemoryScope,
    connector_id: IngestionConnectorId,
    status: IngestionRunStatus,
    error: Option<String>,
    source_count: u32,
    evidence_count: u32,
    entity_count: u32,
    claim_count: u32,
    event_count: u32,
    relationship_count: u32,
}

impl IngestionRun {
    #[must_use]
    pub const fn queued(
        id: IngestionRunId,
        scope: MemoryScope,
        connector_id: IngestionConnectorId,
    ) -> Self {
        Self {
            id,
            scope,
            connector_id,
            status: IngestionRunStatus::Queued,
            error: None,
            source_count: 0,
            evidence_count: 0,
            entity_count: 0,
            claim_count: 0,
            event_count: 0,
            relationship_count: 0,
        }
    }

    #[must_use]
    pub fn started(mut self) -> Self {
        self.status = IngestionRunStatus::Running;
        self
    }

    #[must_use]
    pub fn succeeded(
        mut self,
        source_count: usize,
        evidence_count: usize,
        entity_count: usize,
        claim_count: usize,
    ) -> Self {
        self.status = IngestionRunStatus::Succeeded;
        self.error = None;
        self.source_count = saturating_u32(source_count);
        self.evidence_count = saturating_u32(evidence_count);
        self.entity_count = saturating_u32(entity_count);
        self.claim_count = saturating_u32(claim_count);
        self
    }

    #[must_use]
    pub fn failed(mut self, error: impl Into<String>) -> Self {
        self.status = IngestionRunStatus::Failed;
        self.error = Some(error.into());
        self
    }

    #[must_use]
    #[expect(
        clippy::too_many_arguments,
        reason = "persistence rehydrates the complete run counter snapshot from storage"
    )]
    pub fn recorded(
        id: IngestionRunId,
        scope: MemoryScope,
        connector_id: IngestionConnectorId,
        status: IngestionRunStatus,
        error: Option<String>,
        source_count: u32,
        evidence_count: u32,
        entity_count: u32,
        claim_count: u32,
        event_count: u32,
        relationship_count: u32,
    ) -> Self {
        Self {
            id,
            scope,
            connector_id,
            status,
            error,
            source_count,
            evidence_count,
            entity_count,
            claim_count,
            event_count,
            relationship_count,
        }
    }

    #[must_use]
    pub const fn id(&self) -> &IngestionRunId {
        &self.id
    }

    #[must_use]
    pub const fn scope(&self) -> &MemoryScope {
        &self.scope
    }

    #[must_use]
    pub const fn connector_id(&self) -> &IngestionConnectorId {
        &self.connector_id
    }

    #[must_use]
    pub const fn status(&self) -> IngestionRunStatus {
        self.status
    }

    #[must_use]
    pub fn error(&self) -> Option<&str> {
        self.error.as_deref()
    }

    #[must_use]
    pub const fn source_count(&self) -> u32 {
        self.source_count
    }

    #[must_use]
    pub const fn evidence_count(&self) -> u32 {
        self.evidence_count
    }

    #[must_use]
    pub const fn entity_count(&self) -> u32 {
        self.entity_count
    }

    #[must_use]
    pub const fn claim_count(&self) -> u32 {
        self.claim_count
    }

    #[must_use]
    pub const fn event_count(&self) -> u32 {
        self.event_count
    }

    #[must_use]
    pub const fn relationship_count(&self) -> u32 {
        self.relationship_count
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IngestionRunOutputRecord {
    run_id: IngestionRunId,
    kind: String,
    memory_id: String,
}

impl IngestionRunOutputRecord {
    pub fn new(
        run_id: IngestionRunId,
        kind: impl Into<String>,
        memory_id: impl Into<String>,
    ) -> Result<Self, IngestionError> {
        Ok(Self {
            run_id,
            kind: non_empty(kind.into(), "output kind")?,
            memory_id: non_empty(memory_id.into(), "memory id")?,
        })
    }

    #[must_use]
    pub const fn run_id(&self) -> &IngestionRunId {
        &self.run_id
    }

    #[must_use]
    pub fn kind(&self) -> &str {
        &self.kind
    }

    #[must_use]
    pub fn memory_id(&self) -> &str {
        &self.memory_id
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct IngestionRunOutput {
    run: IngestionRun,
    sources: Vec<Source>,
    evidence: Vec<Evidence>,
    entities: Vec<Entity>,
    claims: Vec<Claim>,
}

impl IngestionRunOutput {
    #[must_use]
    pub const fn run(&self) -> &IngestionRun {
        &self.run
    }

    #[must_use]
    pub fn sources(&self) -> &[Source] {
        &self.sources
    }

    #[must_use]
    pub fn evidence(&self) -> &[Evidence] {
        &self.evidence
    }

    #[must_use]
    pub fn entities(&self) -> &[Entity] {
        &self.entities
    }

    #[must_use]
    pub fn claims(&self) -> &[Claim] {
        &self.claims
    }
}

pub fn run_local_text_ingestion(
    connector: &IngestionConnector,
    run: IngestionRun,
) -> Result<IngestionRunOutput, IngestionError> {
    if connector.kind() != IngestionConnectorKind::LocalText {
        return Err(IngestionError::UnsupportedConnectorKind(
            connector.kind().to_string(),
        ));
    }
    if !connector.enabled() {
        return Err(IngestionError::ConnectorDisabled);
    }
    validate_config(connector.kind(), connector.config())?;

    let scope = connector.scope().clone();
    let source = Source::new(
        SourceId::new(format!("source_{}", run.id().as_str())).map_err(IngestionError::Id)?,
        scope.clone(),
        "local_text",
        connector.config().uri().map(str::to_string),
        connector.config().title(),
        connector.config().authority(),
    )?;
    let chunks = chunks(connector.config().content());
    let evidence = chunks
        .iter()
        .enumerate()
        .map(|(index, chunk)| {
            Evidence::new(
                EvidenceId::new(format!("evidence_{}_{}", run.id().as_str(), index + 1))
                    .map_err(IngestionError::Id)?,
                scope.clone(),
                source.id().clone(),
                format!("chunk:{}", index + 1),
                chunk.as_str(),
                "ingestion",
            )
            .map_err(IngestionError::Memory)
        })
        .collect::<Result<Vec<_>, _>>()?;
    let entities = extract_entities(connector, run.id(), &scope)?;
    let claims = extract_claims(connector, run.id(), &scope, &entities, &evidence)?;
    let run = run
        .started()
        .succeeded(1, evidence.len(), entities.len(), claims.len());
    Ok(IngestionRunOutput {
        run,
        sources: vec![source],
        evidence,
        entities,
        claims,
    })
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IngestionError {
    #[error("{field} cannot be empty")]
    EmptyField { field: &'static str },
    #[error("unsupported content type: {0}")]
    UnsupportedContentType(String),
    #[error("unsupported connector kind: {0}")]
    UnsupportedConnectorKind(String),
    #[error("connector is disabled")]
    ConnectorDisabled,
    #[error("invalid ingestion id: {0}")]
    Id(String),
    #[error("invalid generated memory: {0}")]
    Memory(#[from] MemoryError),
}

fn validate_config(
    kind: IngestionConnectorKind,
    config: &IngestionConnectorConfig,
) -> Result<(), IngestionError> {
    match kind {
        IngestionConnectorKind::LocalText => {
            non_empty(config.title().to_string(), "title")?;
            non_empty(config.content().to_string(), "content")?;
            match config.content_type() {
                "text/plain" | "text/markdown" | "application/json" => Ok(()),
                value => Err(IngestionError::UnsupportedContentType(value.to_string())),
            }
        }
    }
}

fn chunks(content: &str) -> Vec<String> {
    content
        .split("\n\n")
        .map(str::trim)
        .filter(|chunk| !chunk.is_empty())
        .map(str::to_string)
        .collect()
}

fn extract_entities(
    connector: &IngestionConnector,
    run_id: &IngestionRunId,
    scope: &MemoryScope,
) -> Result<Vec<Entity>, IngestionError> {
    let mut names = connector
        .config()
        .content()
        .lines()
        .filter_map(|line| line.trim().strip_prefix("# "))
        .map(str::trim)
        .filter(|name| !name.is_empty())
        .map(str::to_string)
        .collect::<Vec<_>>();
    if names.is_empty() {
        names.push(connector.config().title().to_string());
    }
    names
        .iter()
        .enumerate()
        .map(|(index, name)| {
            Entity::new(
                EntityId::new(format!("entity_{}_{}", run_id.as_str(), index + 1))
                    .map_err(IngestionError::Id)?,
                scope.clone(),
                "DocumentTopic",
                name,
                vec![],
            )
            .map_err(IngestionError::Memory)
        })
        .collect()
}

fn extract_claims(
    connector: &IngestionConnector,
    run_id: &IngestionRunId,
    scope: &MemoryScope,
    entities: &[Entity],
    evidence: &[Evidence],
) -> Result<Vec<Claim>, IngestionError> {
    let Some(subject) = entities.first() else {
        return Ok(Vec::new());
    };
    let Some(first_evidence) = evidence.first() else {
        return Ok(Vec::new());
    };
    connector
        .config()
        .content()
        .lines()
        .map(str::trim)
        .filter(|line| line.starts_with("- "))
        .filter_map(|line| parse_claim_line(line.strip_prefix("- ").unwrap_or(line)))
        .enumerate()
        .map(|(index, (predicate, object))| {
            Claim::new(
                ClaimId::new(format!("claim_{}_{}", run_id.as_str(), index + 1))
                    .map_err(IngestionError::Id)?,
                scope.clone(),
                subject.id().clone(),
                predicate,
                object,
                vec![first_evidence.id().clone()],
                Confidence::new(0.55)?,
                connector.config().authority(),
                "ingestion",
                None,
                None,
            )
            .map_err(IngestionError::Memory)
        })
        .collect()
}

fn parse_claim_line(line: &str) -> Option<(&'static str, String)> {
    for (needle, predicate) in [
        (" blocked by ", "blocked_by"),
        (" depends on ", "depends_on"),
        (" approved ", "approved"),
        (" owns ", "owns"),
        (" has ", "has"),
        (" is ", "is"),
    ] {
        if let Some((_, object)) = line.split_once(needle) {
            let object = object.trim();
            if !object.is_empty() {
                return Some((predicate, object.to_string()));
            }
        }
    }
    line.split_once(':').and_then(|(_, rest)| {
        rest.split_once('=')
            .map(|(predicate, object)| (predicate.trim(), object.trim()))
            .filter(|(predicate, object)| !predicate.is_empty() && !object.is_empty())
            .map(|(predicate, object)| ("asserts", format!("{predicate} = {object}")))
    })
}

fn non_empty(value: String, field: &'static str) -> Result<String, IngestionError> {
    if value.trim().is_empty() {
        return Err(IngestionError::EmptyField { field });
    }
    Ok(value)
}

fn saturating_u32(value: usize) -> u32 {
    u32::try_from(value).unwrap_or(u32::MAX)
}
