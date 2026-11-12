//! The stack fixture: one tenant, two projects and an identity for each role,
//! seeded through the API's own bootstrap and public contracts.
//!
//! Nothing here is a shared secret. Every token and password is generated for
//! one stack and lives in files that the stack's teardown deletes. What stays
//! fixed is everything a test asserts on: aliases, ids, names and roles.

use std::{collections::BTreeMap, fmt::Write as _, net::TcpListener, thread, time::Duration};

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Every fixture project is in the tenant the migrations create. Tenants have
/// no API, and one is all the fixture needs.
pub(crate) const TENANT: &str = "default";

/// The two projects, as (alias, id, name). Isolation tests check that what one
/// holds stays out of the other's reach.
pub(crate) const PROJECTS: [(&str, &str, &str); 2] = [
    ("alpha", "e2e-alpha", "E2E Alpha"),
    ("beta", "e2e-beta", "E2E Beta"),
];

/// An identity: its alias, the principal it authenticates as, its role and the
/// project it lands in when a request selects none.
struct Identity {
    alias: &'static str,
    principal: &'static str,
    role: &'static str,
    project: &'static str,
}

/// Identities from the bootstrap contract, `CAPSULET_API_TOKENS`. A bootstrap
/// admin is a platform admin, so the admin starts in the tenant's default
/// project and selects others; operator and viewer belong to alpha alone.
const BOOTSTRAP: [Identity; 3] = [
    Identity {
        alias: "admin",
        principal: "e2e-admin",
        role: "admin",
        project: "default",
    },
    Identity {
        alias: "operator",
        principal: "e2e-operator",
        role: "operator",
        project: "e2e-alpha",
    },
    Identity {
        alias: "viewer",
        principal: "e2e-viewer",
        role: "viewer",
        project: "e2e-alpha",
    },
];

/// The identity the seed creates through the public contract.
const SERVICE: Identity = Identity {
    alias: "service",
    principal: "e2e-service",
    role: "operator",
    project: "e2e-alpha",
};

/// How long every fixture credential is accepted.
const CREDENTIAL_LIFETIME_SECONDS: u64 = 4 * 60 * 60;

/// The host ports one stack publishes, all on 127.0.0.1.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Ports {
    pub(crate) api: u16,
    pub(crate) dashboard: u16,
    pub(crate) keycloak: u16,
    pub(crate) postgres: u16,
    pub(crate) smtp: u16,
    pub(crate) mail_ui: u16,
    pub(crate) prometheus: u16,
    pub(crate) grafana: u16,
    pub(crate) object_store: u16,
}

impl Ports {
    /// The ports `compose.yaml` documents, for a developer's own stack.
    pub(crate) const STANDARD: Self = Self {
        api: 8080,
        dashboard: 3000,
        keycloak: 18080,
        postgres: 55432,
        smtp: 1025,
        mail_ui: 8025,
        prometheus: 9090,
        grafana: 3001,
        object_store: 8333,
    };

    /// Ports nothing is listening on, so a verifier stack never collides with a
    /// developer's stack or with another verifier run.
    pub(crate) fn free() -> Result<Self, String> {
        let listeners = (0..9)
            .map(|_| TcpListener::bind("127.0.0.1:0"))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("could not find a free port: {error}"))?;
        let ports = listeners
            .iter()
            .map(|listener| listener.local_addr().map(|address| address.port()))
            .collect::<Result<Vec<_>, _>>()
            .map_err(|error| format!("could not read a free port: {error}"))?;
        Ok(Self {
            api: ports[0],
            dashboard: ports[1],
            keycloak: ports[2],
            postgres: ports[3],
            smtp: ports[4],
            mail_ui: ports[5],
            prometheus: ports[6],
            grafana: ports[7],
            object_store: ports[8],
        })
    }

    pub(crate) fn api_url(self) -> String {
        format!("http://127.0.0.1:{}", self.api)
    }

    pub(crate) fn dashboard_url(self) -> String {
        format!("http://127.0.0.1:{}", self.dashboard)
    }
}

/// The secrets of one stack. None of them is ever checked in.
#[derive(Serialize, Deserialize)]
pub(crate) struct Secrets {
    tokens: BTreeMap<String, String>,
    dashboard_password: String,
    keycloak_admin_password: String,
    keycloak_user_password: String,
    grafana_password: String,
    postgres_password: String,
    object_store_secret_key: String,
    object_store_volume_signing_key: String,
    object_store_filer_signing_key: String,
    webhook_secret: String,
    expires_at_unix: u64,
}

impl Secrets {
    /// Generates fresh secrets whose credentials expire a few hours after `now_unix`.
    pub(crate) fn generate(now_unix: u64) -> Result<Self, String> {
        let mut tokens = BTreeMap::new();
        for identity in &BOOTSTRAP {
            tokens.insert(identity.alias.to_string(), random_secret()?);
        }
        Ok(Self {
            tokens,
            dashboard_password: random_secret()?,
            keycloak_admin_password: random_secret()?,
            keycloak_user_password: random_secret()?,
            grafana_password: random_secret()?,
            postgres_password: random_secret()?,
            object_store_secret_key: random_secret()?,
            object_store_volume_signing_key: random_secret()?,
            object_store_filer_signing_key: random_secret()?,
            webhook_secret: random_secret()?,
            expires_at_unix: now_unix + CREDENTIAL_LIFETIME_SECONDS,
        })
    }

    fn token(&self, alias: &str) -> &str {
        &self.tokens[alias]
    }

    /// `CAPSULET_API_TOKENS`: one expiring credential per bootstrap identity.
    fn api_tokens(&self) -> String {
        Value::Array(
            BOOTSTRAP
                .iter()
                .map(|identity| {
                    json!({
                        "name": identity.principal,
                        "role": identity.role,
                        "tenant_id": TENANT,
                        "project_id": identity.project,
                        "token": self.token(identity.alias),
                        "expires_at_unix": self.expires_at_unix,
                    })
                })
                .collect(),
        )
        .to_string()
    }

    /// The environment file `compose.yaml` reads its secrets and ports from.
    pub(crate) fn env_file(&self, ports: Ports) -> String {
        let entries = [
            ("CAPSULET_API_TOKENS", self.api_tokens()),
            (
                "CAPSULET_WEBHOOK_SECRETS",
                json!({ "*/*": self.webhook_secret }).to_string(),
            ),
            ("CAPSULET_POSTGRES_PASSWORD", self.postgres_password.clone()),
            (
                "CAPSULET_OBJECT_STORE_SECRET_KEY",
                self.object_store_secret_key.clone(),
            ),
            (
                "CAPSULET_OBJECT_STORE_VOLUME_SIGNING_KEY",
                self.object_store_volume_signing_key.clone(),
            ),
            (
                "CAPSULET_OBJECT_STORE_FILER_SIGNING_KEY",
                self.object_store_filer_signing_key.clone(),
            ),
            (
                "CAPSULET_DASHBOARD_ADMIN_USERNAME",
                BOOTSTRAP[0].principal.to_string(),
            ),
            (
                "CAPSULET_DASHBOARD_ADMIN_PASSWORD",
                self.dashboard_password.clone(),
            ),
            (
                "CAPSULET_DASHBOARD_ADMIN_TOKEN",
                self.token("admin").to_string(),
            ),
            (
                "CAPSULET_KEYCLOAK_ADMIN_PASSWORD",
                self.keycloak_admin_password.clone(),
            ),
            (
                "CAPSULET_KEYCLOAK_USER_PASSWORD",
                self.keycloak_user_password.clone(),
            ),
            (
                "CAPSULET_GRAFANA_ADMIN_PASSWORD",
                self.grafana_password.clone(),
            ),
            ("CAPSULET_API_PORT", ports.api.to_string()),
            ("CAPSULET_DASHBOARD_PORT", ports.dashboard.to_string()),
            ("CAPSULET_KEYCLOAK_PORT", ports.keycloak.to_string()),
            ("CAPSULET_POSTGRES_HOST_PORT", ports.postgres.to_string()),
            ("CAPSULET_SMTP_PORT", ports.smtp.to_string()),
            ("CAPSULET_MAIL_UI_PORT", ports.mail_ui.to_string()),
            ("CAPSULET_PROMETHEUS_PORT", ports.prometheus.to_string()),
            ("CAPSULET_GRAFANA_PORT", ports.grafana.to_string()),
            ("CAPSULET_OBJECT_STORE_PORT", ports.object_store.to_string()),
        ];
        let mut file = String::from(
            "# Generated for one Capsulet stack and deleted with it. Never commit this file.\n",
        );
        for (key, value) in entries {
            // Single quotes keep Compose from reading anything inside the value.
            debug_assert!(!value.contains('\''), "{key} cannot be single-quoted");
            let _ = writeln!(file, "{key}='{value}'");
        }
        file
    }

    /// How a process on the host reaches the stack's database.
    pub(crate) fn database_url(&self, ports: Ports) -> String {
        format!(
            "postgres://capsulet:{}@127.0.0.1:{}/capsulet",
            self.postgres_password, ports.postgres
        )
    }

    /// What a test needs to act as each identity. The service account's token
    /// only exists once the seed has created it.
    pub(crate) fn identities(&self, ports: Ports, service_token: &str) -> Value {
        let mut identities = serde_json::Map::new();
        for identity in BOOTSTRAP.iter().chain([&SERVICE]) {
            let token = if identity.alias == SERVICE.alias {
                service_token
            } else {
                self.token(identity.alias)
            };
            identities.insert(
                identity.alias.to_string(),
                json!({
                    "principal": identity.principal,
                    "kind": if identity.alias == SERVICE.alias { "service_account" } else { "user" },
                    "role": identity.role,
                    "project": identity.project,
                    "token": token,
                    "expires_at_unix": self.expires_at_unix,
                }),
            );
        }
        json!({
            "api_url": ports.api_url(),
            "dashboard_url": ports.dashboard_url(),
            "tenant": TENANT,
            "projects": PROJECTS
                .iter()
                .map(|(alias, id, name)| ((*alias).to_string(), json!({ "id": id, "name": name })))
                .collect::<serde_json::Map<_, _>>(),
            "resources": PROJECTS
                .iter()
                .map(|(alias, _, _)| ((*alias).to_string(), resources(alias)))
                .collect::<serde_json::Map<_, _>>(),
            "identities": identities,
            "dashboard_login": {
                "username": BOOTSTRAP[0].principal,
                "password": self.dashboard_password,
            },
        })
    }
}

/// 32 random bytes, hex encoded: comfortably over the API's 32-byte minimum.
fn random_secret() -> Result<String, String> {
    let mut bytes = [0_u8; 32];
    getrandom::fill(&mut bytes)
        .map_err(|error| format!("no randomness for a test secret: {error}"))?;
    Ok(bytes
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            let _ = write!(hex, "{byte:02x}");
            hex
        }))
}

/// The id and name of everything the seed creates in one project. Tests read
/// these from the fixture file rather than repeating them.
fn resources(alias: &str) -> Value {
    let name = title(alias);
    let resource = |id: String, name: String| json!({ "id": id, "name": name });
    json!({
        "job_definition": resource(format!("e2e_{alias}_hello"), format!("E2E {name} hello")),
        "memory_subgraph": resource(format!("e2e_{alias}_memory"), format!("E2E {name} memory")),
        "canonical_entity": resource(format!("e2e_{alias}_customer"), format!("{name} Customer")),
        "graph": resource(format!("e2e_{alias}_graph"), format!("E2E {name} graph")),
        "agent": resource(format!("e2e_{alias}_agent"), format!("E2E {name} agent")),
    })
}

fn title(alias: &str) -> String {
    let mut characters = alias.chars();
    characters.next().map_or_else(String::new, |first| {
        first.to_uppercase().chain(characters).collect()
    })
}

/// A blocking client for the stack's API.
pub(crate) struct Api {
    base: String,
    client: reqwest::blocking::Client,
}

impl Api {
    pub(crate) fn new(base: String) -> Result<Self, String> {
        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .map_err(|error| format!("could not build an HTTP client: {error}"))?;
        Ok(Self { base, client })
    }

    /// Sends one request and returns its status and JSON body (`null` when empty).
    fn call(
        &self,
        method: reqwest::Method,
        path: &str,
        token: &str,
        project: Option<&str>,
        body: Option<&Value>,
    ) -> Result<(u16, Value), String> {
        let label = format!("{method} {path}");
        let mut request = self
            .client
            .request(method, format!("{}{path}", self.base))
            .bearer_auth(token);
        if let Some(project) = project {
            request = request.header("x-capsulet-project-id", project);
        }
        if let Some(body) = body {
            request = request.json(body);
        }
        let response = request
            .send()
            .map_err(|error| format!("{label}: {error}"))?;
        let status = response.status().as_u16();
        let text = response
            .text()
            .map_err(|error| format!("{label}: {error}"))?;
        let body = if text.trim().is_empty() {
            Value::Null
        } else {
            serde_json::from_str(&text).unwrap_or(Value::String(text))
        };
        Ok((status, body))
    }

    /// Like [`Api::call`], but any status outside `expected` is an error.
    fn expect(
        &self,
        method: reqwest::Method,
        path: &str,
        token: &str,
        project: Option<&str>,
        body: Option<&Value>,
        expected: &[u16],
    ) -> Result<Value, String> {
        let label = format!("{method} {path}");
        let (status, response) = self.call(method, path, token, project, body)?;
        if expected.contains(&status) {
            Ok(response)
        } else {
            Err(format!(
                "{label} answered {status}, wanted {expected:?}: {response}"
            ))
        }
    }

    /// Waits until the API reports ready.
    pub(crate) fn wait_until_ready(&self, attempts: u32) -> Result<(), String> {
        for _ in 0..attempts {
            let ready = self
                .client
                .get(format!("{}/readyz", self.base))
                .send()
                .is_ok_and(|response| response.status().is_success());
            if ready {
                return Ok(());
            }
            thread::sleep(Duration::from_secs(2));
        }
        Err(format!("{} never reported ready", self.base))
    }
}

/// Seeds the fixture through the API and returns the new service account token.
///
/// Every step is an upsert or a create-if-absent, so seeding a seeded stack
/// leaves the same graph. The service account is the exception by design: its
/// token is only ever shown once, so each seed revokes the last one and issues
/// a fresh, expiring credential.
pub(crate) fn seed(api: &Api, secrets: &Secrets) -> Result<String, String> {
    use reqwest::Method;
    let admin = secrets.token("admin");
    for (_, id, name) in PROJECTS {
        api.expect(
            Method::PUT,
            &format!("/v1/projects/{id}"),
            admin,
            None,
            Some(&json!({ "name": name })),
            &[200],
        )?;
    }
    for (alias, id, _) in PROJECTS {
        seed_project(api, admin, alias, id)?;
    }

    let accounts = api.expect(
        Method::GET,
        "/v1/service-accounts",
        admin,
        Some(SERVICE.project),
        None,
        &[200],
    )?;
    for account in accounts["service_accounts"]
        .as_array()
        .into_iter()
        .flatten()
    {
        if account["name"] == SERVICE.principal && account["revoked_at"].is_null() {
            let id = account["id"]
                .as_str()
                .ok_or("a service account without an id")?;
            api.expect(
                Method::POST,
                &format!("/v1/service-accounts/{id}/revoke"),
                admin,
                Some(SERVICE.project),
                None,
                &[204],
            )?;
        }
    }
    let created = api.expect(
        Method::POST,
        "/v1/service-accounts",
        admin,
        Some(SERVICE.project),
        Some(&json!({
            "name": SERVICE.principal,
            "role": SERVICE.role,
            "tenant_id": TENANT,
            "project_id": SERVICE.project,
            "expires_at_unix": secrets.expires_at_unix,
        })),
        &[201],
    )?;
    created["token"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("the service account came back without a token: {created}"))
}

fn seed_project(api: &Api, admin: &str, alias: &str, project: &str) -> Result<(), String> {
    use reqwest::Method;
    let fixture = resources(alias);
    let id = |kind: &str| fixture[kind]["id"].as_str().unwrap_or_default().to_string();
    let name = |kind: &str| {
        fixture[kind]["name"]
            .as_str()
            .unwrap_or_default()
            .to_string()
    };

    // The artifact names its project, so a test can tell whose it downloaded.
    let script = [
        "from pathlib import Path".to_string(),
        "Path('/capsulet/artifacts').mkdir(parents=True, exist_ok=True)".to_string(),
        format!("Path('/capsulet/artifacts/report.txt').write_text('hello from {project}')"),
        format!("print('hello from {project}')"),
    ]
    .join("\n");
    let job = json!({
        "id": id("job_definition"),
        "name": name("job_definition"),
        "python_script": script,
    });
    let path = format!("/v1/job-definitions/{}", id("job_definition"));
    let (status, _) = api.call(Method::GET, &path, admin, Some(project), None)?;
    if status == 404 {
        api.expect(
            Method::POST,
            "/v1/job-definitions",
            admin,
            Some(project),
            Some(&job),
            &[201],
        )?;
    } else {
        // Puts back whatever a test may have changed.
        api.expect(Method::PUT, &path, admin, Some(project), Some(&job), &[200])?;
    }

    create_if_absent(
        api,
        admin,
        project,
        &format!("/v1/memory/subgraphs/{}", id("memory_subgraph")),
        "/v1/memory/subgraphs",
        &json!({
            "id": id("memory_subgraph"),
            "name": name("memory_subgraph"),
            "description": "Seeded by the stack fixture",
        }),
    )?;

    let entities = api.expect(
        Method::GET,
        "/v1/memory/canonical-entities",
        admin,
        Some(project),
        None,
        &[200],
    )?;
    if find_by_id(&entities, &id("canonical_entity")).is_none() {
        api.expect(
            Method::POST,
            "/v1/memory/canonical-entities",
            admin,
            Some(project),
            Some(&json!({
                "id": id("canonical_entity"),
                "entity_type": "Customer",
                "display_name": name("canonical_entity"),
                "aliases": [],
            })),
            &[200, 201],
        )?;
    }

    create_if_absent(
        api,
        admin,
        project,
        &format!("/v1/graphs/{}", id("graph")),
        "/v1/graphs",
        &graph(&id("graph"), &name("graph")),
    )?;
    create_if_absent(
        api,
        admin,
        project,
        &format!("/v1/agents/{}", id("agent")),
        "/v1/agents",
        &json!({
            "id": id("agent"),
            "name": name("agent"),
            "graph_id": id("graph"),
            "budget": { "max_steps": 3, "max_tokens": 1000, "max_seconds": 300, "max_cost_micros": 1000 },
            "termination_conditions": ["validator_pass"],
        }),
    )
}

fn create_if_absent(
    api: &Api,
    admin: &str,
    project: &str,
    path: &str,
    collection: &str,
    body: &Value,
) -> Result<(), String> {
    let (status, response) = api.call(reqwest::Method::GET, path, admin, Some(project), None)?;
    match status {
        200 => Ok(()),
        404 => api
            .expect(
                reqwest::Method::POST,
                collection,
                admin,
                Some(project),
                Some(body),
                &[200, 201],
            )
            .map(|_| ()),
        other => Err(format!("GET {path} answered {other}: {response}")),
    }
}

/// A three-node graph that needs no model: normalize a query, build a prompt
/// from it, and check the prompt.
fn graph(id: &str, name: &str) -> Value {
    let port = |id: &str, direction: &str, value_type: &str| json!({ "id": id, "direction": direction, "value_type": value_type });
    let to_port = |node: &str, port: &str, value_type: &str| json!({ "kind": "port", "node_id": node, "port_id": port, "value_type": value_type });
    json!({
        "id": id,
        "name": name,
        "transition_policy": { "mode": "static", "cycles_allowed": false },
        "nodes": [
            {
                "id": "normalize", "name": "Normalize", "kind": "query_normalizer",
                "ports": [port("in", "input", "user_query"), port("out", "output", "normalized_query")],
            },
            {
                "id": "prompt", "name": "Prompt", "kind": "prompt_builder",
                "ports": [port("query", "input", "normalized_query"), port("text", "output", "prompt")],
                "config": { "template": "Answer this: {{query}}" },
            },
            {
                "id": "check", "name": "Check", "kind": "validator",
                "ports": [port("text", "input", "prompt"), port("result", "output", "validation_result")],
                "config": { "rule.contains": "Answer this" },
            },
        ],
        "hyperedges": [
            {
                "id": "query-in",
                "sources": [{ "kind": "state_field", "field": "query", "value_type": "user_query" }],
                "targets": [to_port("normalize", "in", "user_query")],
            },
            {
                "id": "normal",
                "sources": [to_port("normalize", "out", "normalized_query")],
                "targets": [to_port("prompt", "query", "normalized_query")],
            },
            {
                "id": "checked",
                "sources": [to_port("prompt", "text", "prompt")],
                "targets": [to_port("check", "text", "prompt")],
            },
            {
                "id": "written",
                "sources": [to_port("prompt", "text", "prompt")],
                "targets": [{ "kind": "state_field", "field": "prompt", "value_type": "prompt" }],
            },
        ],
    })
}

/// The first object anywhere in `value` whose `id` is `id`.
fn find_by_id<'a>(value: &'a Value, id: &str) -> Option<&'a Value> {
    match value {
        Value::Object(map) if map.get("id").and_then(Value::as_str) == Some(id) => Some(value),
        Value::Object(map) => map.values().find_map(|child| find_by_id(child, id)),
        Value::Array(items) => items.iter().find_map(|child| find_by_id(child, id)),
        _ => None,
    }
}

/// The logical fixture graph: what the seed promises, and nothing that changes
/// from one seed to the next (timestamps, generated ids, tokens).
///
/// Two seeds of one stack must produce the same snapshot, and so must a seed
/// after a test suite has run against it.
pub(crate) fn snapshot(api: &Api, secrets: &Secrets, service_token: &str) -> Result<Value, String> {
    use reqwest::Method;
    let admin = secrets.token("admin");
    let projects = api.expect(Method::GET, "/v1/projects", admin, None, None, &[200])?;
    let mut fixture_projects = projects["projects"]
        .as_array()
        .cloned()
        .unwrap_or_default()
        .into_iter()
        .filter(|project| PROJECTS.iter().any(|(_, id, _)| project["id"] == *id))
        .map(|project| json!([project["tenant_id"], project["id"], project["name"]]))
        .collect::<Vec<_>>();
    fixture_projects.sort_by_key(Value::to_string);

    let accounts = api.expect(
        Method::GET,
        "/v1/service-accounts",
        admin,
        Some(SERVICE.project),
        None,
        &[200],
    )?;
    let active_service_accounts = accounts["service_accounts"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|account| account["revoked_at"].is_null())
        .map(|account| json!([account["name"], account["role"], account["project_id"]]))
        .collect::<Vec<_>>();

    Ok(json!({
        "identities": identity_snapshot(api, secrets, service_token)?,
        "projects": fixture_projects,
        "active_service_accounts": active_service_accounts,
        "resources": resource_snapshot(api, admin)?,
    }))
}

/// Who each identity authenticates as, and where it is a member.
fn identity_snapshot(api: &Api, secrets: &Secrets, service_token: &str) -> Result<Value, String> {
    let mut identities = serde_json::Map::new();
    for identity in BOOTSTRAP.iter().chain([&SERVICE]) {
        let token = if identity.alias == SERVICE.alias {
            service_token
        } else {
            secrets.token(identity.alias)
        };
        let me = api.expect(
            reqwest::Method::GET,
            "/v1/auth/me",
            token,
            None,
            None,
            &[200],
        )?;
        let mut memberships = me["project_memberships"]
            .as_array()
            .cloned()
            .unwrap_or_default()
            .into_iter()
            .map(|membership| {
                json!([
                    membership["tenant_id"],
                    membership["project_id"],
                    membership["role"]
                ])
            })
            .collect::<Vec<_>>();
        memberships.sort_by_key(Value::to_string);
        identities.insert(
            identity.alias.to_string(),
            json!({
                "name": me["name"],
                "role": me["role"],
                "platform_admin": me["platform_admin"],
                "project_memberships": memberships,
            }),
        );
    }
    Ok(Value::Object(identities))
}

/// The id and name of everything the seed created, in each project.
fn resource_snapshot(api: &Api, admin: &str) -> Result<Value, String> {
    let mut snapshot = serde_json::Map::new();
    for (alias, project, _) in PROJECTS {
        let fixture = resources(alias);
        let id = |kind: &str| fixture[kind]["id"].as_str().unwrap_or_default().to_string();
        let get = |path: String| {
            api.expect(
                reqwest::Method::GET,
                &path,
                admin,
                Some(project),
                None,
                &[200],
            )
        };
        let entities = get("/v1/memory/canonical-entities".to_string())?;
        let entity = find_by_id(&entities, &id("canonical_entity"))
            .cloned()
            .unwrap_or(Value::Null);
        let job = get(format!("/v1/job-definitions/{}", id("job_definition")))?;
        let subgraph = get(format!("/v1/memory/subgraphs/{}", id("memory_subgraph")))?;
        let graph = get(format!("/v1/graphs/{}", id("graph")))?;
        let agent = get(format!("/v1/agents/{}", id("agent")))?;
        snapshot.insert(
            alias.to_string(),
            json!({
                "job_definition": [job["id"], job["name"]],
                "memory_subgraph": [subgraph["id"], subgraph["name"]],
                "canonical_entity": [entity["id"], entity["display_name"]],
                "graph": [graph["id"], graph["name"]],
                "agent": [agent["id"], agent["name"], agent["graph_id"]],
            }),
        );
    }
    Ok(Value::Object(snapshot))
}

#[cfg(test)]
mod tests {
    use super::{Ports, Secrets};

    #[test]
    fn every_stack_gets_its_own_secrets() {
        let first = Secrets::generate(1_000).expect("secrets");
        let second = Secrets::generate(1_000).expect("secrets");
        let first_file = first.env_file(Ports::STANDARD);
        assert_ne!(first_file, second.env_file(Ports::STANDARD));
        for value in first.tokens.values() {
            assert_eq!(value.len(), 64);
            assert!(value.bytes().all(|byte| byte.is_ascii_hexdigit()));
        }
        assert!(!first_file.contains("change-me"));
        assert!(!first_file.contains("='admin'"));
    }

    #[test]
    fn bootstrap_credentials_expire_and_scope_non_admins_to_one_project() {
        let secrets = Secrets::generate(1_000).expect("secrets");
        let tokens: Vec<serde_json::Value> =
            serde_json::from_str(&secrets.api_tokens()).expect("tokens JSON");
        assert_eq!(tokens.len(), 3);
        for token in &tokens {
            assert_eq!(token["expires_at_unix"], 1_000 + 4 * 60 * 60);
            if token["role"] != "admin" {
                assert_eq!(token["project_id"], "e2e-alpha");
            }
        }
    }

    #[test]
    fn the_identities_file_names_every_role_and_both_projects() {
        let secrets = Secrets::generate(1_000).expect("secrets");
        let identities = secrets.identities(Ports::STANDARD, "service-token");
        for alias in ["admin", "operator", "viewer", "service"] {
            assert!(
                identities["identities"][alias]["token"].is_string(),
                "{alias}"
            );
        }
        assert_eq!(
            identities["identities"]["service"]["token"],
            "service-token"
        );
        assert_eq!(identities["projects"]["alpha"]["id"], "e2e-alpha");
        assert_eq!(identities["projects"]["beta"]["id"], "e2e-beta");
        assert_eq!(
            identities["resources"]["beta"]["agent"]["id"],
            "e2e_beta_agent"
        );
        assert_eq!(
            identities["resources"]["alpha"]["memory_subgraph"]["name"],
            "E2E Alpha memory"
        );
        assert_eq!(identities["api_url"], "http://127.0.0.1:8080");
    }
}
