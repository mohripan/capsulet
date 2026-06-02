use super::JobRunId;

/// Bounded log output captured for one job run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobRunLog {
    pub run_id: JobRunId,
    pub text: String,
}

impl JobRunLog {
    /// Creates a job run log after checking required fields.
    ///
    /// # Errors
    ///
    /// Returns an error when the log text is empty.
    pub fn new(run_id: JobRunId, text: impl Into<String>) -> Result<Self, String> {
        let text = text.into();
        if text.is_empty() {
            return Err("job run log text cannot be empty".to_string());
        }

        Ok(Self { run_id, text })
    }
}

#[cfg(test)]
mod tests {
    use super::{JobRunId, JobRunLog};

    #[test]
    fn rejects_empty_logs() {
        let log = JobRunLog::new(JobRunId::new("run_1").expect("valid run id"), "");

        assert!(log.is_err());
    }

    #[test]
    fn accepts_non_empty_logs() {
        let log =
            JobRunLog::new(JobRunId::new("run_1").expect("valid run id"), "hello").expect("log");

        assert_eq!(log.text, "hello");
    }
}
