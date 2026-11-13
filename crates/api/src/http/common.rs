//! Small helpers the handlers share: generated ids, JSON fields, and list filters.

use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::Value;

use crate::error::ApiError;

pub(super) fn non_empty_trimmed(value: &str, label: &str) -> Result<String, ApiError> {
    let value = value.trim();
    if value.is_empty() {
        return Err(ApiError::Validation(format!("{label} cannot be empty")));
    }
    Ok(value.to_string())
}

pub(crate) fn json_from_string(value: &str) -> Result<Value, ApiError> {
    serde_json::from_str(value).map_err(|error| ApiError::Validation(error.to_string()))
}

pub(super) fn normalized_datetime(value: &str) -> String {
    value.replace('T', " ")
}

pub(super) fn matches_date_range(
    created_at: &str,
    start_at: Option<&str>,
    end_at: Option<&str>,
) -> bool {
    let created = normalized_datetime(created_at);
    if let Some(start) = start_at.filter(|value| !value.trim().is_empty())
        && created < normalized_datetime(start)
    {
        return false;
    }
    if let Some(end) = end_at.filter(|value| !value.trim().is_empty())
        && created > normalized_datetime(end)
    {
        return false;
    }
    true
}

pub(super) fn query_contains(value: &str, query: Option<&str>) -> bool {
    let Some(query) = query.map(str::trim).filter(|value| !value.is_empty()) else {
        return true;
    };
    value.to_lowercase().contains(&query.to_lowercase())
}

pub(super) fn is_desc(direction: Option<&str>) -> bool {
    !matches!(direction, Some(value) if value.eq_ignore_ascii_case("asc"))
}

pub(crate) fn valid_json_object_string(value: &Value, label: &str) -> Result<String, ApiError> {
    if value.is_object() {
        Ok(value.to_string())
    } else {
        Err(ApiError::Validation(format!(
            "{label} must be a JSON object"
        )))
    }
}

pub(crate) fn generated_id(prefix: &str) -> String {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis());
    format!("{prefix}_{millis}")
}
