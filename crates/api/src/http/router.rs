//! The router: every operation the endpoint contract declares, the middleware around them, and the limits they run under.

use std::{
    collections::BTreeSet,
    env,
    sync::Arc,
    time::{Duration, Instant},
};

use axum::{
    Router,
    extract::{DefaultBodyLimit, MatchedPath, Request, State},
    handler::Handler,
    http::{HeaderValue, StatusCode},
    middleware::{self, Next},
    response::Response,
    routing::{MethodFilter, MethodRouter, on},
};
use capsulet_observability::{self as observability, tracing::Instrument};
use capsulet_storage::ObjectStore;
use tower::limit::ConcurrencyLimitLayer;
use tower_governor::{
    GovernorLayer, errors::GovernorError, governor::GovernorConfigBuilder,
    key_extractor::KeyExtractor,
};
use tower_http::{
    compression::CompressionLayer, cors::CorsLayer, limit::RequestBodyLimitLayer,
    timeout::TimeoutLayer, trace::TraceLayer,
};

use super::activity::stream_activity_events;
use super::artifacts::{download_artifact, list_artifacts};
use super::health::{healthz, livez, metrics, openapi_spec, readyz};
use super::identity::{current_principal, require_auth};
use super::jobs::{
    cancel_run, create_job_definition, create_run, delete_job_definition, get_job_definition,
    get_job_definition_source, get_run, get_run_logs, list_execution_pools, list_host_groups,
    list_job_definitions, list_runs, stream_run_logs, update_job_definition,
};
use super::projects::{
    create_service_account, delete_project_membership, list_audit_events, list_project_memberships,
    list_projects, list_service_accounts, revoke_service_account, upsert_project,
    upsert_project_membership,
};
use super::workflows::{
    cancel_workflow_run, create_workflow, delete_workflow, get_topology, get_workflow,
    get_workflow_editability, get_workflow_run, get_workflow_run_logs, list_workflow_runs,
    list_workflows, remove_workflow_run, resume_workflow_run, stream_workflow_run_logs,
    trigger_automation, update_workflow,
};
use crate::{state::AppState, store::ApiStore};

pub(super) const DEFAULT_REQUEST_TIMEOUT_SECONDS: u64 = 30;

pub(super) const DEFAULT_REQUEST_BODY_LIMIT_BYTES: usize = 4 * 1024 * 1024;

pub(super) const DEFAULT_CONCURRENCY_LIMIT: usize = 256;

pub(super) const DEFAULT_RATE_LIMIT_PER_SECOND: u64 = 50;

pub(super) const DEFAULT_RATE_LIMIT_BURST: u32 = 100;

#[derive(Clone)]
pub(super) struct ForwardedIpOrLocalKeyExtractor;

impl KeyExtractor for ForwardedIpOrLocalKeyExtractor {
    type Key = String;

    /// Keys buckets on the client address only.
    ///
    /// The credential must never take part in the key: an unauthenticated caller chooses it
    /// freely, so keying on it would hand every request a fresh bucket and leave the
    /// authentication path — the one thing this limiter exists to protect — unthrottled.
    fn extract<T>(&self, request: &axum::http::Request<T>) -> Result<Self::Key, GovernorError> {
        let headers = request.headers();
        Ok(headers
            .get("x-forwarded-for")
            .and_then(|value| value.to_str().ok())
            .and_then(|value| value.split(',').next())
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .or_else(|| {
                headers
                    .get("x-real-ip")
                    .and_then(|value| value.to_str().ok())
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
            })
            .unwrap_or("local")
            .to_string())
    }
}

#[derive(Debug, Clone)]
pub(super) struct RequestId(pub(super) String);

/// The public routes, each registered from its endpoint contract.
///
/// A handler is registered by operation id and nothing else: the contract gives
/// the method and the path, and decides whether the route is wrapped in
/// authentication, so a route cannot answer on a method, at a path or without the
/// authentication its contract does not declare. The authentication layer is
/// handed the same contract, so the policy it enforces is the one of the route
/// that matched, not a second guess from the request path. `finish` refuses a
/// router that leaves any declared operation unregistered.
pub(super) struct ContractRouter<S, O> {
    pub(super) state: AppState<S, O>,
    pub(super) router: Router<AppState<S, O>>,
    pub(super) registered: BTreeSet<&'static str>,
}

impl<S, O> ContractRouter<S, O>
where
    S: ApiStore,
    O: ObjectStore,
{
    fn new(state: AppState<S, O>) -> Self {
        Self {
            state,
            router: Router::new(),
            registered: BTreeSet::new(),
        }
    }

    /// Registers the handler of one operation.
    fn operation<H, T>(self, operation_id: &'static str, handler: H) -> Self
    where
        H: Handler<T, AppState<S, O>>,
        T: 'static,
    {
        self.register(operation_id, |method| on(method, handler))
    }

    /// Registers the handler of one operation whose bodies may be at most `limit` bytes.
    fn operation_with_body_limit<H, T>(
        self,
        operation_id: &'static str,
        handler: H,
        limit: usize,
    ) -> Self
    where
        H: Handler<T, AppState<S, O>>,
        T: 'static,
    {
        self.register(operation_id, |method| {
            on(method, handler).layer(DefaultBodyLimit::max(limit))
        })
    }

    fn register(
        mut self,
        operation_id: &'static str,
        route: impl FnOnce(MethodFilter) -> MethodRouter<AppState<S, O>>,
    ) -> Self {
        let contract = crate::find_operation(operation_id)
            .unwrap_or_else(|| panic!("unknown route operation ID: {operation_id}"));
        assert!(
            self.registered.insert(operation_id),
            "operation registered more than once: {operation_id}"
        );
        let mut route = route(method_filter(contract.method));
        if contract.authentication.requires_authentication() {
            route = route.route_layer(middleware::from_fn_with_state(
                self.state.clone(),
                move |state: State<AppState<S, O>>, request: Request, next: Next| {
                    require_auth(state, contract, request, next)
                },
            ));
        }
        // Every failure, including a rejection raised before the handler runs,
        // leaves as the contract's JSON error.
        route = route.route_layer(middleware::from_fn(crate::error::json_errors));
        // In tests, every response is held to this contract.
        #[cfg(any(test, feature = "contract-conformance"))]
        {
            route = route.route_layer(middleware::from_fn(move |request: Request, next: Next| {
                crate::openapi::conformance::check(contract, request, next)
            }));
        }
        self.router = self.router.route(contract.path, route);
        self
    }

    /// The routes, once every declared operation has one.
    fn finish(self) -> Router<AppState<S, O>> {
        let missing = crate::endpoint_contracts()
            .iter()
            .map(|endpoint| endpoint.operation_id)
            .filter(|operation_id| !self.registered.contains(operation_id))
            .collect::<Vec<_>>();
        assert!(
            missing.is_empty(),
            "runtime route registrations must exactly cover the endpoint contract; unregistered: {missing:?}"
        );
        self.router
    }
}

pub(super) const fn method_filter(method: crate::HttpMethod) -> MethodFilter {
    match method {
        crate::HttpMethod::Get => MethodFilter::GET,
        crate::HttpMethod::Post => MethodFilter::POST,
        crate::HttpMethod::Put => MethodFilter::PUT,
        crate::HttpMethod::Delete => MethodFilter::DELETE,
        crate::HttpMethod::Patch => MethodFilter::PATCH,
    }
}

#[allow(clippy::missing_panics_doc)]
#[expect(
    clippy::too_many_lines,
    reason = "the route table is declarative and kept as one list, so every public route can be read in one place"
)]
pub fn router<S, O>(state: AppState<S, O>) -> Router
where
    S: ApiStore,
    O: ObjectStore,
{
    let rate_limit = Arc::new(
        GovernorConfigBuilder::default()
            .key_extractor(ForwardedIpOrLocalKeyExtractor)
            .per_millisecond(replenish_period_millis(env_u64(
                "CAPSULET_API_RATE_LIMIT_PER_SECOND",
                DEFAULT_RATE_LIMIT_PER_SECOND,
            )))
            .burst_size(env_u32(
                "CAPSULET_API_RATE_LIMIT_BURST",
                DEFAULT_RATE_LIMIT_BURST,
            ))
            .finish()
            .expect("rate limit configuration is valid"),
    );
    // Every public operation, each once. Its method, path and whether it needs
    // authentication come from its endpoint contract, never from this list.
    let routes = ContractRouter::new(state.clone())
        .operation("getCurrentPrincipal", current_principal)
        .operation("listProjects", list_projects)
        .operation("upsertProject", upsert_project)
        .operation("listProjectMemberships", list_project_memberships)
        .operation("upsertProjectMembership", upsert_project_membership)
        .operation("deleteProjectMembership", delete_project_membership)
        .operation("listServiceAccounts", list_service_accounts)
        .operation("createServiceAccount", create_service_account)
        .operation("revokeServiceAccount", revoke_service_account)
        .operation("listJobDefinitions", list_job_definitions)
        .operation("createJobDefinition", create_job_definition)
        .operation("getJobDefinition", get_job_definition)
        .operation("updateJobDefinition", update_job_definition)
        .operation("deleteJobDefinition", delete_job_definition)
        .operation("getJobDefinitionSource", get_job_definition_source)
        .operation("listExecutionPools", list_execution_pools)
        .operation("listAuditEvents", list_audit_events)
        .operation("listHostGroups", list_host_groups)
        .operation("getTopology", get_topology)
        .operation("listWorkflows", list_workflows)
        .operation("createWorkflow", create_workflow)
        .operation("getWorkflow", get_workflow)
        .operation("updateWorkflow", update_workflow)
        .operation("deleteWorkflow", delete_workflow)
        .operation("getWorkflowEditability", get_workflow_editability)
        .operation("listGraphs", crate::graphs::list_graphs)
        .operation("createGraph", crate::graphs::create_graph)
        .operation("getGraph", crate::graphs::get_graph)
        .operation("listAgents", crate::graphs::list_agents)
        .operation("createAgent", crate::graphs::create_agent)
        .operation("getAgent", crate::graphs::get_agent)
        .operation("startAgentRun", crate::graphs::start_agent_run)
        .operation("listAgentRuns", crate::graphs::list_agent_runs)
        .operation("getAgentRun", crate::graphs::get_agent_run)
        .operation("getAgentRunEvents", crate::graphs::get_agent_run_events)
        .operation("cancelAgentRun", crate::graphs::cancel_agent_run)
        .operation("decideAgentRun", crate::graphs::decide_agent_run)
        .operation(
            "eraseAgentRunPayloads",
            crate::graphs::erase_agent_run_payloads,
        )
        .operation(
            "exportAgentRunBundle",
            crate::graphs::export_agent_run_bundle,
        )
        .operation("explainAgentRun", crate::graphs::explain_agent_run)
        .operation("askReasoning", crate::reasoning::ask)
        .operation("listCertificates", crate::reasoning::list_certificates)
        .operation("installPack", crate::packs::install_pack)
        .operation("listPacks", crate::packs::list_packs)
        .operation("getPack", crate::packs::get_pack)
        .operation(
            "getReviewedCertificate",
            crate::reasoning::get_reviewed_certificate,
        )
        .operation("reviewReading", crate::reasoning::review_reading)
        .operation("listOpenReadings", crate::reasoning::list_open_readings)
        .operation(
            "registerIrDefinition",
            crate::assurance::register_definition,
        )
        .operation("listIrDefinitions", crate::assurance::list_definitions)
        .operation(
            "getIrDefinitionVersion",
            crate::assurance::get_definition_version,
        )
        .operation("startIrRun", crate::assurance::start_ir_run)
        .operation("listIrRuns", crate::assurance::list_ir_runs)
        .operation("getIrRun", crate::assurance::get_ir_run)
        .operation("getIrRunEvents", crate::assurance::get_ir_run_events)
        .operation(
            "listAssuranceCertificates",
            crate::assurance::list_certificates,
        )
        .operation("getAssuranceCertificate", crate::assurance::get_certificate)
        .operation(
            "getAssuranceCertificateBundle",
            crate::assurance::get_certificate_bundle,
        )
        .operation("listMemorySources", crate::memory::list_sources)
        .operation("createMemorySource", crate::memory::create_source)
        .operation("getMemorySource", crate::memory::get_source)
        .operation("listMemoryEvidence", crate::memory::list_evidence)
        .operation("createMemoryEvidence", crate::memory::create_evidence)
        .operation("getMemoryEvidence", crate::memory::get_evidence)
        .operation("listMemoryEntities", crate::memory::list_entities)
        .operation("createMemoryEntity", crate::memory::create_entity)
        .operation("getMemoryEntity", crate::memory::get_entity)
        .operation("listMemoryClaims", crate::memory::list_claims)
        .operation("createMemoryClaim", crate::memory::create_claim)
        .operation("getMemoryClaim", crate::memory::get_claim)
        .operation("listMemoryEvents", crate::memory::list_events)
        .operation("createMemoryEvent", crate::memory::create_event)
        .operation("getMemoryEvent", crate::memory::get_event)
        .operation("listMemoryRelationships", crate::memory::list_relationships)
        .operation(
            "createMemoryRelationship",
            crate::memory::create_relationship,
        )
        .operation("getMemoryRelationship", crate::memory::get_relationship)
        .operation("listMemoryContracts", crate::memory::list_contracts)
        .operation("createMemoryContract", crate::memory::create_contract)
        .operation("getMemoryContract", crate::memory::get_contract)
        .operation("listMemorySubgraphs", crate::memory::list_subgraphs)
        .operation("createMemorySubgraph", crate::memory::create_subgraph)
        .operation("getMemorySubgraph", crate::memory::get_subgraph)
        .operation("activateMemorySubgraph", crate::memory::activate_subgraph)
        .operation(
            "createMemorySubgraphMember",
            crate::memory::create_subgraph_member,
        )
        .operation(
            "listCanonicalEntities",
            crate::memory::list_canonical_entities,
        )
        .operation(
            "createCanonicalEntity",
            crate::memory::create_canonical_entity,
        )
        .operation(
            "listEntityResolutions",
            crate::memory::list_entity_resolutions,
        )
        .operation(
            "createEntityResolution",
            crate::memory::create_entity_resolution,
        )
        .operation(
            "confirmEntityResolution",
            crate::memory::confirm_entity_resolution,
        )
        .operation(
            "rejectEntityResolution",
            crate::memory::reject_entity_resolution,
        )
        .operation("listClaimConflicts", crate::memory::list_claim_conflicts)
        .operation(
            "resolveClaimConflict",
            crate::memory::resolve_claim_conflict,
        )
        .operation(
            "dismissClaimConflict",
            crate::memory::dismiss_claim_conflict,
        )
        .operation("createSummaryTrace", crate::memory::create_summary_trace)
        .operation(
            "createEntityGraphAttachment",
            crate::memory::create_entity_graph_attachment,
        )
        .operation("listIngestionConnectors", crate::ingestion::list_connectors)
        .operation(
            "createIngestionConnector",
            crate::ingestion::create_connector,
        )
        .operation("getIngestionConnector", crate::ingestion::get_connector)
        .operation("runIngestionConnector", crate::ingestion::run_connector)
        .operation("listIngestionRuns", crate::ingestion::list_runs)
        .operation("getIngestionRun", crate::ingestion::get_run)
        .operation(
            "listIngestionReviewClaims",
            crate::ingestion::list_review_claims,
        )
        .operation(
            "approveIngestionReviewClaim",
            crate::ingestion::approve_review_claim,
        )
        .operation(
            "rejectIngestionReviewClaim",
            crate::ingestion::reject_review_claim,
        )
        .operation("createSubgraphEdge", crate::memory::create_subgraph_edge)
        .operation("listAutomations", crate::automations::list_automations)
        .operation("createAutomation", crate::automations::create_automation)
        .operation("getAutomation", crate::automations::get_automation)
        .operation("updateAutomation", crate::automations::update_automation)
        .operation("deleteAutomation", crate::automations::delete_automation)
        .operation("enableAutomation", crate::automations::enable_automation)
        .operation("disableAutomation", crate::automations::disable_automation)
        .operation(
            "listAutomationTriggers",
            crate::automations::list_automation_triggers,
        )
        .operation("triggerAutomation", trigger_automation)
        .operation(
            "listTriggerPlugins",
            crate::automations::list_trigger_plugins,
        )
        .operation(
            "createTriggerPlugin",
            crate::automations::create_trigger_plugin,
        )
        .operation("getTriggerPlugin", crate::automations::get_trigger_plugin)
        .operation("listWorkflowRuns", list_workflow_runs)
        .operation("streamActivityEvents", stream_activity_events)
        .operation("getWorkflowRun", get_workflow_run)
        .operation("getWorkflowRunLogs", get_workflow_run_logs)
        .operation("streamWorkflowRunLogs", stream_workflow_run_logs)
        .operation("removeWorkflowRun", remove_workflow_run)
        .operation("cancelWorkflowRun", cancel_workflow_run)
        .operation("resumeWorkflowRun", resume_workflow_run)
        .operation("listJobRuns", list_runs)
        .operation("createJobRun", create_run)
        .operation("getJobRun", get_run)
        .operation("cancelJobRun", cancel_run)
        .operation("getJobRunLogs", get_run_logs)
        .operation("streamJobRunLogs", stream_run_logs)
        .operation("listJobArtifacts", list_artifacts)
        .operation("downloadJobArtifact", download_artifact)
        .operation("getHealth", healthz::<S, O>)
        .operation("getLiveness", livez)
        .operation("getReadiness", readyz::<S, O>)
        .operation("getMetrics", metrics::<S, O>)
        .operation("getOpenApi", openapi_spec)
        .operation_with_body_limit("ingestWebhook", crate::webhooks::ingest::<S, O>, 1_048_576)
        .finish();

    routes
        .with_state(state)
        .layer(middleware::from_fn(request_context))
        .layer(TraceLayer::new_for_http())
        .layer(CompressionLayer::new())
        .layer(CorsLayer::permissive())
        .layer(TimeoutLayer::with_status_code(
            StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(env_u64(
                "CAPSULET_API_REQUEST_TIMEOUT_SECONDS",
                DEFAULT_REQUEST_TIMEOUT_SECONDS,
            )),
        ))
        .layer(RequestBodyLimitLayer::new(env_usize(
            "CAPSULET_API_REQUEST_BODY_LIMIT_BYTES",
            DEFAULT_REQUEST_BODY_LIMIT_BYTES,
        )))
        .layer(ConcurrencyLimitLayer::new(env_usize(
            "CAPSULET_API_CONCURRENCY_LIMIT",
            DEFAULT_CONCURRENCY_LIMIT,
        )))
        .layer(GovernorLayer::new(rate_limit))
}

/// The label for requests no route matched.
pub(super) const UNMATCHED_ROUTE: &str = "unmatched";

pub(super) async fn request_context(mut request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let path = request.uri().path().to_string();
    // The route template, never the path: a path carries ids, and anybody can send any
    // path, so a label taken from it has no bound on how many values it can take.
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map_or(UNMATCHED_ROUTE, MatchedPath::as_str)
        .to_string();
    let request_id = observability::request_id(
        request
            .headers()
            .get("x-request-id")
            .and_then(|value| value.to_str().ok()),
    );
    request
        .extensions_mut()
        .insert(RequestId(request_id.clone()));
    let span = observability::tracing::info_span!(
        "http.request",
        request.id = %request_id,
        http.method = %method,
        http.route = %route,
        url.path = %path,
        http.status_code = observability::tracing::field::Empty,
        duration.ms = observability::tracing::field::Empty,
    );
    // A caller that is already tracing hands its context over in `traceparent`, and
    // this request joins that trace. Only the W3C header is read, and a malformed one
    // is ignored. A parent-based sampler follows the caller's sampled flag, so a
    // client can ask for its own requests to be traced; it cannot see anything by it.
    observability::continue_trace(
        &span,
        request
            .headers()
            .get("traceparent")
            .and_then(|value| value.to_str().ok()),
    );
    async move {
        let started = Instant::now();
        let mut response = next.run(request).await;
        let status = response.status();
        let elapsed = started.elapsed();
        observability::record_http_request(method.as_str(), &route, status.as_u16(), elapsed);
        observability::tracing::Span::current()
            .record("http.status_code", status.as_u16())
            .record("duration.ms", elapsed.as_millis());
        if let Ok(value) = HeaderValue::from_str(&request_id) {
            response.headers_mut().insert("x-request-id", value);
        }
        response
    }
    .instrument(span)
    .await
}

/// Converts a requests-per-second budget into the quota replenish interval that
/// `tower_governor` expects.
///
/// `GovernorConfigBuilder::per_second` takes the number of *seconds between* single-cell
/// replenishments, not a per-second request budget, so passing the configured rate straight
/// through throttles callers to one request every `rate` seconds once the burst is spent.
pub(super) fn replenish_period_millis(requests_per_second: u64) -> u64 {
    (1_000 / requests_per_second.max(1)).max(1)
}

pub(super) fn env_u64(name: &str, default: u64) -> u64 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

pub(super) fn env_u32(name: &str, default: u32) -> u32 {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<u32>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

pub(super) fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

#[cfg(test)]
mod endpoint_policy_tests {
    use axum::http::Method;

    /// The scope the endpoint contract names for a route, as the router checks it.
    fn required_scope(method: &Method, path: &str) -> Option<&'static str> {
        crate::find_endpoint(method.as_str(), path)
            .map(|endpoint| endpoint.required_permission.as_str())
    }

    #[test]
    fn unregistered_route_should_not_receive_a_prefix_based_permission() {
        assert_eq!(required_scope(&Method::GET, "/v1/unregistered"), None);
    }
}

#[cfg(test)]
mod rate_limit_tests {
    use super::{DEFAULT_RATE_LIMIT_PER_SECOND, replenish_period_millis};

    #[test]
    fn converts_requests_per_second_into_replenish_interval() {
        assert_eq!(replenish_period_millis(50), 20);
        assert_eq!(replenish_period_millis(1), 1_000);
        assert_eq!(replenish_period_millis(1_000), 1);
    }

    #[test]
    fn clamps_degenerate_rates_to_a_non_zero_interval() {
        // `per_millisecond(0)` is rejected by tower_governor, and a zero rate would
        // otherwise divide by zero.
        assert_eq!(replenish_period_millis(0), 1_000);
        assert_eq!(replenish_period_millis(u64::MAX), 1);
    }

    #[test]
    fn default_rate_replenishes_far_faster_than_once_per_configured_second() {
        // Regression guard: passing the rate straight to `per_second` produced a
        // 50_000 ms interval, i.e. one request per 50 seconds after the burst.
        assert!(replenish_period_millis(DEFAULT_RATE_LIMIT_PER_SECOND) < 1_000);
    }
}

#[cfg(test)]
mod rate_limit_key_tests {
    use axum::http::Request;
    use tower_governor::key_extractor::KeyExtractor;

    use super::ForwardedIpOrLocalKeyExtractor;

    fn key(headers: &[(&str, &str)]) -> String {
        let mut builder = Request::builder();
        for (name, value) in headers {
            builder = builder.header(*name, *value);
        }
        let request = builder.body(()).expect("request builds");
        ForwardedIpOrLocalKeyExtractor
            .extract(&request)
            .expect("key extraction never fails")
    }

    #[test]
    fn rotating_the_bearer_token_does_not_move_the_caller_to_a_new_bucket() {
        let first = key(&[
            ("authorization", "Bearer aaaaaaaaaaaaaaaa"),
            ("x-forwarded-for", "203.0.113.7"),
        ]);
        let second = key(&[
            ("authorization", "Bearer bbbbbbbbbbbbbbbb"),
            ("x-forwarded-for", "203.0.113.7"),
        ]);
        assert_eq!(first, second);
        assert_eq!(first, "203.0.113.7");
    }

    #[test]
    fn credentials_alone_never_produce_a_bucket() {
        assert_eq!(key(&[("authorization", "Bearer cst_secret")]), "local");
    }
}
