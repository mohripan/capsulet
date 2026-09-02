//! Structural compatibility, including the cases a nominal tag would have let
//! through.

use std::collections::BTreeMap;

use capsulet_ir::value::{SchemaMismatch, aliases};
use capsulet_ir::{Field, IntegerRange, LengthBounds, ValueSchema};

fn text() -> ValueSchema {
    ValueSchema::Text {
        length: LengthBounds::new(0, 1_024),
    }
}

fn record(fields: Vec<(&str, Field)>) -> ValueSchema {
    let mut map = BTreeMap::new();
    for (name, field) in fields {
        map.insert(name.to_string(), field);
    }
    ValueSchema::Record { fields: map }
}

#[test]
fn a_wider_producer_satisfies_a_narrower_requirement() {
    let produced = record(vec![
        ("id", Field::required(text())),
        ("body", Field::required(text())),
        ("debug_note", Field::required(text())),
    ]);
    let required = record(vec![("id", Field::required(text()))]);

    assert_eq!(produced.check_satisfies(&required), Ok(()));
}

#[test]
fn an_optional_producer_field_does_not_satisfy_a_required_one() {
    let produced = record(vec![("id", Field::optional(text()))]);
    let required = record(vec![("id", Field::required(text()))]);

    assert_eq!(
        produced.check_satisfies(&required),
        Err(SchemaMismatch::MissingField {
            path: "<root>".to_string(),
            field: "id".to_string(),
        })
    );
}

#[test]
fn a_mismatch_names_the_field_and_the_rule() {
    let produced = record(vec![(
        "budget",
        Field::required(ValueSchema::Integer {
            range: IntegerRange::new(0, 1_000_000),
        }),
    )]);
    let required = record(vec![(
        "budget",
        Field::required(ValueSchema::Integer {
            range: IntegerRange::new(0, 100),
        }),
    )]);

    let error = produced
        .check_satisfies(&required)
        .expect_err("the wider range does not fit");
    assert!(matches!(error, SchemaMismatch::OutOfRange { ref path, .. } if path == "budget"));
    assert!(error.to_string().contains("budget"));
}

#[test]
fn a_producer_variant_the_requirement_cannot_handle_is_refused() {
    let mut produced_variants = BTreeMap::new();
    produced_variants.insert("patch".to_string(), text());
    produced_variants.insert("escalation".to_string(), text());
    let produced = ValueSchema::Union {
        discriminant: "kind".to_string(),
        variants: produced_variants,
    };

    let mut required_variants = BTreeMap::new();
    required_variants.insert("patch".to_string(), text());
    let required = ValueSchema::Union {
        discriminant: "kind".to_string(),
        variants: required_variants,
    };

    assert_eq!(
        produced.check_satisfies(&required),
        Err(SchemaMismatch::UnexpectedVariant {
            path: "<root>".to_string(),
            variant: "escalation".to_string(),
        })
    );
}

#[test]
fn a_different_discriminant_is_not_the_same_union() {
    let mut variants = BTreeMap::new();
    variants.insert("patch".to_string(), text());

    let produced = ValueSchema::Union {
        discriminant: "kind".to_string(),
        variants: variants.clone(),
    };
    let required = ValueSchema::Union {
        discriminant: "type".to_string(),
        variants,
    };

    assert!(matches!(
        produced.check_satisfies(&required),
        Err(SchemaMismatch::MissingDiscriminant { .. })
    ));
}

#[test]
fn opacity_never_satisfies_structure_but_structure_satisfies_opacity() {
    let opaque = aliases::opaque("an unmodelled port");
    let structured = record(vec![("id", Field::required(text()))]);

    assert_eq!(
        opaque.check_satisfies(&structured),
        Err(SchemaMismatch::OpaqueNotAccepted {
            path: "<root>".to_string()
        })
    );
    assert_eq!(structured.check_satisfies(&opaque), Ok(()));
    assert_eq!(opaque.check_satisfies(&opaque), Ok(()));
}

#[test]
fn opacity_anywhere_inside_a_value_is_visible_from_outside() {
    let nested = record(vec![
        ("id", Field::required(text())),
        (
            "payload",
            Field::required(ValueSchema::List {
                item: Box::new(aliases::opaque("connector passthrough")),
                length: LengthBounds::new(0, 8),
            }),
        ),
    ]);

    assert!(nested.carries_opacity());
    assert!(!record(vec![("id", Field::required(text()))]).carries_opacity());
}

#[test]
fn decimal_scale_is_part_of_the_type() {
    let cents = ValueSchema::Decimal { scale: 2 };
    let millis = ValueSchema::Decimal { scale: 3 };

    assert_eq!(cents.check_satisfies(&cents), Ok(()));
    assert!(matches!(
        cents.check_satisfies(&millis),
        Err(SchemaMismatch::Scale { .. })
    ));
}

#[test]
fn every_current_port_tag_maps_to_a_structural_schema() {
    // The tags the running graph model uses today, from
    // `capsulet_core::domain::graph::PortValueType`.
    let tags = [
        "user_query",
        "conversation_context",
        "normalized_query",
        "embedding_vector",
        "retrieved_documents",
        "ranked_documents",
        "prompt",
        "model_response",
        "validation_result",
        "final_answer",
        "json",
    ];

    for tag in tags {
        let schema = aliases::for_port_value_type(tag)
            .unwrap_or_else(|| panic!("port tag `{tag}` has no structural schema"));
        // Only the deliberately opaque tag is allowed to lose structure.
        assert_eq!(
            schema.carries_opacity(),
            tag == "json",
            "port tag `{tag}` has the wrong opacity"
        );
    }

    assert!(
        aliases::for_port_value_type("a_tag_added_later").is_none(),
        "an unknown tag must fail loudly rather than defaulting to opacity"
    );
}

#[test]
fn the_published_mapping_lists_every_tag_the_code_knows() {
    let published = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../docs/contracts/ir-value-types.md"),
    )
    .expect("the published mapping is readable");

    for tag in [
        "user_query",
        "conversation_context",
        "normalized_query",
        "embedding_vector",
        "retrieved_documents",
        "ranked_documents",
        "prompt",
        "model_response",
        "validation_result",
        "final_answer",
        "json",
    ] {
        assert!(
            aliases::for_port_value_type(tag).is_some(),
            "`{tag}` is published but the code no longer maps it"
        );
        assert!(
            published.contains(&format!("| `{tag}` |")),
            "`{tag}` is mapped in code but missing from the published table"
        );
    }
}
