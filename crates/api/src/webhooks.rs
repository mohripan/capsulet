use std::{
    collections::HashMap,
    fmt::Write as _,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use axum::{
    Json,
    body::Bytes,
    extract::{Path, State},
    http::{HeaderMap, StatusCode},
};
use capsulet_core::{AutomationId, TriggerKind};
use capsulet_postgres::TriggerEvent;
use capsulet_storage::ObjectStore;
use hmac::{Hmac, Mac};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use crate::{ApiStore, error::ApiError, state::AppState};

type HmacSha256 = Hmac<Sha256>;
const REPLAY_WINDOW_SECONDS: i64 = 300;

#[derive(Clone, Default)]
pub struct WebhookSecrets(Arc<HashMap<String, Arc<[u8]>>>);

impl std::fmt::Debug for WebhookSecrets {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("WebhookSecrets")
            .field("count", &self.0.len())
            .finish()
    }
}

impl WebhookSecrets {
    /// Parses HMAC secrets keyed by `automation/trigger`.
    ///
    /// # Errors
    ///
    /// Returns an error for invalid JSON, malformed keys, or weak secrets.
    pub fn from_json(value: &str) -> Result<Self, String> {
        let values: HashMap<String, String> = serde_json::from_str(value)
            .map_err(|error| format!("invalid CAPSULET_WEBHOOK_SECRETS: {error}"))?;
        let mut secrets = HashMap::with_capacity(values.len());
        for (key, secret) in values {
            if key.split_once('/').is_none() {
                return Err(format!(
                    "webhook secret key must be automation/trigger: {key}"
                ));
            }
            if secret.len() < 32 {
                return Err(format!("webhook secret {key} must be at least 32 bytes"));
            }
            secrets.insert(key, Arc::from(secret.into_bytes()));
        }
        Ok(Self(Arc::new(secrets)))
    }

    fn get(&self, automation_id: &str, trigger_name: &str) -> Option<&[u8]> {
        self.0
            .get(&format!("{automation_id}/{trigger_name}"))
            .or_else(|| self.0.get(&format!("{automation_id}/*")))
            .or_else(|| self.0.get("*/*"))
            .map(AsRef::as_ref)
    }
}

pub(crate) async fn ingest<S, O>(
    State(state): State<AppState<S, O>>,
    Path((automation_id, trigger_name)): Path<(String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<(StatusCode, Json<Value>), ApiError>
where
    S: ApiStore,
    O: ObjectStore,
{
    let secret = state
        .webhook_secrets
        .get(&automation_id, &trigger_name)
        .ok_or(ApiError::Unauthorized)?;
    let timestamp = header(&headers, "x-capsulet-timestamp")?
        .parse::<i64>()
        .map_err(|_| ApiError::Unauthorized)?;
    let now = i64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|error| ApiError::Store(error.to_string()))?
            .as_secs(),
    )
    .map_err(|_| ApiError::Store("epoch seconds exceed i64".to_string()))?;
    if now.abs_diff(timestamp) > REPLAY_WINDOW_SECONDS as u64 {
        return Err(ApiError::Unauthorized);
    }
    let delivery = header(&headers, "x-capsulet-delivery")?;
    if delivery.len() > 200 {
        return Err(ApiError::Validation(
            "webhook delivery id is too long".to_string(),
        ));
    }
    verify_signature(
        secret,
        timestamp,
        &body,
        header(&headers, "x-capsulet-signature")?,
    )?;
    let payload: Value = serde_json::from_slice(&body).map_err(|error| {
        ApiError::Validation(format!("webhook body must be valid JSON: {error}"))
    })?;
    let id = format!(
        "evt_{}",
        hex(
            &Sha256::digest(format!("{automation_id}\0{trigger_name}\0{delivery}").as_bytes())
                [..12]
        )
    );
    let correlation = headers
        .get("x-capsulet-correlation")
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .unwrap_or(delivery);
    if correlation.len() > 200 {
        return Err(ApiError::Validation(
            "webhook correlation id is too long".to_string(),
        ));
    }
    let automation = AutomationId::new(automation_id.clone()).map_err(ApiError::validation)?;
    let (triggers, _) = state
        .store
        .list_automation_triggers(&automation)
        .await
        .map_err(ApiError::store)?;
    if !triggers.iter().any(|trigger| {
        trigger.name().as_str() == trigger_name
            && trigger.kind() == TriggerKind::Webhook
            && trigger.enabled()
    }) {
        return Err(ApiError::Unauthorized);
    }
    let event = TriggerEvent {
        id,
        automation_id: automation.as_str().to_string(),
        trigger_name,
        correlation_key: correlation.to_string(),
        payload_json: payload.to_string(),
        occurred_at: timestamp.to_string(),
    };
    let inserted = state
        .store
        .enqueue_trigger_event(&event, delivery)
        .await
        .map_err(ApiError::store)?;
    Ok((
        if inserted {
            StatusCode::ACCEPTED
        } else {
            StatusCode::OK
        },
        Json(json!({
            "accepted": inserted, "event_id": event.id
        })),
    ))
}

fn header<'a>(headers: &'a HeaderMap, name: &str) -> Result<&'a str, ApiError> {
    headers
        .get(name)
        .and_then(|value| value.to_str().ok())
        .filter(|value| !value.is_empty())
        .ok_or(ApiError::Unauthorized)
}

fn verify_signature(
    secret: &[u8],
    timestamp: i64,
    body: &[u8],
    signature: &str,
) -> Result<(), ApiError> {
    let encoded = signature
        .strip_prefix("sha256=")
        .ok_or(ApiError::Unauthorized)?;
    let signature = decode_hex(encoded).ok_or(ApiError::Unauthorized)?;
    let mut mac = HmacSha256::new_from_slice(secret).map_err(|_| ApiError::Unauthorized)?;
    mac.update(timestamp.to_string().as_bytes());
    mac.update(b".");
    mac.update(body);
    mac.verify_slice(&signature)
        .map_err(|_| ApiError::Unauthorized)
}

fn decode_hex(value: &str) -> Option<Vec<u8>> {
    if value.len() != 64 {
        return None;
    }
    (0..value.len())
        .step_by(2)
        .map(|index| u8::from_str_radix(&value[index..index + 2], 16).ok())
        .collect()
}

fn hex(bytes: &[u8]) -> String {
    let mut output = String::with_capacity(bytes.len() * 2);
    for byte in bytes {
        write!(output, "{byte:02x}").expect("writing to a String cannot fail");
    }
    output
}

#[cfg(test)]
mod tests {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    use super::{WebhookSecrets, hex, verify_signature};

    #[test]
    fn verifies_timestamp_bound_hmac_and_rejects_tampering() {
        let secret = b"a-production-grade-webhook-secret";
        let timestamp = 1_782_134_400;
        let body = br#"{"ready":true}"#;
        let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC key");
        mac.update(timestamp.to_string().as_bytes());
        mac.update(b".");
        mac.update(body);
        let signature = format!("sha256={}", hex(&mac.finalize().into_bytes()));

        assert!(verify_signature(secret, timestamp, body, &signature).is_ok());
        assert!(verify_signature(secret, timestamp, br#"{"ready":false}"#, &signature).is_err());
        assert!(verify_signature(secret, timestamp + 1, body, &signature).is_err());
    }

    #[test]
    fn rejects_short_webhook_secrets() {
        assert!(WebhookSecrets::from_json(r#"{"automation/ready":"short"}"#).is_err());
        assert!(
            WebhookSecrets::from_json(
                r#"{"automation/ready":"a-production-grade-webhook-secret"}"#
            )
            .is_ok()
        );
        let wildcard =
            WebhookSecrets::from_json(r#"{"*/*":"a-production-grade-shared-webhook-secret"}"#)
                .expect("wildcard secret");
        assert_eq!(
            wildcard.get("automation", "ready"),
            Some(b"a-production-grade-shared-webhook-secret".as_slice())
        );
    }
}
