//! Execution runner boundary and adapter implementations.

mod internal;

/// Shared execution contracts.
pub mod contract {
    pub use crate::internal::{
        CancellationCheck, CollectedArtifact, InputArtifact, NeverCancelled, NeverCancelledError,
        RunExecution, RunOutcome, RunReport, Runner,
    };
}

/// Static execution-pool configuration.
pub mod pools {
    pub use crate::internal::{
        ExecutionPoolConfig, ExecutionPoolsConfig, PoolResources, PoolToleration,
    };
}

/// Deterministic test and smoke-test runner.
pub mod stub {
    pub use crate::internal::{StubRunner, StubRunnerError};
}

/// Trusted local process runner.
pub mod process {
    pub use crate::internal::{ProcessRunner, ProcessRunnerError};
}

/// WASI Python runner.
pub mod wasm_python {
    pub use crate::internal::{WasmPythonConfig, WasmPythonRunner, WasmPythonRunnerError};
}

/// Kubernetes Job runner.
pub mod kubernetes {
    pub use crate::internal::{KubernetesRunner, KubernetesRunnerError, build_job};
}

pub use contract::{
    CancellationCheck, CollectedArtifact, InputArtifact, NeverCancelled, NeverCancelledError,
    RunExecution, RunOutcome, RunReport, Runner,
};
pub use kubernetes::{KubernetesRunner, KubernetesRunnerError, build_job};
pub use pools::{ExecutionPoolConfig, ExecutionPoolsConfig, PoolResources, PoolToleration};
pub use process::{ProcessRunner, ProcessRunnerError};
pub use stub::{StubRunner, StubRunnerError};
pub use wasm_python::{WasmPythonConfig, WasmPythonRunner, WasmPythonRunnerError};
