use std::str::FromStr;

use super::{ArtifactId, JobAttemptId, JobRunId, ParseDomainValueError};

/// Run-scoped object kind stored in object storage.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ArtifactObjectKind {
    Bundle,
    Log,
    Artifact,
}

impl ArtifactObjectKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Bundle => "bundle",
            Self::Log => "log",
            Self::Artifact => "artifact",
        }
    }

    #[must_use]
    pub const fn prefix(self) -> &'static str {
        match self {
            Self::Bundle => "bundles",
            Self::Log => "logs",
            Self::Artifact => "artifacts",
        }
    }
}

impl FromStr for ArtifactObjectKind {
    type Err = ParseDomainValueError;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        match value {
            "bundle" => Ok(Self::Bundle),
            "log" => Ok(Self::Log),
            "artifact" => Ok(Self::Artifact),
            value => Err(ParseDomainValueError::new("artifact object kind", value)),
        }
    }
}

/// Metadata for one object-backed artifact or log object.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct JobArtifact {
    id: ArtifactId,
    run_id: JobRunId,
    attempt_id: Option<JobAttemptId>,
    name: String,
    object_key: String,
    content_type: String,
    size_bytes: u64,
    checksum_sha256: Option<String>,
    kind: ArtifactObjectKind,
}

impl JobArtifact {
    /// Creates validated artifact metadata.
    ///
    /// # Errors
    ///
    /// Returns an error when a required metadata field is empty or invalid.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: ArtifactId,
        run_id: JobRunId,
        attempt_id: Option<JobAttemptId>,
        name: impl Into<String>,
        object_key: impl Into<String>,
        content_type: impl Into<String>,
        size_bytes: u64,
        checksum_sha256: Option<String>,
        kind: ArtifactObjectKind,
    ) -> Result<Self, String> {
        let name = name.into();
        let object_key = object_key.into();
        let content_type = content_type.into();
        if name.trim().is_empty() {
            return Err("artifact name cannot be empty".to_string());
        }
        if name.contains('/') || name.contains('\\') || name == "." || name == ".." {
            return Err("artifact name must be a single relative file name".to_string());
        }
        if object_key.trim().is_empty() {
            return Err("artifact object key cannot be empty".to_string());
        }
        if content_type.trim().is_empty() {
            return Err("artifact content type cannot be empty".to_string());
        }

        Ok(Self {
            id,
            run_id,
            attempt_id,
            name,
            object_key,
            content_type,
            size_bytes,
            checksum_sha256,
            kind,
        })
    }

    #[must_use]
    pub const fn id(&self) -> &ArtifactId {
        &self.id
    }

    #[must_use]
    pub const fn run_id(&self) -> &JobRunId {
        &self.run_id
    }

    #[must_use]
    pub const fn attempt_id(&self) -> Option<&JobAttemptId> {
        self.attempt_id.as_ref()
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn object_key(&self) -> &str {
        &self.object_key
    }

    #[must_use]
    pub fn content_type(&self) -> &str {
        &self.content_type
    }

    #[must_use]
    pub const fn size_bytes(&self) -> u64 {
        self.size_bytes
    }

    #[must_use]
    pub fn checksum_sha256(&self) -> Option<&str> {
        self.checksum_sha256.as_deref()
    }

    #[must_use]
    pub const fn kind(&self) -> ArtifactObjectKind {
        self.kind
    }
}

#[cfg(test)]
mod tests {
    use super::{ArtifactId, ArtifactObjectKind, JobArtifact, JobRunId};

    #[test]
    fn accepts_artifact_metadata() {
        let artifact = JobArtifact::new(
            ArtifactId::new("artifact_1").expect("artifact id"),
            JobRunId::new("run_1").expect("run id"),
            None,
            "report.txt",
            "artifacts/run_1/report.txt",
            "text/plain",
            5,
            None,
            ArtifactObjectKind::Artifact,
        )
        .expect("artifact");

        assert_eq!(artifact.name(), "report.txt");
    }

    #[test]
    fn rejects_path_artifact_names() {
        let artifact = JobArtifact::new(
            ArtifactId::new("artifact_1").expect("artifact id"),
            JobRunId::new("run_1").expect("run id"),
            None,
            "../report.txt",
            "artifacts/run_1/report.txt",
            "text/plain",
            5,
            None,
            ArtifactObjectKind::Artifact,
        );

        assert!(artifact.is_err());
    }
}
