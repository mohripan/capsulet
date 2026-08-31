use serde_json::Value;

fn serialize<T: utoipa::PartialSchema>() -> Value {
    serde_json::to_value(T::schema()).expect("derived wire schemas serialize to JSON")
}

fn alias_schema(name: &str) -> Option<Value> {
    Some(match name {
        "CreateMemorySourceRequest" => serialize::<crate::memory::CreateSourceRequest>(),
        "MemorySourceResponse" => serialize::<crate::memory::SourceResponse>(),
        "CreateMemoryEvidenceRequest" => serialize::<crate::memory::CreateEvidenceRequest>(),
        "MemoryEvidenceResponse" => serialize::<crate::memory::EvidenceResponse>(),
        "CreateMemoryEntityRequest" => serialize::<crate::memory::CreateEntityRequest>(),
        "MemoryEntityResponse" => serialize::<crate::memory::EntityResponse>(),
        "CreateMemoryClaimRequest" => serialize::<crate::memory::CreateClaimRequest>(),
        "MemoryClaimResponse" => serialize::<crate::memory::ClaimResponse>(),
        "CreateMemoryEventRequest" => serialize::<crate::memory::CreateEventRequest>(),
        "MemoryEventResponse" => serialize::<crate::memory::EventResponse>(),
        "CreateMemoryRelationshipRequest" => {
            serialize::<crate::memory::CreateRelationshipRequest>()
        }
        "MemoryRelationshipResponse" => serialize::<crate::memory::RelationshipResponse>(),
        _ => return None,
    })
}

fn schema_chunk_1(name: &str) -> Option<Value> {
    Some(match name {
        "HealthResponse" => serialize::<crate::models::HealthResponse>(),
        "CreateServiceAccountRequest" => serialize::<crate::models::CreateServiceAccountRequest>(),
        "ServiceAccountResponse" => serialize::<crate::models::ServiceAccountResponse>(),
        "CreateServiceAccountResponse" => {
            serialize::<crate::models::CreateServiceAccountResponse>()
        }
        "ListServiceAccountsResponse" => serialize::<crate::models::ListServiceAccountsResponse>(),
        "ProjectResponse" => serialize::<crate::models::ProjectResponse>(),
        "ListProjectsResponse" => serialize::<crate::models::ListProjectsResponse>(),
        "UpsertProjectMembershipRequest" => {
            serialize::<crate::models::UpsertProjectMembershipRequest>()
        }
        "ProjectMembershipResponse" => serialize::<crate::models::ProjectMembershipResponse>(),
        "ListProjectMembershipsResponse" => {
            serialize::<crate::models::ListProjectMembershipsResponse>()
        }
        "CreateRunRequest" => serialize::<crate::models::CreateRunRequest>(),
        "CreateJobDefinitionRequest" => serialize::<crate::models::CreateJobDefinitionRequest>(),
        "CreateWorkflowRequest" => serialize::<crate::models::CreateWorkflowRequest>(),
        "CreateGraphRequest" => serialize::<crate::models::CreateGraphRequest>(),
        "GraphNodeRequest" => serialize::<crate::models::GraphNodeRequest>(),
        "GraphPortRequest" => serialize::<crate::models::GraphPortRequest>(),
        "GraphHyperedgeRequest" => serialize::<crate::models::GraphHyperedgeRequest>(),
        "HyperedgeEndpointRequest" => serialize::<crate::models::HyperedgeEndpointRequest>(),
        _ => return None,
    })
}

fn schema_chunk_2(name: &str) -> Option<Value> {
    Some(match name {
        "GraphTransitionPolicyRequest" => {
            serialize::<crate::models::GraphTransitionPolicyRequest>()
        }
        "CreateAgentRequest" => serialize::<crate::models::CreateAgentRequest>(),
        "AgentBudgetRequest" => serialize::<crate::models::AgentBudgetRequest>(),
        "StartAgentRunRequest" => serialize::<crate::models::StartAgentRunRequest>(),
        "JobDefinitionSourceResponse" => serialize::<crate::models::JobDefinitionSourceResponse>(),
        "WorkflowEditabilityResponse" => serialize::<crate::models::WorkflowEditabilityResponse>(),
        "TopologyResponse" => serialize::<crate::models::TopologyResponse>(),
        "TopologyNodeResponse" => serialize::<crate::models::TopologyNodeResponse>(),
        "TopologyEdgeResponse" => serialize::<crate::models::TopologyEdgeResponse>(),
        "CreateWorkflowStepRequest" => serialize::<crate::models::CreateWorkflowStepRequest>(),
        "CreateAutomationRequest" => serialize::<crate::models::CreateAutomationRequest>(),
        "CreateAutomationTriggerRequest" => {
            serialize::<crate::models::CreateAutomationTriggerRequest>()
        }
        "CreateTriggerPluginRequest" => serialize::<crate::models::CreateTriggerPluginRequest>(),
        "ListRunsQuery" => serialize::<crate::models::ListRunsQuery>(),
        "CreateWorkflowDependencyRequest" => {
            serialize::<crate::models::CreateWorkflowDependencyRequest>()
        }
        "ListWorkflowRunsQuery" => serialize::<crate::models::ListWorkflowRunsQuery>(),
        "ListJobDefinitionsQuery" => serialize::<crate::models::ListJobDefinitionsQuery>(),
        "ListRunsResponse" => serialize::<crate::models::ListRunsResponse>(),
        _ => return None,
    })
}

fn schema_chunk_3(name: &str) -> Option<Value> {
    Some(match name {
        "ListJobDefinitionsResponse" => serialize::<crate::models::ListJobDefinitionsResponse>(),
        "ListExecutionPoolsResponse" => serialize::<crate::models::ListExecutionPoolsResponse>(),
        "ListHostGroupsResponse" => serialize::<crate::models::ListHostGroupsResponse>(),
        "ListWorkflowsResponse" => serialize::<crate::models::ListWorkflowsResponse>(),
        "ListGraphsResponse" => serialize::<crate::models::ListGraphsResponse>(),
        "ListAgentsResponse" => serialize::<crate::models::ListAgentsResponse>(),
        "ListAgentRunsResponse" => serialize::<crate::models::ListAgentRunsResponse>(),
        "ListAutomationsResponse" => serialize::<crate::models::ListAutomationsResponse>(),
        "ListAutomationTriggersResponse" => {
            serialize::<crate::models::ListAutomationTriggersResponse>()
        }
        "ListTriggerPluginsResponse" => serialize::<crate::models::ListTriggerPluginsResponse>(),
        "ListWorkflowRunsResponse" => serialize::<crate::models::ListWorkflowRunsResponse>(),
        "WorkflowRunLogsResponse" => serialize::<crate::models::WorkflowRunLogsResponse>(),
        "WorkflowRunLogEntryResponse" => serialize::<crate::models::WorkflowRunLogEntryResponse>(),
        "ExecutionPoolResponse" => serialize::<crate::models::ExecutionPoolResponse>(),
        "HostGroupResponse" => serialize::<crate::models::HostGroupResponse>(),
        "JobDefinitionResponse" => serialize::<crate::models::JobDefinitionResponse>(),
        "WorkflowResponse" => serialize::<crate::models::WorkflowResponse>(),
        "GraphResponse" => serialize::<crate::models::GraphResponse>(),
        _ => return None,
    })
}

fn schema_chunk_4(name: &str) -> Option<Value> {
    Some(match name {
        "GraphNodeResponse" => serialize::<crate::models::GraphNodeResponse>(),
        "GraphPortResponse" => serialize::<crate::models::GraphPortResponse>(),
        "GraphHyperedgeResponse" => serialize::<crate::models::GraphHyperedgeResponse>(),
        "HyperedgeEndpointResponse" => serialize::<crate::models::HyperedgeEndpointResponse>(),
        "GraphTransitionPolicyResponse" => {
            serialize::<crate::models::GraphTransitionPolicyResponse>()
        }
        "AgentResponse" => serialize::<crate::models::AgentResponse>(),
        "AgentBudgetResponse" => serialize::<crate::models::AgentBudgetResponse>(),
        "AgentRunResponse" => serialize::<crate::models::AgentRunResponse>(),
        "AuditEventResponse" => serialize::<crate::models::AuditEventResponse>(),
        "ListAuditEventsResponse" => serialize::<crate::models::ListAuditEventsResponse>(),
        "WorkflowDependencyResponse" => serialize::<crate::models::WorkflowDependencyResponse>(),
        "WorkflowStepResponse" => serialize::<crate::models::WorkflowStepResponse>(),
        "AutomationResponse" => serialize::<crate::models::AutomationResponse>(),
        "TriggerResponse" => serialize::<crate::models::TriggerResponse>(),
        "TriggerPluginResponse" => serialize::<crate::models::TriggerPluginResponse>(),
        "WorkflowRunResponse" => serialize::<crate::models::WorkflowRunResponse>(),
        "WorkflowStepRunResponse" => serialize::<crate::models::WorkflowStepRunResponse>(),
        "JobRunResponse" => serialize::<crate::models::JobRunResponse>(),
        _ => return None,
    })
}

fn schema_chunk_5(name: &str) -> Option<Value> {
    Some(match name {
        "JobRunLogsResponse" => serialize::<crate::models::JobRunLogsResponse>(),
        "ListArtifactsResponse" => serialize::<crate::models::ListArtifactsResponse>(),
        "ArtifactResponse" => serialize::<crate::models::ArtifactResponse>(),
        _ => return None,
    })
}

fn schema_chunk_6(name: &str) -> Option<Value> {
    Some(match name {
        "CreateSourceRequest" => serialize::<crate::memory::CreateSourceRequest>(),
        "CreateEvidenceRequest" => serialize::<crate::memory::CreateEvidenceRequest>(),
        "CreateEntityRequest" => serialize::<crate::memory::CreateEntityRequest>(),
        "CreateClaimRequest" => serialize::<crate::memory::CreateClaimRequest>(),
        "CreateEventRequest" => serialize::<crate::memory::CreateEventRequest>(),
        "CreateRelationshipRequest" => serialize::<crate::memory::CreateRelationshipRequest>(),
        "CreateMemoryContractRequest" => serialize::<crate::memory::CreateMemoryContractRequest>(),
        "CreateMemorySubgraphRequest" => serialize::<crate::memory::CreateMemorySubgraphRequest>(),
        "ActivateMemorySubgraphRequest" => {
            serialize::<crate::memory::ActivateMemorySubgraphRequest>()
        }
        "CreateMemorySubgraphMemberRequest" => {
            serialize::<crate::memory::CreateMemorySubgraphMemberRequest>()
        }
        "CreateCanonicalEntityRequest" => {
            serialize::<crate::memory::CreateCanonicalEntityRequest>()
        }
        "CreateEntityResolutionRequest" => {
            serialize::<crate::memory::CreateEntityResolutionRequest>()
        }
        "ListEntityResolutionsQuery" => serialize::<crate::memory::ListEntityResolutionsQuery>(),
        "ListClaimConflictsQuery" => serialize::<crate::memory::ListClaimConflictsQuery>(),
        "ResolveClaimConflictRequest" => serialize::<crate::memory::ResolveClaimConflictRequest>(),
        "CreateSummaryTraceRequest" => serialize::<crate::memory::CreateSummaryTraceRequest>(),
        "CreateEntityGraphAttachmentRequest" => {
            serialize::<crate::memory::CreateEntityGraphAttachmentRequest>()
        }
        "CreateSubgraphEdgeRequest" => serialize::<crate::memory::CreateSubgraphEdgeRequest>(),
        _ => return None,
    })
}

fn schema_chunk_7(name: &str) -> Option<Value> {
    Some(match name {
        "SourceResponse" => serialize::<crate::memory::SourceResponse>(),
        "EvidenceResponse" => serialize::<crate::memory::EvidenceResponse>(),
        "EntityResponse" => serialize::<crate::memory::EntityResponse>(),
        "ClaimResponse" => serialize::<crate::memory::ClaimResponse>(),
        "EventResponse" => serialize::<crate::memory::EventResponse>(),
        "RelationshipResponse" => serialize::<crate::memory::RelationshipResponse>(),
        "MemoryContractResponse" => serialize::<crate::memory::MemoryContractResponse>(),
        "MemorySubgraphResponse" => serialize::<crate::memory::MemorySubgraphResponse>(),
        "MemorySubgraphMemberResponse" => {
            serialize::<crate::memory::MemorySubgraphMemberResponse>()
        }
        "CanonicalEntityResponse" => serialize::<crate::memory::CanonicalEntityResponse>(),
        "EntityResolutionResponse" => serialize::<crate::memory::EntityResolutionResponse>(),
        "ClaimConflictResponse" => serialize::<crate::memory::ClaimConflictResponse>(),
        "SummaryTraceResponse" => serialize::<crate::memory::SummaryTraceResponse>(),
        "EntityGraphAttachmentResponse" => {
            serialize::<crate::memory::EntityGraphAttachmentResponse>()
        }
        "SubgraphEdgeResponse" => serialize::<crate::memory::SubgraphEdgeResponse>(),
        "CompiledMemoryPolicyResponse" => {
            serialize::<crate::memory::CompiledMemoryPolicyResponse>()
        }
        "RelationPolicyResponse" => serialize::<crate::memory::RelationPolicyResponse>(),
        "ClaimPolicyResponse" => serialize::<crate::memory::ClaimPolicyResponse>(),
        _ => return None,
    })
}

fn schema_chunk_8(name: &str) -> Option<Value> {
    Some(match name {
        "RetrievalPolicyResponse" => serialize::<crate::memory::RetrievalPolicyResponse>(),
        "ListSourcesResponse" => serialize::<crate::memory::ListSourcesResponse>(),
        "ListEvidenceResponse" => serialize::<crate::memory::ListEvidenceResponse>(),
        "ListEntitiesResponse" => serialize::<crate::memory::ListEntitiesResponse>(),
        "ListClaimsResponse" => serialize::<crate::memory::ListClaimsResponse>(),
        "ListEventsResponse" => serialize::<crate::memory::ListEventsResponse>(),
        "ListRelationshipsResponse" => serialize::<crate::memory::ListRelationshipsResponse>(),
        "ListMemoryContractsResponse" => serialize::<crate::memory::ListMemoryContractsResponse>(),
        "ListMemorySubgraphsResponse" => serialize::<crate::memory::ListMemorySubgraphsResponse>(),
        "ListCanonicalEntitiesResponse" => {
            serialize::<crate::memory::ListCanonicalEntitiesResponse>()
        }
        "ListEntityResolutionsResponse" => {
            serialize::<crate::memory::ListEntityResolutionsResponse>()
        }
        "ListClaimConflictsResponse" => serialize::<crate::memory::ListClaimConflictsResponse>(),
        _ => return None,
    })
}

fn schema_chunk_9(name: &str) -> Option<Value> {
    Some(match name {
        "CreateIngestionConnectorRequest" => {
            serialize::<crate::ingestion::CreateIngestionConnectorRequest>()
        }
        "LocalTextConnectorConfigRequest" => {
            serialize::<crate::ingestion::LocalTextConnectorConfigRequest>()
        }
        "IngestionConnectorResponse" => serialize::<crate::ingestion::IngestionConnectorResponse>(),
        "LocalTextConnectorConfigResponse" => {
            serialize::<crate::ingestion::LocalTextConnectorConfigResponse>()
        }
        "ListIngestionConnectorsResponse" => {
            serialize::<crate::ingestion::ListIngestionConnectorsResponse>()
        }
        "IngestionRunResponse" => serialize::<crate::ingestion::IngestionRunResponse>(),
        "IngestionRunOutputsResponse" => {
            serialize::<crate::ingestion::IngestionRunOutputsResponse>()
        }
        "IngestionRunWithOutputsResponse" => {
            serialize::<crate::ingestion::IngestionRunWithOutputsResponse>()
        }
        "ListIngestionRunsResponse" => serialize::<crate::ingestion::ListIngestionRunsResponse>(),
        "ReviewClaimsQuery" => serialize::<crate::ingestion::ReviewClaimsQuery>(),
        "ReviewClaimResponse" => serialize::<crate::ingestion::ReviewClaimResponse>(),
        "ReviewEvidenceResponse" => serialize::<crate::ingestion::ReviewEvidenceResponse>(),
        "ReviewSourceResponse" => serialize::<crate::ingestion::ReviewSourceResponse>(),
        "ListReviewClaimsResponse" => serialize::<crate::ingestion::ListReviewClaimsResponse>(),
        _ => return None,
    })
}

fn schema_chunk_10(name: &str) -> Option<Value> {
    Some(match name {
        "AskRequest" => serialize::<crate::reasoning::AskRequest>(),
        _ => return None,
    })
}

pub(crate) fn schema(name: &str) -> Option<Value> {
    alias_schema(name)
        .or_else(|| schema_chunk_1(name))
        .or_else(|| schema_chunk_2(name))
        .or_else(|| schema_chunk_3(name))
        .or_else(|| schema_chunk_4(name))
        .or_else(|| schema_chunk_5(name))
        .or_else(|| schema_chunk_6(name))
        .or_else(|| schema_chunk_7(name))
        .or_else(|| schema_chunk_8(name))
        .or_else(|| schema_chunk_9(name))
        .or_else(|| schema_chunk_10(name))
}
