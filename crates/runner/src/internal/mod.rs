//! The runners and what they share.
//!
//! `contract` is what every runner is held to; `pools` is the static configuration a
//! job runs under; each runner has a module of its own; `artifacts` is how a job
//! hands files back. The crate's public modules re-export from here.

mod artifacts;
mod contract;
mod kubernetes;
mod pools;
mod process;
mod stub;
mod wasm_python;

pub use self::contract::{
    CancellationCheck, CollectedArtifact, InputArtifact, NeverCancelled, NeverCancelledError,
    RunExecution, RunOutcome, RunReport, Runner,
};
pub use self::kubernetes::{KubernetesRunner, KubernetesRunnerError, build_job};
pub use self::pools::{
    ExecutionPolicy, ExecutionPoolConfig, ExecutionPoolsConfig, ImagePolicy, PoolResources,
    PoolToleration, PythonDependencyPolicy,
};
pub use self::process::{ProcessRunner, ProcessRunnerError};
pub use self::stub::{StubRunner, StubRunnerError};
pub use self::wasm_python::{WasmPythonConfig, WasmPythonRunner, WasmPythonRunnerError};

// What the tests reach that the crate does not publish.
#[cfg(test)]
use self::{
    artifacts::split_artifact_markers,
    kubernetes::{RUN_LABEL, kubernetes_job_name, run_label_value, truncate_utf8},
    wasm_python::{validate_wasm_python_execution, wasmtime_command_spec},
};

#[cfg(test)]
#[path = "../tests.rs"]
mod tests;
