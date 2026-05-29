/// Runtime component role.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ComponentKind {
    Api,
    Worker,
    Scheduler,
    Evaluator,
    Runner,
    Cli,
}

impl ComponentKind {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Api => "capsulet-api",
            Self::Worker => "capsulet-worker",
            Self::Scheduler => "capsulet-scheduler",
            Self::Evaluator => "capsulet-evaluator",
            Self::Runner => "capsulet-runner",
            Self::Cli => "capsulet-cli",
        }
    }
}

/// Minimal descriptor every binary can expose before real startup exists.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentDescriptor {
    pub kind: ComponentKind,
    pub purpose: &'static str,
}

impl ComponentDescriptor {
    #[must_use]
    pub const fn new(kind: ComponentKind, purpose: &'static str) -> Self {
        Self { kind, purpose }
    }

    #[must_use]
    pub fn banner(&self) -> String {
        format!("{}: {}", self.kind.as_str(), self.purpose)
    }
}

#[cfg(test)]
mod tests {
    use super::{ComponentDescriptor, ComponentKind};

    #[test]
    fn component_banner_contains_name_and_purpose() {
        let descriptor = ComponentDescriptor::new(ComponentKind::Api, "control plane api");

        assert_eq!(descriptor.banner(), "capsulet-api: control plane api");
    }
}
