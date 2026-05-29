use std::collections::HashSet;
use std::fmt::{self, Display};

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
}
