use thiserror::Error;

/// Error returned when a boundary value has no corresponding domain variant.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("unknown {kind} {value}")]
pub struct ParseDomainValueError {
    kind: &'static str,
    value: String,
}

impl ParseDomainValueError {
    pub(crate) fn new(kind: &'static str, value: &str) -> Self {
        Self {
            kind,
            value: value.to_owned(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{
        ArtifactObjectKind, AutomationStatus, JobRunStatus, TriggerKind, WorkflowRunStatus,
        WorkflowStatus,
    };

    #[test]
    fn persisted_domain_values_round_trip() {
        assert_eq!("artifact".parse(), Ok(ArtifactObjectKind::Artifact));
        assert_eq!("enabled".parse(), Ok(AutomationStatus::Enabled));
        assert_eq!("retry_scheduled".parse(), Ok(JobRunStatus::RetryScheduled));
        assert_eq!("custom".parse(), Ok(TriggerKind::Custom));
        assert_eq!("timed_out".parse(), Ok(WorkflowRunStatus::TimedOut));
        assert_eq!("draft".parse(), Ok(WorkflowStatus::Draft));
    }

    #[test]
    fn unknown_persisted_domain_value_is_rejected() {
        assert!("not-real".parse::<JobRunStatus>().is_err());
    }
}
