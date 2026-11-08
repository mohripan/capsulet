//! Calling a model.
//!
//! A model is something a node asks and an executor does not depend on: the trait is
//! what tests fake, and the one implementation talks to a local Ollama. Nothing here
//! reaches a hosted service, and a deployment with no model configured still runs
//! everything deterministic.
//!
//! What a model used is reported as it was reported. A response with no token counts
//! comes back as unknown rather than zero, because those are different statements and
//! only one of them is true.

use std::time::{Duration, Instant};

use async_trait::async_trait;
use capsulet_runtime::event::NodeUsage;
use serde_json::{Value, json};
use thiserror::Error;

/// What a node asks a model for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelRequest {
    pub model: String,
    pub prompt: String,
    /// Temperature in thousandths, because a parameter that is part of a definition
    /// digest has no business being a float. `None` asks for the deterministic end.
    pub temperature_milli: Option<u32>,
    pub max_tokens: Option<u32>,
    pub timeout: Duration,
}

/// What a model answered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Completion {
    pub text: String,
    pub usage: NodeUsage,
}

/// What a model embedded. A JSON value, because the numbers are floats and this crate
/// has no reason to look at them.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Embedding {
    pub vector: Value,
    pub usage: NodeUsage,
}

/// Why a model call failed.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ModelError {
    #[error("could not be reached: {0}")]
    Unreachable(String),
    #[error("refused with status {status}: {body}")]
    Rejected { status: u16, body: String },
    #[error("answered with something that could not be read: {0}")]
    Malformed(String),
}

/// Something that can answer a model request.
#[async_trait]
pub trait ModelClient: Send + Sync {
    /// The provider's name, as recorded in usage.
    fn name(&self) -> String;

    /// Asks for a completion.
    async fn complete(&self, request: &ModelRequest) -> Result<Completion, ModelError>;

    /// Asks for an embedding.
    async fn embed(
        &self,
        model: &str,
        text: &str,
        timeout: Duration,
    ) -> Result<Embedding, ModelError>;
}

/// A local Ollama.
pub struct OllamaClient {
    client: reqwest::Client,
    base_url: String,
}

impl OllamaClient {
    /// A client for the Ollama at `base_url`.
    #[must_use]
    pub fn new(base_url: impl Into<String>) -> Self {
        Self {
            client: reqwest::Client::new(),
            base_url: base_url.into().trim_end_matches('/').to_string(),
        }
    }

    async fn post(
        &self,
        path: &str,
        body: &Value,
        timeout: Duration,
    ) -> Result<(Value, u64), ModelError> {
        let started = Instant::now();
        let response = self
            .client
            .post(format!("{}{path}", self.base_url))
            .timeout(timeout)
            .json(body)
            .send()
            .await
            .map_err(unreachable)?;
        let status = response.status();
        let text = response.text().await.map_err(unreachable)?;
        if !status.is_success() {
            return Err(ModelError::Rejected {
                status: status.as_u16(),
                body: text,
            });
        }
        let parsed = serde_json::from_str(&text)
            .map_err(|error| ModelError::Malformed(error.to_string()))?;
        let elapsed = u64::try_from(started.elapsed().as_millis()).unwrap_or(u64::MAX);
        Ok((parsed, elapsed))
    }
}

/// A transport failure, said without the endpoint's URL.
///
/// The URL is the operator's configuration: an internal host name, a path, anything in
/// a query string. What this error becomes is a node's failure detail, which the
/// project's users read, so it says how the request failed and not where it was going.
/// (Credentials in the URL's user part never appear either way: the client moves them
/// into a header before sending.) The causes are kept, because "connection refused" is
/// what makes the failure actionable.
fn unreachable(error: reqwest::Error) -> ModelError {
    let error = error.without_url();
    let mut described = error.to_string();
    let mut cause = std::error::Error::source(&error);
    while let Some(each) = cause {
        described.push_str(": ");
        described.push_str(&each.to_string());
        cause = each.source();
    }
    ModelError::Unreachable(described)
}

#[async_trait]
impl ModelClient for OllamaClient {
    fn name(&self) -> String {
        "ollama".to_string()
    }

    async fn complete(&self, request: &ModelRequest) -> Result<Completion, ModelError> {
        let mut options = serde_json::Map::new();
        options.insert(
            "temperature".to_string(),
            json!(f64::from(request.temperature_milli.unwrap_or(0)) / 1_000.0),
        );
        if let Some(max_tokens) = request.max_tokens {
            options.insert("num_predict".to_string(), json!(max_tokens));
        }
        let (body, elapsed) = self
            .post(
                "/api/generate",
                &json!({
                    "model": request.model,
                    "prompt": request.prompt,
                    "stream": false,
                    "options": options,
                }),
                request.timeout,
            )
            .await?;
        let text = body
            .get("response")
            .and_then(Value::as_str)
            .ok_or_else(|| ModelError::Malformed("no `response` field".to_string()))?
            .to_string();
        Ok(Completion {
            text,
            usage: self.usage_of(&request.model, &body, elapsed),
        })
    }

    async fn embed(
        &self,
        model: &str,
        text: &str,
        timeout: Duration,
    ) -> Result<Embedding, ModelError> {
        let (body, elapsed) = self
            .post(
                "/api/embed",
                &json!({ "model": model, "input": text }),
                timeout,
            )
            .await?;
        let vector = body
            .get("embeddings")
            .and_then(|embeddings| embeddings.get(0))
            .cloned()
            .ok_or_else(|| ModelError::Malformed("no `embeddings` field".to_string()))?;
        Ok(Embedding {
            vector,
            usage: self.usage_of(model, &body, elapsed),
        })
    }
}

impl OllamaClient {
    fn usage_of(&self, model: &str, body: &Value, elapsed: u64) -> NodeUsage {
        NodeUsage {
            provider: Some(self.name()),
            model: Some(model.to_string()),
            input_tokens: body.get("prompt_eval_count").and_then(Value::as_u64),
            output_tokens: body.get("eval_count").and_then(Value::as_u64),
            // A local model has no price. Saying zero here would be a claim about a
            // bill that does not exist, so it stays unknown.
            cost_micro_units: None,
            latency_ms: Some(elapsed),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{ModelClient, ModelError, ModelRequest, OllamaClient};

    #[tokio::test]
    async fn an_unreachable_endpoint_is_reported_without_its_address() {
        // Port 1 on loopback refuses at once, so the request fails in transport.
        let client = OllamaClient::new("http://operator:hunter2-secret@127.0.0.1:1");
        let error = client
            .complete(&ModelRequest {
                model: "m".to_string(),
                prompt: "p".to_string(),
                temperature_milli: None,
                max_tokens: None,
                timeout: Duration::from_secs(5),
            })
            .await
            .expect_err("nothing listens there");
        let ModelError::Unreachable(described) = &error else {
            panic!("a transport failure: {error:?}");
        };
        assert!(!described.contains("hunter2-secret"), "{described}");
        assert!(!described.contains("operator:"), "{described}");
        assert!(!described.contains("127.0.0.1:1/api"), "{described}");
        assert!(!described.contains("/api/generate"), "{described}");
        assert!(
            described.starts_with("error sending request"),
            "{described}"
        );
    }
}
