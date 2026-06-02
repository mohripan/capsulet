use super::JobDefinitionId;

/// Minimal fixed-delay retry policy for a job definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RetryPolicy {
    pub max_attempts: u32,
    pub delay_seconds: u64,
}

impl RetryPolicy {
    #[must_use]
    pub const fn no_retry() -> Self {
        Self {
            max_attempts: 1,
            delay_seconds: 0,
        }
    }
}

/// Minimal executable job definition for Sprint 002.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobDefinition {
    pub id: JobDefinitionId,
    pub name: String,
    pub runtime_image: String,
    pub command: Vec<String>,
    pub bundle_object_key: String,
    pub input_schema: String,
    pub retry_max_attempts: u32,
    pub retry_delay_seconds: u64,
}

impl JobDefinition {
    /// Creates a validated job definition.
    ///
    /// # Errors
    ///
    /// Returns an error when required execution fields are empty.
    pub fn new(
        id: JobDefinitionId,
        name: impl Into<String>,
        runtime_image: impl Into<String>,
        command: Vec<String>,
        bundle_object_key: impl Into<String>,
        input_schema: impl Into<String>,
        retry_policy: RetryPolicy,
    ) -> Result<Self, String> {
        let name = name.into();
        let runtime_image = runtime_image.into();
        let bundle_object_key = bundle_object_key.into();
        let input_schema = input_schema.into();

        if name.trim().is_empty() {
            return Err("job definition name cannot be empty".to_string());
        }
        if runtime_image.trim().is_empty() {
            return Err("job definition runtime image cannot be empty".to_string());
        }
        if command.is_empty() || command.iter().any(|part| part.trim().is_empty()) {
            return Err("job definition command cannot be empty".to_string());
        }
        if bundle_object_key.trim().is_empty() {
            return Err("job definition bundle object key cannot be empty".to_string());
        }
        if input_schema.trim().is_empty() {
            return Err("job definition input schema cannot be empty".to_string());
        }
        if retry_policy.max_attempts == 0 {
            return Err("job definition retry max attempts must be greater than zero".to_string());
        }

        Ok(Self {
            id,
            name,
            runtime_image,
            command,
            bundle_object_key,
            input_schema,
            retry_max_attempts: retry_policy.max_attempts,
            retry_delay_seconds: retry_policy.delay_seconds,
        })
    }

    /// Returns the built-in hello Python definition used for local testing.
    ///
    /// # Panics
    ///
    /// Panics only if the static built-in identifier is invalid.
    #[must_use]
    pub fn hello_python() -> Self {
        Self {
            id: JobDefinitionId::new("job_hello_python").expect("static job definition id"),
            name: "Hello Python".to_string(),
            runtime_image: "python:3.12-slim".to_string(),
            command: vec![
                "python".to_string(),
                "-c".to_string(),
                "print('hello from capsulet')".to_string(),
            ],
            bundle_object_key: "bundles/job_hello_python.tar.gz".to_string(),
            input_schema: "{}".to_string(),
            retry_max_attempts: RetryPolicy::no_retry().max_attempts,
            retry_delay_seconds: RetryPolicy::no_retry().delay_seconds,
        }
    }

    /// Returns a built-in long-running Python definition used for cancellation.
    ///
    /// # Panics
    ///
    /// Panics only if the static built-in identifier is invalid.
    #[must_use]
    pub fn sleep_python() -> Self {
        Self {
            id: JobDefinitionId::new("job_sleep_python").expect("static job definition id"),
            name: "Sleep Python".to_string(),
            runtime_image: "python:3.12-slim".to_string(),
            command: vec![
                "python".to_string(),
                "-c".to_string(),
                "import time; print('sleeping from capsulet'); time.sleep(300)".to_string(),
            ],
            bundle_object_key: "bundles/job_sleep_python.tar.gz".to_string(),
            input_schema: "{}".to_string(),
            retry_max_attempts: RetryPolicy::no_retry().max_attempts,
            retry_delay_seconds: RetryPolicy::no_retry().delay_seconds,
        }
    }

    /// Returns a built-in failing Python definition used for retry testing.
    ///
    /// # Panics
    ///
    /// Panics only if the static built-in identifier is invalid.
    #[must_use]
    pub fn fail_python() -> Self {
        Self {
            id: JobDefinitionId::new("job_fail_python").expect("static job definition id"),
            name: "Fail Python".to_string(),
            runtime_image: "python:3.12-slim".to_string(),
            command: vec![
                "python".to_string(),
                "-c".to_string(),
                "import sys; print('failing from capsulet'); sys.exit(1)".to_string(),
            ],
            bundle_object_key: "bundles/job_fail_python.tar.gz".to_string(),
            input_schema: "{}".to_string(),
            retry_max_attempts: 2,
            retry_delay_seconds: 1,
        }
    }

    /// Returns a built-in slow Python definition used for timeout testing.
    ///
    /// # Panics
    ///
    /// Panics only if the static built-in identifier is invalid.
    #[must_use]
    pub fn timeout_python() -> Self {
        Self {
            id: JobDefinitionId::new("job_timeout_python").expect("static job definition id"),
            name: "Timeout Python".to_string(),
            runtime_image: "python:3.12-slim".to_string(),
            command: vec![
                "python".to_string(),
                "-c".to_string(),
                "import time; print('timing out from capsulet'); time.sleep(300)".to_string(),
            ],
            bundle_object_key: "bundles/job_timeout_python.tar.gz".to_string(),
            input_schema: "{}".to_string(),
            retry_max_attempts: RetryPolicy::no_retry().max_attempts,
            retry_delay_seconds: RetryPolicy::no_retry().delay_seconds,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{JobDefinition, JobDefinitionId, RetryPolicy};

    #[test]
    fn validates_required_execution_fields() {
        let definition = JobDefinition::new(
            JobDefinitionId::new("job_1").expect("valid id"),
            "Example",
            "python:3.12-slim",
            vec!["python".to_string(), "/workspace/main.py".to_string()],
            "bundles/job_1.tar.gz",
            "{}",
            RetryPolicy::no_retry(),
        )
        .expect("valid definition");

        assert_eq!(definition.runtime_image, "python:3.12-slim");
    }

    #[test]
    fn rejects_empty_command() {
        let definition = JobDefinition::new(
            JobDefinitionId::new("job_1").expect("valid id"),
            "Example",
            "python:3.12-slim",
            Vec::new(),
            "bundles/job_1.tar.gz",
            "{}",
            RetryPolicy::no_retry(),
        );

        assert!(definition.is_err());
    }

    #[test]
    fn rejects_zero_retry_attempts() {
        let definition = JobDefinition::new(
            JobDefinitionId::new("job_1").expect("valid id"),
            "Example",
            "python:3.12-slim",
            vec!["python".to_string(), "/workspace/main.py".to_string()],
            "bundles/job_1.tar.gz",
            "{}",
            RetryPolicy {
                max_attempts: 0,
                delay_seconds: 0,
            },
        );

        assert!(definition.is_err());
    }
}
