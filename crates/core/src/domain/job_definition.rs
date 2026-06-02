use super::JobDefinitionId;

/// Minimal executable job definition for Sprint 002.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobDefinition {
    pub id: JobDefinitionId,
    pub name: String,
    pub runtime_image: String,
    pub command: Vec<String>,
    pub bundle_object_key: String,
    pub input_schema: String,
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

        Ok(Self {
            id,
            name,
            runtime_image,
            command,
            bundle_object_key,
            input_schema,
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
            command: vec!["python".to_string(), "/workspace/main.py".to_string()],
            bundle_object_key: "bundles/job_hello_python.tar.gz".to_string(),
            input_schema: "{}".to_string(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{JobDefinition, JobDefinitionId};

    #[test]
    fn validates_required_execution_fields() {
        let definition = JobDefinition::new(
            JobDefinitionId::new("job_1").expect("valid id"),
            "Example",
            "python:3.12-slim",
            vec!["python".to_string(), "/workspace/main.py".to_string()],
            "bundles/job_1.tar.gz",
            "{}",
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
        );

        assert!(definition.is_err());
    }
}
