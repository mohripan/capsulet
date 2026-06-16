use std::collections::HashSet;
use std::fmt::{self, Display};

use super::AutomationId;

/// Trigger name scoped to a single automation.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct TriggerName(String);

impl TriggerName {
    /// Creates a trigger name scoped to an automation.
    ///
    /// # Errors
    ///
    /// Returns an error when the trigger name is empty or whitespace.
    pub fn new(value: impl Into<String>) -> Result<Self, String> {
        let value = value.into();
        if value.trim().is_empty() {
            return Err("trigger name cannot be empty".to_string());
        }
        Ok(Self(value))
    }

    #[must_use]
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl Display for TriggerName {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

/// Trigger implementation kind supported by the automation control plane.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TriggerKind {
    Manual,
    Schedule,
    Sql,
    Custom,
}

impl Display for TriggerKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::Manual => "manual",
            Self::Schedule => "schedule",
            Self::Sql => "sql",
            Self::Custom => "custom",
        })
    }
}

/// A trigger definition attached to one automation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AutomationTrigger {
    automation_id: AutomationId,
    name: TriggerName,
    kind: TriggerKind,
    config_json: String,
    plugin_id: Option<String>,
    enabled: bool,
}

impl AutomationTrigger {
    #[must_use]
    pub fn new(
        automation_id: AutomationId,
        name: TriggerName,
        kind: TriggerKind,
        config_json: impl Into<String>,
        plugin_id: Option<String>,
        enabled: bool,
    ) -> Self {
        Self {
            automation_id,
            name,
            kind,
            config_json: config_json.into(),
            plugin_id,
            enabled,
        }
    }

    #[must_use]
    pub const fn automation_id(&self) -> &AutomationId {
        &self.automation_id
    }

    #[must_use]
    pub const fn name(&self) -> &TriggerName {
        &self.name
    }

    #[must_use]
    pub const fn kind(&self) -> TriggerKind {
        self.kind
    }

    #[must_use]
    pub fn config_json(&self) -> &str {
        &self.config_json
    }

    #[must_use]
    pub fn plugin_id(&self) -> Option<&str> {
        self.plugin_id.as_deref()
    }

    #[must_use]
    pub const fn enabled(&self) -> bool {
        self.enabled
    }
}

/// Registry entry for a custom trigger plugin image.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomTriggerPlugin {
    id: String,
    name: String,
    description: String,
    runtime_image: String,
    command: Vec<String>,
    config_schema_json: String,
}

impl CustomTriggerPlugin {
    #[must_use]
    pub fn new(
        id: impl Into<String>,
        name: impl Into<String>,
        description: impl Into<String>,
        runtime_image: impl Into<String>,
        command: Vec<String>,
        config_schema_json: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            name: name.into(),
            description: description.into(),
            runtime_image: runtime_image.into(),
            command,
            config_schema_json: config_schema_json.into(),
        }
    }

    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn description(&self) -> &str {
        &self.description
    }

    #[must_use]
    pub fn runtime_image(&self) -> &str {
        &self.runtime_image
    }

    #[must_use]
    pub fn command(&self) -> &[String] {
        &self.command
    }

    #[must_use]
    pub fn config_schema_json(&self) -> &str {
        &self.config_schema_json
    }
}

/// Structured boolean condition tree for automation trigger evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConditionExpr {
    Trigger(TriggerName),
    All(Vec<ConditionExpr>),
    Any(Vec<ConditionExpr>),
}

impl ConditionExpr {
    #[must_use]
    pub fn evaluate(&self, satisfied_triggers: &HashSet<TriggerName>) -> bool {
        match self {
            Self::Trigger(trigger) => satisfied_triggers.contains(trigger),
            Self::All(expressions) => expressions
                .iter()
                .all(|expression| expression.evaluate(satisfied_triggers)),
            Self::Any(expressions) => expressions
                .iter()
                .any(|expression| expression.evaluate(satisfied_triggers)),
        }
    }

    /// Validates that every condition group contains at least one expression.
    ///
    /// # Errors
    ///
    /// Returns an error when an `All` or `Any` group is empty.
    pub fn validate(&self) -> Result<(), String> {
        match self {
            Self::Trigger(_) => Ok(()),
            Self::All(expressions) | Self::Any(expressions) => {
                if expressions.is_empty() {
                    return Err("condition groups cannot be empty".to_string());
                }

                expressions.iter().try_for_each(Self::validate)
            }
        }
    }

    /// Validates that condition leaves reference existing trigger definitions.
    ///
    /// # Errors
    ///
    /// Returns an error when the condition is structurally invalid or refers to
    /// a trigger name that is not part of the automation.
    pub fn validate_references(&self, trigger_names: &HashSet<TriggerName>) -> Result<(), String> {
        self.validate()?;
        match self {
            Self::Trigger(trigger) => {
                if trigger_names.contains(trigger) {
                    Ok(())
                } else {
                    Err(format!("condition references unknown trigger: {trigger}"))
                }
            }
            Self::All(expressions) | Self::Any(expressions) => expressions
                .iter()
                .try_for_each(|expression| expression.validate_references(trigger_names)),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::{ConditionExpr, TriggerName};

    #[test]
    fn evaluates_grouped_trigger_expression() {
        let data_ready = TriggerName::new("data_ready").expect("valid trigger");
        let approved = TriggerName::new("approved").expect("valid trigger");
        let manual = TriggerName::new("manual_override").expect("valid trigger");
        let expression = ConditionExpr::Any(vec![
            ConditionExpr::All(vec![
                ConditionExpr::Trigger(data_ready.clone()),
                ConditionExpr::Trigger(approved.clone()),
            ]),
            ConditionExpr::Trigger(manual.clone()),
        ]);

        let mut satisfied = HashSet::new();
        satisfied.insert(data_ready);
        assert!(!expression.evaluate(&satisfied));

        satisfied.insert(approved);
        assert!(expression.evaluate(&satisfied));

        let satisfied = HashSet::from([manual]);
        assert!(expression.evaluate(&satisfied));
    }

    #[test]
    fn rejects_empty_condition_groups() {
        let expression = ConditionExpr::All(Vec::new());

        assert!(expression.validate().is_err());
    }

    #[test]
    fn rejects_condition_references_to_unknown_triggers() {
        let expression =
            ConditionExpr::Trigger(TriggerName::new("data_ready").expect("valid trigger"));
        let known_triggers = HashSet::from([TriggerName::new("approved").expect("valid trigger")]);

        assert!(expression.validate_references(&known_triggers).is_err());
    }
}
