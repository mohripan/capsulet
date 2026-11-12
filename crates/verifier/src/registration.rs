//! What a validator is allowed to be, declared before it runs.
//!
//! A validator's basis comes from its registration, never from its own
//! response, and the registration is checked against what the validator is
//! allowed to reach: a tool granted the network or a model cannot be registered
//! as `replayable_tool`, because nothing re-runs a call to something that may
//! answer differently tomorrow (ADR 0021 §4).

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// What a validator may reach beyond its inputs.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Capability {
    Network,
    /// A learned model, wherever it runs.
    Model,
}

/// What kind of checker a validator is.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum RegisteredBasis {
    /// No network and no model, in an environment pinned by digest. Its
    /// `holds` discharges an obligation, and a replay can run it again.
    ReplayableTool,
    /// A learned model. Its `holds` endorses and its `does_not_hold` disputes;
    /// it never discharges anything.
    ModelJudgment {
        family: String,
        independent_of_proposer: bool,
        #[serde(default, skip_serializing_if = "Option::is_none")]
        calibration: Option<String>,
    },
}

/// Where a validator runs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum Environment {
    /// A container image pinned by digest, run isolated: no network unless
    /// granted, a read-only root and inputs, and resource limits.
    Container { image: String },
    /// A program on this machine, unisolated. For the development harness
    /// only: what it decides is recorded as decided in an unisolated run, and
    /// discharges nothing.
    Process {
        program: String,
        #[serde(default)]
        args: Vec<String>,
    },
}

impl Environment {
    /// Whether the run is isolated, which is what lets it decide anything.
    #[must_use]
    pub const fn is_isolated(&self) -> bool {
        matches!(self, Self::Container { .. })
    }
}

/// What a run may spend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limits {
    pub timeout_ms: u64,
    pub memory_mb: u64,
    /// The most the outputs directory may hold when the run ends.
    pub max_output_bytes: u64,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            timeout_ms: 60_000,
            memory_mb: 512,
            max_output_bytes: 16 * 1024 * 1024,
        }
    }
}

/// A validator, as a pack declares it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Registration {
    pub name: String,
    pub version: String,
    pub basis: RegisteredBasis,
    pub environment: Environment,
    #[serde(default)]
    pub capabilities: Vec<Capability>,
    #[serde(default)]
    pub limits: Limits,
    #[serde(default)]
    pub parameters: std::collections::BTreeMap<String, String>,
}

/// Why a registration cannot be run.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum RegistrationError {
    #[error("validator `{name}` is registered as a replayable tool but is granted {capability:?}")]
    ToolWithCapability {
        name: String,
        capability: Capability,
    },
    #[error("validator `{name}` is a model and is not granted the model capability")]
    ModelWithoutCapability { name: String },
    #[error("validator `{name}` names image `{image}`, which is not pinned by digest")]
    ImageNotPinned { name: String, image: String },
    #[error("validator `{name}` has no name, version or limit it can be held to: {detail}")]
    Incomplete { name: String, detail: &'static str },
}

impl Registration {
    /// Checks the registration is one the host may run.
    ///
    /// # Errors
    ///
    /// Returns [`RegistrationError`] when a tool is granted the network or a
    /// model, a model is not granted the model capability, an image is not
    /// pinned by digest, or a name, version or limit is missing.
    pub fn validate(&self) -> Result<(), RegistrationError> {
        let name = || self.name.clone();
        if self.name.trim().is_empty() || self.version.trim().is_empty() {
            return Err(RegistrationError::Incomplete {
                name: name(),
                detail: "a name and a version",
            });
        }
        if self.limits.timeout_ms == 0 || self.limits.memory_mb == 0 {
            return Err(RegistrationError::Incomplete {
                name: name(),
                detail: "a time and a memory limit",
            });
        }
        match &self.basis {
            RegisteredBasis::ReplayableTool => {
                if let Some(&capability) = self.capabilities.first() {
                    return Err(RegistrationError::ToolWithCapability {
                        name: name(),
                        capability,
                    });
                }
            }
            RegisteredBasis::ModelJudgment { .. } => {
                if !self.capabilities.contains(&Capability::Model) {
                    return Err(RegistrationError::ModelWithoutCapability { name: name() });
                }
            }
        }
        if let Environment::Container { image } = &self.environment
            && !pinned(image)
        {
            return Err(RegistrationError::ImageNotPinned {
                name: name(),
                image: image.clone(),
            });
        }
        Ok(())
    }
}

/// Whether an image reference ends in a SHA-256 digest.
fn pinned(image: &str) -> bool {
    image
        .rsplit_once("@sha256:")
        .is_some_and(|(repository, digest)| {
            !repository.is_empty()
                && digest.len() == 64
                && digest
                    .chars()
                    .all(|character| character.is_ascii_hexdigit())
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tool(capabilities: Vec<Capability>, image: &str) -> Registration {
        Registration {
            name: "compiler".to_string(),
            version: "1.99".to_string(),
            basis: RegisteredBasis::ReplayableTool,
            environment: Environment::Container {
                image: image.to_string(),
            },
            capabilities,
            limits: Limits::default(),
            parameters: std::collections::BTreeMap::new(),
        }
    }

    const PINNED: &str =
        "rust@sha256:0000000000000000000000000000000000000000000000000000000000000000";

    #[test]
    fn a_tool_granted_the_network_or_a_model_is_not_a_replayable_tool() {
        assert_eq!(tool(vec![], PINNED).validate(), Ok(()));
        for capability in [Capability::Network, Capability::Model] {
            assert!(matches!(
                tool(vec![capability], PINNED).validate(),
                Err(RegistrationError::ToolWithCapability { .. })
            ));
        }
    }

    #[test]
    fn an_image_has_to_be_pinned_by_digest() {
        for image in ["rust:1.99", "rust@sha256:abc", "@sha256:0000"] {
            assert!(
                matches!(
                    tool(vec![], image).validate(),
                    Err(RegistrationError::ImageNotPinned { .. })
                ),
                "{image}"
            );
        }
    }

    #[test]
    fn a_model_has_to_be_granted_the_model_it_is() {
        let mut model = tool(vec![], PINNED);
        model.basis = RegisteredBasis::ModelJudgment {
            family: "other".to_string(),
            independent_of_proposer: true,
            calibration: None,
        };
        assert!(matches!(
            model.validate(),
            Err(RegistrationError::ModelWithoutCapability { .. })
        ));
        model.capabilities = vec![Capability::Model];
        assert_eq!(model.validate(), Ok(()));
    }
}
