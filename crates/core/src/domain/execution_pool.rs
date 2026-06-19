use std::fmt::{self, Display};
use std::time::Duration;

/// Named compute target used by Capsulet before Kubernetes chooses a node.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExecutionPoolName(String);

impl ExecutionPoolName {
    /// Creates an execution pool name.
    ///
    /// # Errors
    ///
    /// Returns an error when the pool name is empty or whitespace.
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err("execution pool name cannot be empty".to_string());
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for ExecutionPoolName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Runtime resource defaults for jobs routed through an execution pool.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResourceRequirements {
    cpu_millis: u32,
    memory_mib: u32,
}

impl ResourceRequirements {
    #[must_use]
    pub const fn new(cpu_millis: u32, memory_mib: u32) -> Self {
        Self {
            cpu_millis,
            memory_mib,
        }
    }

    #[must_use]
    pub const fn cpu_millis(&self) -> u32 {
        self.cpu_millis
    }

    #[must_use]
    pub const fn memory_mib(&self) -> u32 {
        self.memory_mib
    }
}

/// Execution pool configuration as understood by the domain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionPool {
    name: ExecutionPoolName,
    timeout: Duration,
    max_concurrent_jobs: u32,
    resources: ResourceRequirements,
}

impl ExecutionPool {
    /// Creates an execution pool with validated operational defaults.
    ///
    /// # Errors
    ///
    /// Returns an error when timeout or max concurrency are zero.
    pub fn new(
        name: ExecutionPoolName,
        timeout: Duration,
        max_concurrent_jobs: u32,
        resources: ResourceRequirements,
    ) -> Result<Self, String> {
        if timeout.is_zero() {
            return Err("execution pool timeout must be greater than zero".to_string());
        }

        if max_concurrent_jobs == 0 {
            return Err("execution pool concurrency must be greater than zero".to_string());
        }

        Ok(Self {
            name,
            timeout,
            max_concurrent_jobs,
            resources,
        })
    }

    #[must_use]
    pub const fn name(&self) -> &ExecutionPoolName {
        &self.name
    }

    #[must_use]
    pub const fn timeout(&self) -> Duration {
        self.timeout
    }

    #[must_use]
    pub const fn max_concurrent_jobs(&self) -> u32 {
        self.max_concurrent_jobs
    }

    #[must_use]
    pub const fn resources(&self) -> &ResourceRequirements {
        &self.resources
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::{ExecutionPool, ExecutionPoolName, ResourceRequirements};

    #[test]
    fn execution_pool_requires_positive_concurrency() {
        let pool = ExecutionPool::new(
            ExecutionPoolName::new("mini").expect("valid pool name"),
            Duration::from_mins(2),
            0,
            ResourceRequirements::new(100, 128),
        );

        assert!(pool.is_err());
    }

    #[test]
    fn execution_pool_accepts_valid_defaults() {
        let pool = ExecutionPool::new(
            ExecutionPoolName::new("large").expect("valid pool name"),
            Duration::from_hours(1),
            10,
            ResourceRequirements::new(2000, 4096),
        )
        .expect("valid pool");

        assert_eq!(pool.name().as_str(), "large");
        assert_eq!(pool.max_concurrent_jobs(), 10);
    }
}
