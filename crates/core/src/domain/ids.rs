use std::fmt::{self, Display};

macro_rules! id_type {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
        pub struct $name(String);

        impl $name {
            /// Creates an identifier after checking that it is not empty.
            ///
            /// The first real persistence layer can replace this with UUIDs or
            /// ULIDs. For now, string identifiers keep tests and YAML examples
            /// simple while still avoiding raw stringly-typed domain APIs.
            ///
            /// # Errors
            ///
            /// Returns an error when the identifier is empty or whitespace.
            pub fn new(value: impl Into<String>) -> Result<Self, String> {
                let value = value.into();
                if value.trim().is_empty() {
                    return Err(format!("{} cannot be empty", stringify!($name)));
                }
                Ok(Self(value))
            }

            #[must_use]
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl Display for $name {
            fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str(&self.0)
            }
        }
    };
}

id_type!(AutomationId);
id_type!(WorkflowId);
id_type!(WorkflowStepId);
id_type!(WorkflowRunId);
id_type!(WorkflowStepRunId);
id_type!(GraphId);
id_type!(AgentId);
id_type!(AgentRunId);
id_type!(TraceEventId);
id_type!(SourceId);
id_type!(EvidenceId);
id_type!(EntityId);
id_type!(ClaimId);
id_type!(EventId);
id_type!(RelationshipId);
id_type!(ObservationId);
id_type!(MemoryContractId);
id_type!(MemorySubgraphId);
id_type!(MemorySubgraphMemberId);
id_type!(MemoryMemberId);
id_type!(CanonicalEntityId);
id_type!(EntityResolutionId);
id_type!(SubgraphEdgeId);
id_type!(SummaryTraceId);
id_type!(EntityGraphAttachmentId);
id_type!(IngestionConnectorId);
id_type!(IngestionRunId);
id_type!(NodeId);
id_type!(PortId);
id_type!(HyperedgeId);
id_type!(ActionId);
id_type!(JobDefinitionId);
id_type!(JobRunId);
id_type!(JobAttemptId);
id_type!(ArtifactId);

#[cfg(test)]
mod tests {
    use super::JobRunId;

    #[test]
    fn rejects_empty_ids() {
        assert!(JobRunId::new(" ").is_err());
    }

    #[test]
    fn accepts_non_empty_ids() {
        let id = JobRunId::new("run_123").expect("valid id");

        assert_eq!(id.as_str(), "run_123");
    }
}
