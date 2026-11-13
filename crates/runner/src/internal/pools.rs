//! Execution pools: the policy, images, dependencies and resources a job runs under.

use std::collections::BTreeMap;

use k8s_openapi::api::core::v1::{ResourceRequirements, Toleration};
use serde::Deserialize;

use super::contract::RunExecution;
use super::kubernetes::map_quantities;

pub(super) const DEFAULT_JOB_TIMEOUT_SECONDS: u64 = 300;

/// Static execution pool configuration used to render Kubernetes Jobs.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPoolConfig {
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub node_selector: BTreeMap<String, String>,
    #[serde(default)]
    pub tolerations: Vec<PoolToleration>,
    #[serde(default)]
    pub resources: PoolResources,
    #[serde(default = "default_timeout_seconds")]
    pub timeout_seconds: u64,
    #[serde(default)]
    pub max_concurrent_jobs: u32,
    #[serde(default)]
    pub ttl_seconds_after_finished: Option<i32>,
    #[serde(default)]
    pub runtime_class_name: Option<String>,
    #[serde(default)]
    pub service_account_name: Option<String>,
    #[serde(default)]
    pub policy: ExecutionPolicy,
    #[serde(default)]
    pub allowed_images: Vec<String>,
    #[serde(default)]
    pub require_digest_images: bool,
    #[serde(default)]
    pub max_python_dependencies: Option<usize>,
    #[serde(default)]
    pub max_python_dependency_length: Option<usize>,
    #[serde(default)]
    pub blocked_python_dependencies: Vec<String>,
}

impl Default for ExecutionPoolConfig {
    fn default() -> Self {
        Self {
            description: String::new(),
            node_selector: BTreeMap::new(),
            tolerations: Vec::new(),
            resources: PoolResources::default(),
            timeout_seconds: default_timeout_seconds(),
            max_concurrent_jobs: 0,
            ttl_seconds_after_finished: None,
            runtime_class_name: None,
            service_account_name: None,
            policy: ExecutionPolicy::default(),
            allowed_images: Vec::new(),
            require_digest_images: false,
            max_python_dependencies: None,
            max_python_dependency_length: None,
            blocked_python_dependencies: Vec::new(),
        }
    }
}

impl ExecutionPoolConfig {
    #[must_use]
    pub fn execution_policy(&self) -> ExecutionPolicy {
        let mut policy = self.policy.clone();
        if !self.allowed_images.is_empty() {
            policy.images.allowed.clone_from(&self.allowed_images);
        }
        if self.require_digest_images {
            policy.images.require_digest = true;
        }
        if self.max_python_dependencies.is_some() {
            policy.python.max_dependencies = self.max_python_dependencies;
        }
        if self.max_python_dependency_length.is_some() {
            policy.python.max_dependency_length = self.max_python_dependency_length;
        }
        if !self.blocked_python_dependencies.is_empty() {
            policy
                .python
                .blocked_dependencies
                .clone_from(&self.blocked_python_dependencies);
        }
        policy
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPolicy {
    #[serde(default)]
    pub images: ImagePolicy,
    #[serde(default)]
    pub python: PythonDependencyPolicy,
}

impl ExecutionPolicy {
    /// Validates an execution request against image and Python dependency policy.
    ///
    /// # Errors
    ///
    /// Returns an error when the runtime image is not allowed, a digest is
    /// required but missing, or the Python dependency list violates the pool
    /// policy.
    pub fn validate(
        &self,
        runtime_image: &str,
        python_dependencies: &[String],
    ) -> Result<(), String> {
        self.images.validate(runtime_image)?;
        self.python.validate(python_dependencies)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImagePolicy {
    #[serde(default)]
    pub allowed: Vec<String>,
    #[serde(default)]
    pub require_digest: bool,
}

impl ImagePolicy {
    fn validate(&self, image: &str) -> Result<(), String> {
        if self.require_digest && !image.contains("@sha256:") {
            return Err(format!("runtime image {image} must be pinned by digest"));
        }
        if !self.allowed.is_empty()
            && !self
                .allowed
                .iter()
                .any(|allowed| image_matches_policy(image, allowed))
        {
            return Err(format!("runtime image {image} is not allowed in this pool"));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PythonDependencyPolicy {
    #[serde(default)]
    pub max_dependencies: Option<usize>,
    #[serde(default)]
    pub max_dependency_length: Option<usize>,
    #[serde(default)]
    pub blocked_dependencies: Vec<String>,
}

impl PythonDependencyPolicy {
    fn validate(&self, dependencies: &[String]) -> Result<(), String> {
        if let Some(max) = self.max_dependencies
            && dependencies.len() > max
        {
            return Err(format!(
                "python dependency count {} exceeds pool limit {max}",
                dependencies.len()
            ));
        }
        if let Some(max) = self.max_dependency_length
            && let Some(dependency) = dependencies
                .iter()
                .find(|dependency| dependency.len() > max)
        {
            return Err(format!(
                "python dependency {dependency} exceeds pool length limit {max}"
            ));
        }
        for dependency in dependencies {
            if self
                .blocked_dependencies
                .iter()
                .any(|blocked| dependency_name_matches(dependency, blocked))
            {
                return Err(format!("python dependency {dependency} is blocked"));
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct PoolResources {
    #[serde(default)]
    pub requests: BTreeMap<String, String>,
    #[serde(default)]
    pub limits: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Deserialize)]
pub struct PoolToleration {
    pub key: Option<String>,
    pub operator: Option<String>,
    pub value: Option<String>,
    pub effect: Option<String>,
}

/// Execution pools document shape stored in Helm values/config.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionPoolsConfig {
    pub default_pool: String,
    pub pools: BTreeMap<String, ExecutionPoolConfig>,
}

impl ExecutionPoolsConfig {
    /// Parses execution pool YAML.
    ///
    /// # Errors
    ///
    /// Returns an error when the YAML cannot be decoded.
    pub fn from_yaml(yaml: &str) -> Result<Self, serde_yaml::Error> {
        serde_yaml::from_str(yaml)
    }

    #[must_use]
    pub fn find(&self, name: &str) -> Option<&ExecutionPoolConfig> {
        self.pools.get(name)
    }
}

pub(super) fn validate_execution_policy(execution: &RunExecution) -> Result<(), String> {
    execution.pool.execution_policy().validate(
        execution.definition.runtime_image(),
        execution.definition.python_dependencies(),
    )
}

pub(super) fn image_matches_policy(image: &str, allowed: &str) -> bool {
    allowed
        .strip_suffix('*')
        .map_or_else(|| image == allowed, |prefix| image.starts_with(prefix))
}

pub(super) fn dependency_name_matches(dependency: &str, blocked: &str) -> bool {
    let blocked = blocked.trim().to_ascii_lowercase();
    if blocked.is_empty() {
        return false;
    }
    let name = dependency
        .trim()
        .split(['=', '<', '>', '!', '~', '[', ';', ' '])
        .next()
        .unwrap_or_default()
        .to_ascii_lowercase();
    name == blocked
}

impl PoolResources {
    pub(super) fn to_kubernetes(&self) -> ResourceRequirements {
        ResourceRequirements {
            requests: map_quantities(&self.requests),
            limits: map_quantities(&self.limits),
            ..ResourceRequirements::default()
        }
    }
}

impl PoolToleration {
    pub(super) fn to_kubernetes(&self) -> Toleration {
        Toleration {
            key: self.key.clone(),
            operator: self.operator.clone(),
            value: self.value.clone(),
            effect: self.effect.clone(),
            ..Toleration::default()
        }
    }
}

pub(super) fn default_timeout_seconds() -> u64 {
    DEFAULT_JOB_TIMEOUT_SECONDS
}
