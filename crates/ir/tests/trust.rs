//! Trust strengthens only through an admitted record. These tests are the
//! statement of that rule.

use capsulet_ir::trust::{RawTrustClass, RawVerificationRecord, RecordVerdict, TrustError};
use capsulet_ir::{Digest, TrustClass, VerificationRecord};

fn certificate() -> Digest {
    Digest::of(b"a certificate")
}

fn raw(
    verdict: RecordVerdict,
    residual_count: u32,
    provenance_complete: bool,
) -> RawVerificationRecord {
    RawVerificationRecord {
        contract: "compiles-and-passes-named-tests".to_string(),
        certificate: certificate(),
        verdict,
        residual_count,
        provenance_complete,
    }
}

fn admitted(verdict: RecordVerdict, residual_count: u32, complete: bool) -> VerificationRecord {
    VerificationRecord::admit(raw(verdict, residual_count, complete)).expect("record is admitted")
}

#[test]
fn a_clean_accepted_record_justifies_verified() {
    let record = admitted(RecordVerdict::Accepted, 0, true);
    assert!(matches!(
        TrustClass::from_record(&record),
        TrustClass::Verified { .. }
    ));
}

#[test]
fn residuals_or_lost_provenance_cap_trust_at_conditional() {
    assert!(matches!(
        TrustClass::from_record(&admitted(RecordVerdict::Accepted, 1, true)),
        TrustClass::Conditional { .. }
    ));
    assert!(matches!(
        TrustClass::from_record(&admitted(RecordVerdict::Accepted, 0, false)),
        TrustClass::Conditional { .. }
    ));
    assert!(matches!(
        TrustClass::from_record(&admitted(RecordVerdict::Conditional, 0, true)),
        TrustClass::Conditional { .. }
    ));
}

#[test]
fn a_rejected_or_unevaluated_record_carries_no_trust() {
    assert_eq!(
        TrustClass::from_record(&admitted(RecordVerdict::Rejected, 0, true)),
        TrustClass::Unverified
    );
    assert_eq!(
        TrustClass::from_record(&admitted(RecordVerdict::Unverified, 0, true)),
        TrustClass::Unverified
    );
}

#[test]
fn a_record_without_a_contract_is_not_admitted() {
    let mut anonymous = raw(RecordVerdict::Accepted, 0, true);
    anonymous.contract = "   ".to_string();

    assert_eq!(
        VerificationRecord::admit(anonymous),
        Err(TrustError::MissingContract)
    );
}

#[test]
fn deserializing_cannot_assert_a_class_the_record_does_not_justify() {
    let overclaimed = RawTrustClass::Verified {
        record: raw(RecordVerdict::Conditional, 2, true),
    };

    assert_eq!(
        TrustClass::try_from(overclaimed),
        Err(TrustError::Unjustified {
            claimed: "verified",
            justified: "conditional",
        })
    );

    let rejected = RawTrustClass::Conditional {
        record: raw(RecordVerdict::Rejected, 0, true),
    };
    assert_eq!(
        TrustClass::try_from(rejected),
        Err(TrustError::VerdictTooWeak {
            found: "rejected".to_string()
        })
    );
}

#[test]
fn a_bare_verified_claim_does_not_deserialize() {
    // No record at all: the wire shape cannot express a strengthened class
    // without the evidence for it.
    let error = serde_json_stub::from_str_trust(r#"{"kind":"verified"}"#)
        .expect_err("a bare claim is refused");
    assert!(
        error.contains("record"),
        "the failure should name the missing record, found: {error}"
    );
}

#[test]
fn claiming_less_than_the_record_justifies_is_allowed() {
    let modest = RawTrustClass::Conditional {
        record: raw(RecordVerdict::Accepted, 0, true),
    };

    assert!(matches!(
        TrustClass::try_from(modest).expect("a conservative claim is honest"),
        TrustClass::Conditional { .. }
    ));
}

#[test]
fn combining_values_yields_the_weakest_relevant_trust() {
    let verified = TrustClass::from_record(&admitted(RecordVerdict::Accepted, 0, true));
    let conditional = TrustClass::from_record(&admitted(RecordVerdict::Conditional, 1, true));

    assert_eq!(verified.meet(&verified), verified);
    assert_eq!(verified.meet(&conditional), conditional);
    assert_eq!(
        verified.meet(&TrustClass::Unverified),
        TrustClass::Unverified
    );
    assert_eq!(
        TrustClass::meet_all(&[verified.clone(), conditional.clone()]),
        conditional
    );
    assert_eq!(
        TrustClass::meet_all(std::iter::empty::<&TrustClass>()),
        TrustClass::Unverified
    );
}

#[test]
fn combining_different_contracts_establishes_neither() {
    let compiled = TrustClass::from_record(&admitted(RecordVerdict::Accepted, 0, true));

    let other = VerificationRecord::admit(RawVerificationRecord {
        contract: "scanned-under-named-rules".to_string(),
        certificate: certificate(),
        verdict: RecordVerdict::Accepted,
        residual_count: 0,
        provenance_complete: true,
    })
    .expect("record is admitted");
    let scanned = TrustClass::from_record(&other);

    assert_eq!(compiled.meet(&scanned), TrustClass::Unverified);
}

#[test]
fn an_opaque_hop_drops_trust_and_records_why() {
    let verified = TrustClass::from_record(&admitted(RecordVerdict::Accepted, 0, true));
    let (after, loss) = verified.after_opaque_hop("agent state was carried as opaque JSON");

    assert_eq!(after, TrustClass::Unverified);
    assert_eq!(loss.lost_class, "verified");
    assert!(loss.reason.contains("opaque JSON"));
}

/// The crate deliberately has no JSON dependency, so this test reaches for a
/// deserializer only here, through the canonical reader plus serde.
mod serde_json_stub {
    use capsulet_ir::TrustClass;
    use serde::Deserialize;
    use serde::de::IntoDeserializer;
    use serde::de::value::{Error, MapDeserializer};

    /// Deserializes a trust class from the one shape this test needs: an object
    /// carrying only `kind`.
    pub(super) fn from_str_trust(document: &str) -> Result<TrustClass, String> {
        assert!(document.contains("\"kind\":\"verified\""));
        let entries = std::iter::once(("kind", "verified"));
        let deserializer: MapDeserializer<'_, _, Error> =
            MapDeserializer::new(entries.map(|(key, value)| (key, value.into_deserializer())));
        TrustClass::deserialize(deserializer).map_err(|error| error.to_string())
    }
}
