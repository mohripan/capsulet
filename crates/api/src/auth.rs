use std::sync::Arc;

use serde::Deserialize;
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Role {
    Viewer,
    Operator,
    Admin,
}

impl Role {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Viewer => "viewer",
            Self::Operator => "operator",
            Self::Admin => "admin",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Principal {
    pub name: Arc<str>,
    pub role: Role,
}

#[derive(Clone)]
pub struct AuthConfig {
    enabled: bool,
    credentials: Arc<[Credential]>,
}

impl std::fmt::Debug for AuthConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuthConfig")
            .field("enabled", &self.enabled)
            .field("credential_count", &self.credentials.len())
            .finish()
    }
}

#[derive(Clone)]
struct Credential {
    name: Arc<str>,
    role: Role,
    digest: [u8; 32],
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialInput {
    name: String,
    role: String,
    token: String,
}

impl AuthConfig {
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            credentials: Arc::from([]),
        }
    }

    /// Parses API credentials from a JSON array.
    ///
    /// # Errors
    ///
    /// Returns an error for malformed JSON, duplicate/empty names, weak tokens,
    /// unknown roles, or an empty credential set.
    pub fn from_json(value: &str) -> Result<Self, String> {
        let inputs: Vec<CredentialInput> = serde_json::from_str(value)
            .map_err(|error| format!("invalid CAPSULET_API_TOKENS: {error}"))?;
        if inputs.is_empty() {
            return Err("CAPSULET_API_TOKENS must contain at least one credential".to_string());
        }

        let mut names = std::collections::HashSet::new();
        let mut credentials = Vec::with_capacity(inputs.len());
        for input in inputs {
            let name = input.name.trim();
            if name.is_empty() {
                return Err("API credential name cannot be empty".to_string());
            }
            if !names.insert(name.to_string()) {
                return Err(format!("duplicate API credential name: {name}"));
            }
            if input.token.len() < 32 {
                return Err(format!(
                    "API credential {name} must use a token of at least 32 bytes"
                ));
            }
            let role = match input.role.as_str() {
                "viewer" => Role::Viewer,
                "operator" => Role::Operator,
                "admin" => Role::Admin,
                role => return Err(format!("unknown API role {role} for credential {name}")),
            };
            credentials.push(Credential {
                name: Arc::from(name),
                role,
                digest: Sha256::digest(input.token.as_bytes()).into(),
            });
        }

        Ok(Self {
            enabled: true,
            credentials: credentials.into(),
        })
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }

    #[must_use]
    pub fn authenticate(&self, token: &str) -> Option<Principal> {
        if !self.enabled {
            return Some(Principal {
                name: Arc::from("authentication-disabled"),
                role: Role::Admin,
            });
        }

        let candidate: [u8; 32] = Sha256::digest(token.as_bytes()).into();
        self.credentials.iter().find_map(|credential| {
            bool::from(credential.digest.ct_eq(&candidate)).then(|| Principal {
                name: Arc::clone(&credential.name),
                role: credential.role,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{AuthConfig, Role};

    #[test]
    fn authenticates_configured_roles_without_exposing_tokens() {
        let config = AuthConfig::from_json(
            r#"[{"name":"ops","role":"operator","token":"0123456789abcdef0123456789abcdef"}]"#,
        )
        .expect("valid auth config");

        let principal = config
            .authenticate("0123456789abcdef0123456789abcdef")
            .expect("known token");
        assert_eq!(principal.role, Role::Operator);
        assert!(config.authenticate("wrong").is_none());
        assert!(!format!("{config:?}").contains("0123456789abcdef"));
    }

    #[test]
    fn rejects_weak_or_duplicate_credentials() {
        assert!(AuthConfig::from_json(r#"[{"name":"a","role":"admin","token":"short"}]"#).is_err());
        assert!(AuthConfig::from_json(
            r#"[{"name":"a","role":"admin","token":"0123456789abcdef0123456789abcdef"},{"name":"a","role":"viewer","token":"abcdef0123456789abcdef0123456789"}]"#,
        )
        .is_err());
    }
}
