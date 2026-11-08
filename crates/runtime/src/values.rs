//! Whether a value is what its port says it is.
//!
//! Ports have always been typed, and until now only the wiring was checked: nothing
//! looked at the values moving through them, so a model that answered with the wrong
//! shape was believed. This is the check, run on what a node actually produced.
//!
//! Checking is structural and total. It never coerces, because a value quietly
//! converted into what a port wanted is a value nobody produced. Floating point is
//! accepted only where a port declares it does not model the structure at all.

use std::collections::BTreeMap;

use capsulet_ir::digest::Digest;
use capsulet_ir::value::{Field, ValueSchema};
use serde_json::Value;
use thiserror::Error;

/// How a value failed to be what its port declared.
///
/// Said in two parts, because the two go to different readers. `path` and `problem`
/// come from the schema and never contain the value, so they can be recorded in a
/// run's log. `found` is the offending piece of the value itself, for whoever sent
/// it, and is left out of anything recorded: a log quoting what it rejected would be
/// holding content an erasure cannot reach.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
#[error("at `{path}`: {problem}{}", found.as_ref().map(|found| format!(" (found `{found}`)")).unwrap_or_default())]
pub struct ValueViolation {
    /// Where in the value, from the root, in terms of the schema.
    pub path: String,
    pub problem: String,
    /// The offending piece of the value, when quoting it helps the sender.
    pub found: Option<String>,
}

impl ValueViolation {
    /// The violation without any part of the value, for a log or a trace.
    #[must_use]
    pub fn without_content(&self) -> String {
        format!("at `{}`: {}", self.path, self.problem)
    }
}

/// Checks a value against a schema.
///
/// # Errors
///
/// Returns the first [`ValueViolation`], with where it was found.
pub fn check_value(schema: &ValueSchema, value: &Value) -> Result<(), ValueViolation> {
    check_at(schema, value, "$")
}

fn violation(path: &str, problem: String) -> ValueViolation {
    ValueViolation {
        path: path.to_string(),
        problem,
        found: None,
    }
}

fn quoting(path: &str, problem: String, found: String) -> ValueViolation {
    ValueViolation {
        path: path.to_string(),
        problem,
        found: Some(found),
    }
}

fn expected(path: &str, what: &str, value: &Value) -> ValueViolation {
    violation(path, format!("expected {what}, found {}", shape_of(value)))
}

fn check_at(schema: &ValueSchema, value: &Value, path: &str) -> Result<(), ValueViolation> {
    match schema {
        ValueSchema::List { item, length } => {
            let items = value
                .as_array()
                .ok_or_else(|| expected(path, "a list", value))?;
            let count = u32::try_from(items.len()).unwrap_or(u32::MAX);
            if count < length.minimum || count > length.maximum {
                return Err(violation(
                    path,
                    format!(
                        "a list of {count} items is outside {}..={}",
                        length.minimum, length.maximum
                    ),
                ));
            }
            for (index, each) in items.iter().enumerate() {
                check_at(item, each, &format!("{path}[{index}]"))?;
            }
            Ok(())
        }
        ValueSchema::Record { fields } => check_record(fields, value, path),
        ValueSchema::Union {
            discriminant,
            variants,
        } => check_union(discriminant, variants, value, path),
        // Declared unmodelled, so there is nothing to hold it to.
        ValueSchema::Json { .. } => Ok(()),
        scalar => check_scalar(scalar, value, path),
    }
}

fn check_scalar(schema: &ValueSchema, value: &Value, path: &str) -> Result<(), ValueViolation> {
    match schema {
        ValueSchema::Unit if value.is_null() => Ok(()),
        ValueSchema::Unit => Err(expected(path, "nothing", value)),
        ValueSchema::Bool if value.is_boolean() => Ok(()),
        ValueSchema::Bool => Err(expected(path, "a boolean", value)),
        ValueSchema::Integer { range } => {
            let reading = value
                .as_i64()
                .map(i128::from)
                .or_else(|| value.as_u64().map(i128::from))
                .ok_or_else(|| expected(path, "an integer", value))?;
            if reading < range.minimum || reading > range.maximum {
                return Err(quoting(
                    path,
                    format!(
                        "the integer is outside {}..={}",
                        range.minimum, range.maximum
                    ),
                    reading.to_string(),
                ));
            }
            Ok(())
        }
        ValueSchema::Decimal { scale } => {
            let text = value
                .as_str()
                .ok_or_else(|| expected(path, "a decimal string", value))?;
            if is_decimal_at_scale(text, *scale) {
                Ok(())
            } else {
                Err(quoting(
                    path,
                    format!("not a decimal with exactly {scale} fractional digits"),
                    text.to_string(),
                ))
            }
        }
        ValueSchema::Text { length } => {
            let text = value
                .as_str()
                .ok_or_else(|| expected(path, "text", value))?;
            let count = u32::try_from(text.chars().count()).unwrap_or(u32::MAX);
            if count < length.minimum || count > length.maximum {
                return Err(violation(
                    path,
                    format!(
                        "text of {count} characters is outside {}..={}",
                        length.minimum, length.maximum
                    ),
                ));
            }
            Ok(())
        }
        ValueSchema::BytesRef => {
            let text = value
                .as_str()
                .ok_or_else(|| expected(path, "a content digest", value))?;
            text.parse::<Digest>()
                .map(|_| ())
                .map_err(|_| quoting(path, "not a content digest".to_string(), text.to_string()))
        }
        ValueSchema::Enumeration { members } => {
            let text = value
                .as_str()
                .ok_or_else(|| expected(path, "a named member", value))?;
            if members.iter().any(|member| member == text) {
                Ok(())
            } else {
                Err(quoting(
                    path,
                    "not a declared member".to_string(),
                    text.to_string(),
                ))
            }
        }
        ValueSchema::ArtifactRef { .. } => {
            if value.as_str().is_some_and(|text| !text.is_empty()) {
                Ok(())
            } else {
                Err(expected(path, "an artifact reference", value))
            }
        }
        // The composite kinds are answered by `check_at` before it gets here.
        other => Err(violation(
            path,
            format!("{} is not a scalar", other.kind_name()),
        )),
    }
}

fn check_union(
    discriminant: &str,
    variants: &BTreeMap<String, ValueSchema>,
    value: &Value,
    path: &str,
) -> Result<(), ValueViolation> {
    let object = value
        .as_object()
        .ok_or_else(|| expected(path, "a tagged union", value))?;
    let tag = object
        .get(discriminant)
        .and_then(Value::as_str)
        .ok_or_else(|| {
            violation(
                path,
                format!("no `{discriminant}` says which variant this is"),
            )
        })?;
    let schema = variants.get(tag).ok_or_else(|| {
        quoting(
            path,
            "names a variant that is not declared".to_string(),
            tag.to_string(),
        )
    })?;
    let mut body = object.clone();
    body.remove(discriminant);
    // A record variant is checked against the rest of the object; anything else is
    // carried under `value`.
    match schema {
        ValueSchema::Record { .. } => {
            check_at(schema, &Value::Object(body), &format!("{path}.{tag}"))
        }
        _ => check_at(
            schema,
            body.get("value").unwrap_or(&Value::Null),
            &format!("{path}.{tag}"),
        ),
    }
}

fn check_record(
    fields: &BTreeMap<String, Field>,
    value: &Value,
    path: &str,
) -> Result<(), ValueViolation> {
    let Some(object) = value.as_object() else {
        return Err(expected(path, "a record", value));
    };
    // The unknown name is the value's, not the schema's, so it is quoted rather than
    // made part of the path.
    if let Some(name) = object.keys().find(|name| !fields.contains_key(*name)) {
        return Err(quoting(
            path,
            "has a field this record does not declare".to_string(),
            name.clone(),
        ));
    }
    for (name, field) in fields {
        match object.get(name) {
            Some(Value::Null) | None if !field.required => {}
            None => {
                return Err(violation(
                    &format!("{path}.{name}"),
                    "this required field is missing".to_string(),
                ));
            }
            Some(each) => check_at(&field.schema, each, &format!("{path}.{name}"))?,
        }
    }
    Ok(())
}

fn is_decimal_at_scale(text: &str, scale: u8) -> bool {
    let unsigned = text.strip_prefix('-').unwrap_or(text);
    let (whole, fraction) = unsigned.split_once('.').unwrap_or((unsigned, ""));
    !whole.is_empty()
        && whole.bytes().all(|byte| byte.is_ascii_digit())
        && fraction.len() == usize::from(scale)
        && fraction.bytes().all(|byte| byte.is_ascii_digit())
}

const fn shape_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "nothing",
        Value::Bool(_) => "a boolean",
        Value::Number(_) => "a number",
        Value::String(_) => "text",
        Value::Array(_) => "a list",
        Value::Object(_) => "a record",
    }
}

#[cfg(test)]
mod tests {
    use capsulet_ir::value::{IntegerRange, LengthBounds, aliases};
    use serde_json::json;

    use super::*;

    #[test]
    fn prose_is_text_within_its_bounds() {
        assert!(check_value(&aliases::prose(), &json!("an answer")).is_ok());
        assert!(check_value(&aliases::prose(), &json!(42)).is_err());
        assert!(check_value(&aliases::prose(), &Value::Null).is_err());

        let short = ValueSchema::Text {
            length: LengthBounds::new(1, 3),
        };
        assert!(check_value(&short, &json!("")).is_err());
        assert!(check_value(&short, &json!("four")).is_err());
    }

    #[test]
    fn a_validation_result_needs_its_verdict() {
        let schema = aliases::for_port_value_type("validation_result").expect("a known tag");
        assert!(check_value(&schema, &json!({ "passed": true })).is_ok());
        assert!(check_value(&schema, &json!({ "passed": false, "detail": "why" })).is_ok());
        let missing = check_value(&schema, &json!({ "detail": "why" })).expect_err("no verdict");
        assert_eq!(missing.path, "$.passed");
    }

    #[test]
    fn a_record_does_not_take_fields_it_never_declared() {
        let schema = aliases::for_port_value_type("validation_result").expect("a known tag");
        let extra = check_value(&schema, &json!({ "passed": true, "score": 0.9 }))
            .expect_err("an undeclared field");
        assert_eq!(
            extra.path, "$",
            "the name is the value's, so it is not the path"
        );
        assert_eq!(extra.found.as_deref(), Some("score"));
    }

    #[test]
    fn what_a_log_records_of_a_violation_never_quotes_the_value() {
        let member = ValueSchema::Enumeration {
            members: vec!["low".to_string(), "high".to_string()],
        };
        let secret = "hunter2-not-a-member";
        let violation = check_value(&member, &json!(secret)).expect_err("not a member");
        assert!(
            violation.to_string().contains(secret),
            "the sender is told what it sent: {violation}"
        );
        assert!(
            !violation.without_content().contains(secret),
            "{}",
            violation.without_content()
        );

        let bad = json!([{ "source_id": "claim-1", "content_digest": secret }]);
        let violation = check_value(&aliases::documents(), &bad).expect_err("not a digest");
        assert!(!violation.without_content().contains(secret));
        let extra = check_value(
            &aliases::for_port_value_type("validation_result").expect("a known tag"),
            &json!({ "passed": true, secret: 1 }),
        )
        .expect_err("an undeclared field");
        assert!(!extra.without_content().contains(secret));
    }

    #[test]
    fn a_document_cites_real_digests() {
        let digest = Digest::of(b"the content").to_string();
        let good = json!([{ "source_id": "claim-1", "content_digest": digest }]);
        assert!(check_value(&aliases::documents(), &good).is_ok());

        let bad = json!([{ "source_id": "claim-1", "content_digest": "not-a-digest" }]);
        let violation = check_value(&aliases::documents(), &bad).expect_err("not a digest");
        assert_eq!(violation.path, "$[0].content_digest");
    }

    #[test]
    fn an_integer_stays_in_range_and_is_never_a_float() {
        let schema = ValueSchema::Integer {
            range: IntegerRange::new(0, 10),
        };
        assert!(check_value(&schema, &json!(10)).is_ok());
        assert!(check_value(&schema, &json!(11)).is_err());
        assert!(check_value(&schema, &json!(1.5)).is_err());
    }

    #[test]
    fn a_decimal_is_exact_text_at_its_scale() {
        let schema = ValueSchema::Decimal { scale: 2 };
        assert!(check_value(&schema, &json!("-12.50")).is_ok());
        assert!(check_value(&schema, &json!("12.5")).is_err());
        assert!(check_value(&schema, &json!(12.5)).is_err());
    }

    #[test]
    fn opaque_json_holds_nothing_to_anything() {
        let schema = aliases::opaque("the test does not model this");
        assert!(check_value(&schema, &json!({ "anything": [1.5, null] })).is_ok());
    }
}
