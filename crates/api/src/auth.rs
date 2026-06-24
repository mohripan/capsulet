use std::{
    collections::BTreeSet,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

use jsonwebtoken::{
    Algorithm, DecodingKey, Validation, decode, decode_header,
    jwk::{AlgorithmParameters, JwkSet},
};
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

    #[must_use]
    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "viewer" => Some(Self::Viewer),
            "operator" => Some(Self::Operator),
            "admin" => Some(Self::Admin),
            _ => None,
        }
    }
}

#[derive(Debug, Clone)]
pub struct Principal {
    pub name: Arc<str>,
    pub role: Role,
    pub platform_admin: bool,
    pub tenant_id: Arc<str>,
    pub project_id: Arc<str>,
    pub project_memberships: Arc<[ProjectMembership]>,
    scopes: Arc<[Arc<str>]>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProjectMembership {
    pub tenant_id: Arc<str>,
    pub project_id: Arc<str>,
    pub role: Arc<str>,
}

impl Principal {
    #[must_use]
    pub fn scopes(&self) -> &[Arc<str>] {
        &self.scopes
    }

    #[must_use]
    pub fn has_scope(&self, required: &str) -> bool {
        self.role == Role::Admin
            || self.scopes.iter().any(|scope| {
                scope.as_ref() == "*"
                    || scope.as_ref() == required
                    || required
                        .strip_suffix(":read")
                        .is_some_and(|prefix| scope.as_ref() == format!("{prefix}:*"))
                    || required
                        .split_once(':')
                        .is_some_and(|(resource, _)| scope.as_ref() == format!("{resource}:*"))
            })
    }

    #[must_use]
    pub fn service_account(
        name: impl Into<Arc<str>>,
        role: Role,
        tenant_id: impl Into<Arc<str>>,
        project_id: impl Into<Arc<str>>,
        scopes: impl IntoIterator<Item = String>,
    ) -> Self {
        let tenant_id = tenant_id.into();
        let project_id = project_id.into();
        Self {
            name: name.into(),
            role,
            platform_admin: role == Role::Admin,
            tenant_id: Arc::clone(&tenant_id),
            project_id: Arc::clone(&project_id),
            project_memberships: Arc::from([ProjectMembership {
                tenant_id,
                project_id,
                role: Arc::from(project_role_for_role(role)),
            }]),
            scopes: scopes.into_iter().map(Arc::from).collect::<Vec<_>>().into(),
        }
    }

    #[must_use]
    pub fn with_project_memberships(
        mut self,
        memberships: impl IntoIterator<Item = ProjectMembership>,
    ) -> Self {
        let memberships = memberships.into_iter().collect::<Vec<_>>();
        let mut scopes = self
            .scopes
            .iter()
            .map(|scope| scope.as_ref().to_string())
            .collect::<BTreeSet<_>>();
        if !scopes.contains("*") {
            for membership in &memberships {
                for scope in project_scopes_for_role(&membership.role) {
                    scopes.insert((*scope).to_string());
                }
            }
        }
        self.project_memberships = memberships.into();
        self.scopes = scopes.into_iter().map(Arc::from).collect::<Vec<_>>().into();
        self
    }
}

#[derive(Clone)]
pub struct AuthConfig {
    enabled: bool,
    credentials: Arc<[Credential]>,
    oidc: Option<Arc<OidcConfig>>,
}

impl std::fmt::Debug for AuthConfig {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("AuthConfig")
            .field("enabled", &self.enabled)
            .field("credential_count", &self.credentials.len())
            .field("oidc_enabled", &self.oidc.is_some())
            .finish()
    }
}

#[derive(Clone)]
struct Credential {
    name: Arc<str>,
    role: Role,
    tenant_id: Arc<str>,
    project_id: Arc<str>,
    scopes: Arc<[Arc<str>]>,
    expires_at_unix: Option<u64>,
    digest: [u8; 32],
}

#[derive(Clone)]
struct OidcConfig {
    issuer: Arc<str>,
    audience: Arc<str>,
    keys: Arc<[OidcKey]>,
}

#[derive(Clone)]
struct OidcKey {
    kid: Option<Arc<str>>,
    decoding_key: DecodingKey,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CredentialInput {
    name: String,
    role: String,
    token: String,
    #[serde(default = "default_tenant_id")]
    tenant_id: String,
    #[serde(default = "default_project_id")]
    project_id: String,
    #[serde(default)]
    scopes: Vec<String>,
    expires_at_unix: Option<u64>,
}

impl AuthConfig {
    #[must_use]
    pub fn disabled() -> Self {
        Self {
            enabled: false,
            credentials: Arc::from([]),
            oidc: None,
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
            let role = Role::parse(input.role.as_str())
                .ok_or_else(|| format!("unknown API role {} for credential {name}", input.role))?;
            let scopes = parse_scopes(name, role, input.scopes)?;
            credentials.push(Credential {
                name: Arc::from(name),
                role,
                tenant_id: Arc::from(input.tenant_id.trim().to_string()),
                project_id: Arc::from(input.project_id.trim().to_string()),
                scopes,
                expires_at_unix: input.expires_at_unix,
                digest: token_digest(&input.token),
            });
        }

        Ok(Self {
            enabled: true,
            credentials: credentials.into(),
            oidc: None,
        })
    }

    #[must_use]
    pub fn with_oidc(mut self, issuer: String, audience: String, jwks: &JwkSet) -> Self {
        let keys = jwks
            .keys
            .iter()
            .filter_map(|jwk| match &jwk.algorithm {
                AlgorithmParameters::RSA(parameters) => {
                    DecodingKey::from_rsa_components(&parameters.n, &parameters.e)
                        .ok()
                        .map(|decoding_key| OidcKey {
                            kid: jwk.common.key_id.as_deref().map(Arc::from),
                            decoding_key,
                        })
                }
                _ => None,
            })
            .collect::<Vec<_>>();
        if keys.is_empty() {
            return self;
        }
        self.enabled = true;
        self.oidc = Some(Arc::new(OidcConfig {
            issuer: Arc::from(issuer),
            audience: Arc::from(audience),
            keys: keys.into(),
        }));
        self
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
                platform_admin: true,
                tenant_id: Arc::from("default"),
                project_id: Arc::from("default"),
                project_memberships: Arc::from([ProjectMembership {
                    tenant_id: Arc::from("default"),
                    project_id: Arc::from("default"),
                    role: Arc::from("project_admin"),
                }]),
                scopes: Arc::from([Arc::from("*")]),
            });
        }

        self.authenticate_service_token(token)
            .or_else(|| self.authenticate_oidc_token(token))
    }

    fn authenticate_service_token(&self, token: &str) -> Option<Principal> {
        let candidate = token_digest(token);
        self.credentials.iter().find_map(|credential| {
            let not_expired = credential
                .expires_at_unix
                .is_none_or(|expires_at| now_unix_seconds() < expires_at);
            (bool::from(credential.digest.ct_eq(&candidate)) && not_expired).then(|| {
                let project_memberships = Arc::from([ProjectMembership {
                    tenant_id: Arc::clone(&credential.tenant_id),
                    project_id: Arc::clone(&credential.project_id),
                    role: Arc::from(project_role_for_role(credential.role)),
                }]);
                Principal {
                    name: Arc::clone(&credential.name),
                    role: credential.role,
                    platform_admin: credential.role == Role::Admin,
                    tenant_id: Arc::clone(&credential.tenant_id),
                    project_id: Arc::clone(&credential.project_id),
                    project_memberships,
                    scopes: Arc::clone(&credential.scopes),
                }
            })
        })
    }

    fn authenticate_oidc_token(&self, token: &str) -> Option<Principal> {
        let oidc = self.oidc.as_ref()?;
        let header = decode_header(token).ok()?;
        let mut validation = Validation::new(Algorithm::RS256);
        validation.set_issuer(&[oidc.issuer.as_ref()]);
        validation.set_audience(&[oidc.audience.as_ref()]);
        validation.validate_exp = true;

        oidc.keys
            .iter()
            .filter(|key| {
                header
                    .kid
                    .as_deref()
                    .is_none_or(|kid| key.kid.as_deref() == Some(kid))
            })
            .find_map(
                |key| match decode::<OidcClaims>(token, &key.decoding_key, &validation) {
                    Ok(data) => Some(principal_from_claims(&data.claims, &oidc.audience)),
                    Err(error) => {
                        capsulet_observability::tracing::warn!(%error, "OIDC token rejected");
                        None
                    }
                },
            )
    }
}

#[derive(Debug, Deserialize, Clone)]
struct OidcClaims {
    preferred_username: Option<String>,
    email: Option<String>,
    sub: Option<String>,
    realm_access: Option<OidcRealmAccess>,
    resource_access: Option<std::collections::HashMap<String, OidcRealmAccess>>,
}

#[derive(Debug, Deserialize, Clone)]
struct OidcRealmAccess {
    roles: Vec<String>,
}

fn principal_from_claims(claims: &OidcClaims, audience: &str) -> Principal {
    let role = oidc_role(claims, audience).unwrap_or(Role::Viewer);
    let name = claims
        .preferred_username
        .as_deref()
        .or(claims.email.as_deref())
        .or(claims.sub.as_deref())
        .unwrap_or("oidc-user");
    Principal {
        name: Arc::from(name),
        role,
        platform_admin: role == Role::Admin,
        tenant_id: Arc::from("default"),
        project_id: Arc::from("default"),
        project_memberships: Arc::from([]),
        scopes: default_scopes_for_role(role),
    }
}

const fn project_role_for_role(role: Role) -> &'static str {
    match role {
        Role::Viewer => "project_viewer",
        Role::Operator => "project_operator",
        Role::Admin => "project_admin",
    }
}

#[must_use]
pub fn token_digest(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

fn default_tenant_id() -> String {
    "default".to_string()
}

fn default_project_id() -> String {
    "default".to_string()
}

fn parse_scopes(name: &str, role: Role, scopes: Vec<String>) -> Result<Arc<[Arc<str>]>, String> {
    if scopes.is_empty() {
        return Ok(default_scopes_for_role(role));
    }
    let mut unique = BTreeSet::new();
    for scope in scopes {
        let scope = scope.trim();
        if scope.is_empty() {
            return Err(format!("API credential {name} contains an empty scope"));
        }
        if !scope.chars().all(|ch| {
            ch.is_ascii_lowercase() || ch.is_ascii_digit() || matches!(ch, ':' | '-' | '*' | '_')
        }) {
            return Err(format!(
                "API credential {name} contains invalid scope {scope}"
            ));
        }
        unique.insert(scope.to_string());
    }
    Ok(unique.into_iter().map(Arc::from).collect::<Vec<_>>().into())
}

fn default_scopes_for_role(role: Role) -> Arc<[Arc<str>]> {
    let scopes: &[&str] = match role {
        Role::Viewer => &[
            "auth:read",
            "jobs:read",
            "workflows:read",
            "automations:read",
            "system:read",
        ],
        Role::Operator => &[
            "auth:read",
            "jobs:read",
            "jobs:run",
            "jobs:cancel",
            "workflows:read",
            "workflows:operate",
            "automations:read",
            "automations:operate",
            "system:read",
        ],
        Role::Admin => &["*"],
    };
    scopes
        .iter()
        .copied()
        .map(Arc::from)
        .collect::<Vec<_>>()
        .into()
}

fn project_scopes_for_role(role: &str) -> &'static [&'static str] {
    match role {
        "project_viewer" => &[
            "auth:read",
            "jobs:read",
            "workflows:read",
            "automations:read",
            "system:read",
        ],
        "project_operator" => &[
            "auth:read",
            "jobs:read",
            "jobs:run",
            "jobs:cancel",
            "workflows:read",
            "workflows:operate",
            "automations:read",
            "automations:operate",
            "system:read",
        ],
        "project_admin" => &[
            "auth:read",
            "jobs:read",
            "jobs:run",
            "jobs:cancel",
            "jobs:write",
            "workflows:read",
            "workflows:operate",
            "workflows:write",
            "automations:read",
            "automations:operate",
            "automations:write",
            "system:read",
        ],
        _ => &[],
    }
}

fn now_unix_seconds() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_secs())
}

fn oidc_role(claims: &OidcClaims, audience: &str) -> Option<Role> {
    let realm_roles = claims
        .realm_access
        .as_ref()
        .into_iter()
        .flat_map(|access| access.roles.iter());
    let client_roles = claims
        .resource_access
        .as_ref()
        .and_then(|resources| resources.get(audience))
        .into_iter()
        .flat_map(|access| access.roles.iter());
    realm_roles
        .chain(client_roles)
        .fold(None, |current, role| match role.as_str() {
            "capsulet-platform-admin" | "capsulet-admin" | "admin" => Some(Role::Admin),
            "capsulet-operator" | "operator" => current.max(Some(Role::Operator)),
            "capsulet-viewer" | "viewer" => current.max(Some(Role::Viewer)),
            _ => current,
        })
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
        assert!(principal.has_scope("jobs:run"));
        assert!(config.authenticate("wrong").is_none());
        assert!(!format!("{config:?}").contains("0123456789abcdef"));
    }

    #[test]
    fn honors_explicit_scopes_and_expiry() {
        let config = AuthConfig::from_json(
            r#"[{"name":"ci","role":"operator","token":"0123456789abcdef0123456789abcdef","scopes":["jobs:run"],"expires_at_unix":4102444800}]"#,
        )
        .expect("valid scoped credential");

        let principal = config
            .authenticate("0123456789abcdef0123456789abcdef")
            .expect("known token");
        assert!(principal.has_scope("jobs:run"));
        assert!(!principal.has_scope("workflows:operate"));

        let expired = AuthConfig::from_json(
            r#"[{"name":"old","role":"admin","token":"abcdef0123456789abcdef0123456789","expires_at_unix":1}]"#,
        )
        .expect("valid expired credential");
        assert!(
            expired
                .authenticate("abcdef0123456789abcdef0123456789")
                .is_none()
        );
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
