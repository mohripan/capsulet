//! Claim memory in PostgreSQL: one module per kind of record, each with the rows it
//! reads back into domain records.
//!
//! What several share, the project scope a write is checked against and the parsers
//! for stored identifiers and statuses, is here.

mod contracts;
mod graph;
mod records;
mod sources;

use capsulet_core::{
    Authority, CanonicalEntityId, ClaimId, EntityId, EvidenceId, MemoryMemberId, MemoryScope,
    MemorySubgraphId,
};
use sqlx::Row;

use crate::PostgresStoreError;

/// Memory ids are globally unique, so an upsert can name a row owned by another tenant. The
/// scoped `ON CONFLICT ... WHERE` clause skips that row instead of taking it over, which shows
/// up here as an untouched row.
fn scope_guard(
    result: &sqlx::postgres::PgQueryResult,
    table: &str,
    id: &str,
) -> Result<(), PostgresStoreError> {
    if result.rows_affected() == 0 {
        return Err(PostgresStoreError::InvalidPersistedValue(format!(
            "{table} row {id} belongs to a different tenant or project"
        )));
    }
    Ok(())
}

fn scope(row: &sqlx::postgres::PgRow) -> Result<MemoryScope, PostgresStoreError> {
    MemoryScope::new(
        row.try_get::<String, _>("tenant_id")?,
        row.try_get::<String, _>("project_id")?,
    )
    .map_err(PostgresStoreError::Memory)
}

fn parse_authority(value: &str) -> Result<Authority, PostgresStoreError> {
    match value {
        "low" => Ok(Authority::Low),
        "medium" => Ok(Authority::Medium),
        "high" => Ok(Authority::High),
        value => Err(PostgresStoreError::InvalidPersistedValue(format!(
            "unknown authority {value}"
        ))),
    }
}

fn id_strings<T>(ids: &[T]) -> Vec<String>
where
    T: ToString,
{
    ids.iter().map(ToString::to_string).collect()
}

fn ids<T>(values: Vec<String>) -> Result<Vec<T>, PostgresStoreError>
where
    T: TryFromString,
{
    values.into_iter().map(T::try_from_string).collect()
}

trait TryFromString: Sized {
    fn try_from_string(value: String) -> Result<Self, PostgresStoreError>;
}

impl TryFromString for EntityId {
    fn try_from_string(value: String) -> Result<Self, PostgresStoreError> {
        Self::new(value).map_err(PostgresStoreError::InvalidPersistedValue)
    }
}

impl TryFromString for EvidenceId {
    fn try_from_string(value: String) -> Result<Self, PostgresStoreError> {
        Self::new(value).map_err(PostgresStoreError::InvalidPersistedValue)
    }
}

impl TryFromString for ClaimId {
    fn try_from_string(value: String) -> Result<Self, PostgresStoreError> {
        Self::new(value).map_err(PostgresStoreError::InvalidPersistedValue)
    }
}

impl TryFromString for MemorySubgraphId {
    fn try_from_string(value: String) -> Result<Self, PostgresStoreError> {
        Self::new(value).map_err(PostgresStoreError::InvalidPersistedValue)
    }
}

impl TryFromString for MemoryMemberId {
    fn try_from_string(value: String) -> Result<Self, PostgresStoreError> {
        Self::new(value).map_err(PostgresStoreError::InvalidPersistedValue)
    }
}

impl TryFromString for CanonicalEntityId {
    fn try_from_string(value: String) -> Result<Self, PostgresStoreError> {
        Self::new(value).map_err(PostgresStoreError::InvalidPersistedValue)
    }
}
