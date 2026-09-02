//! What the canonical encoding promises, stated as tests.

use std::collections::BTreeMap;

use capsulet_ir::canonical::{CanonicalError, CanonicalValue, Decimal};
use capsulet_ir::version::{IR_NAMESPACE, SchemaVersion, SchemaVersionError};
use capsulet_ir::{Digest, digest_of, from_json_slice, read_compatible, verify_canonical};
use serde::Serialize;

#[test]
fn presentation_does_not_change_a_digest() {
    let first = from_json_slice(br#"{"b":1,"a":[1,2]}"#).expect("first document parses");
    let second =
        from_json_slice(b"{\n  \"a\" : [1, 2],\n  \"b\":\t1\n}").expect("second document parses");

    assert_eq!(first, second);
    assert_eq!(
        Digest::of(&first.to_canonical_bytes()),
        Digest::of(&second.to_canonical_bytes())
    );
}

#[test]
fn list_order_changes_a_digest() {
    let first = from_json_slice(b"[1,2]").expect("first list parses");
    let second = from_json_slice(b"[2,1]").expect("second list parses");

    assert_ne!(
        Digest::of(&first.to_canonical_bytes()),
        Digest::of(&second.to_canonical_bytes())
    );
}

#[test]
fn floating_point_is_refused_before_any_digest() {
    assert_eq!(from_json_slice(b"1.5"), Err(CanonicalError::Float));
    assert_eq!(from_json_slice(b"1e3"), Err(CanonicalError::Float));
    assert_eq!(
        capsulet_ir::to_canonical_bytes(&0.1_f64),
        Err(CanonicalError::Float)
    );
    assert_eq!(
        capsulet_ir::to_canonical_bytes(&f64::NAN),
        Err(CanonicalError::Float)
    );
    assert_eq!(
        capsulet_ir::to_canonical_bytes(&f64::INFINITY),
        Err(CanonicalError::Float)
    );
}

#[test]
fn duplicate_keys_are_refused_rather_than_collapsed() {
    assert_eq!(
        from_json_slice(br#"{"a":1,"a":2}"#),
        Err(CanonicalError::DuplicateKey {
            key: "a".to_string()
        })
    );
}

#[test]
fn non_utf8_input_is_refused() {
    assert_eq!(
        from_json_slice(&[0x22, 0xff, 0x22]),
        Err(CanonicalError::NotUtf8)
    );
}

#[test]
fn unnormalized_text_is_refused() {
    // "é" written as `e` plus a combining acute accent is not NFC.
    let decomposed = "\"e\u{301}\"";
    assert!(matches!(
        from_json_slice(decomposed.as_bytes()),
        Err(CanonicalError::NotNormalized { .. })
    ));
    assert!(matches!(
        capsulet_ir::to_canonical_bytes(&"e\u{301}"),
        Err(CanonicalError::NotNormalized { .. })
    ));

    let composed = "\"\u{e9}\"";
    assert!(from_json_slice(composed.as_bytes()).is_ok());
}

#[test]
fn raw_bytes_and_oversized_integers_are_refused() {
    assert_eq!(
        capsulet_ir::to_canonical_bytes(&serde_bytes_stub::Bytes),
        Err(CanonicalError::RawBytes)
    );
    assert_eq!(
        capsulet_ir::to_canonical_bytes(&u128::MAX),
        Err(CanonicalError::IntegerRange {
            found: u128::MAX.to_string()
        })
    );
}

mod serde_bytes_stub {
    use serde::{Serialize, Serializer};

    pub(super) struct Bytes;

    impl Serialize for Bytes {
        fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
            serializer.serialize_bytes(&[1, 2, 3])
        }
    }
}

#[test]
fn structs_encode_with_sorted_keys_and_no_whitespace() {
    #[derive(Serialize)]
    struct Node {
        name: String,
        budget_ms: u64,
        attempts: u8,
    }

    let bytes = capsulet_ir::to_canonical_bytes(&Node {
        name: "compile".to_string(),
        budget_ms: 30_000,
        attempts: 2,
    })
    .expect("node encodes");

    assert_eq!(
        String::from_utf8(bytes).expect("canonical bytes are UTF-8"),
        r#"{"attempts":2,"budget_ms":30000,"name":"compile"}"#
    );
}

#[test]
fn decimals_are_fixed_point_text_not_numbers() {
    let value = Decimal::parse("1.250").expect("decimal parses");
    let bytes = capsulet_ir::to_canonical_bytes(&value).expect("decimal encodes");

    assert_eq!(String::from_utf8(bytes).expect("UTF-8"), "\"1.250\"");
    assert_ne!(
        Decimal::parse("1.25").expect("decimal parses"),
        Decimal::parse("1.250").expect("decimal parses")
    );
    assert!(Decimal::parse("1.").is_err());
    assert!(Decimal::parse("01").is_err());
    assert!(Decimal::parse("1e3").is_err());
}

#[test]
fn canonical_form_is_verified_not_assumed() {
    let canonical = br#"{"a":1,"b":2}"#;
    assert!(verify_canonical(canonical).is_ok());

    assert_eq!(
        verify_canonical(br#"{"b":2,"a":1}"#),
        Err(CanonicalError::NotCanonical)
    );
    assert_eq!(
        verify_canonical(b"{\"a\": 1,\"b\":2}"),
        Err(CanonicalError::NotCanonical)
    );
}

#[test]
fn digests_render_and_parse_in_exactly_one_form() {
    let digest = Digest::of(b"capsulet");
    let rendered = digest.to_string();

    assert!(rendered.starts_with("sha256:"));
    assert_eq!(rendered.len(), "sha256:".len() + 64);
    assert_eq!(rendered.parse::<Digest>().expect("digest parses"), digest);

    assert!("deadbeef".parse::<Digest>().is_err());
    assert!(rendered.to_uppercase().parse::<Digest>().is_err());
    assert!("sha256:zz".parse::<Digest>().is_err());
}

#[test]
fn digest_of_a_value_matches_the_digest_of_its_canonical_bytes() {
    #[derive(Serialize)]
    struct Record {
        verdict: &'static str,
    }

    let record = Record {
        verdict: "conditional",
    };
    let bytes = capsulet_ir::to_canonical_bytes(&record).expect("record encodes");

    assert_eq!(bytes, br#"{"verdict":"conditional"}"#);
    assert_eq!(
        digest_of(&record).expect("record digests"),
        Digest::of(&bytes)
    );

    // The model reached by parsing agrees with the model reached by serializing.
    let mut entries = BTreeMap::new();
    entries.insert(
        "verdict".to_string(),
        CanonicalValue::Text("conditional".to_string()),
    );
    assert_eq!(
        from_json_slice(&bytes).expect("canonical bytes parse"),
        CanonicalValue::Map(entries)
    );
}

#[test]
fn unknown_schema_majors_fail_closed() {
    let expected = SchemaVersion::new(IR_NAMESPACE, 1);

    assert_eq!(
        read_compatible("capsulet.ir/v1", &expected).expect("known major reads"),
        expected
    );
    assert_eq!(
        read_compatible("capsulet.ir/v2", &expected),
        Err(SchemaVersionError::UnsupportedMajor {
            found: 2,
            supported: 1
        })
    );
    assert!(matches!(
        read_compatible("capsulet.certificate/v1", &expected),
        Err(SchemaVersionError::Namespace { .. })
    ));
    assert!(matches!(
        read_compatible("capsulet.ir", &expected),
        Err(SchemaVersionError::Malformed { .. })
    ));
}

#[test]
fn deep_nesting_is_refused_rather_than_overflowing() {
    let deep = format!("{}1{}", "[".repeat(1_000), "]".repeat(1_000));
    assert!(matches!(
        from_json_slice(deep.as_bytes()),
        Err(CanonicalError::Syntax { .. })
    ));
}
