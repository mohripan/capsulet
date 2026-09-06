//! Arithmetic a certificate can stand behind.
//!
//! These tests are mostly about what the old float implementation got wrong:
//! the answer depended on the order of the operands, the tolerance meant nothing
//! at large magnitudes, and a value that could not be serialized collapsed the
//! replay digest to the empty string.

use capsulet_kernel::quantity::{MAX_SCALE, Quantity, QuantityError};
use capsulet_kernel::{ArithOp, Proposal, Proposition, Rule, Snapshot, Verdict, check};

fn q(text: &str) -> Quantity {
    Quantity::parse(text).expect("a fixed-point decimal")
}

fn arith(op: ArithOp, operands: &[&str], claimed: &str) -> Verdict {
    let proposition = Proposition::new("total", "is", claimed);
    check(
        &Proposal {
            goal: proposition.clone(),
            derivation: Rule::Arith {
                op,
                operands: operands.iter().map(|text| q(text)).collect(),
                claimed: q(claimed),
                proposition,
            },
        },
        &Snapshot::new(),
    )
    .verdict
}

#[test]
fn a_sum_does_not_depend_on_the_order_of_its_operands() {
    // The float version could not promise this: addition is not associative in
    // binary floating point, so the same operands in a different order could
    // land either side of the tolerance.
    let forwards = q("0.1")
        .checked_add(q("0.2"))
        .and_then(|acc| acc.checked_add(q("0.3")))
        .expect("exact");
    let backwards = q("0.3")
        .checked_add(q("0.2"))
        .and_then(|acc| acc.checked_add(q("0.1")))
        .expect("exact");

    assert_eq!(forwards, backwards);
    assert_eq!(forwards, q("0.6"));
}

#[test]
fn the_classic_float_sum_is_exact_here() {
    // 0.1 + 0.2 is famously not 0.3 in binary floating point.
    assert_eq!(
        arith(ArithOp::Sum, &["0.1", "0.2"], "0.3"),
        Verdict::Accepted
    );
    assert_eq!(
        arith(ArithOp::Sum, &["0.1", "0.2"], "0.30000000001"),
        Verdict::Rejected
    );
}

#[test]
fn neighbouring_values_are_distinguished_at_any_magnitude() {
    // The old tolerance was absolute: `1e-9` meant something near zero and
    // nothing at all out here, where the gap between adjacent `f64` values is
    // already wider than the tolerance and every neighbour compared equal.
    let big = "1000000000000000000";
    let next = "1000000000000000001";

    assert_ne!(q(big), q(next));
    assert_eq!(
        arith(ArithOp::Sum, &[big, "1"], next),
        Verdict::Accepted,
        "the exact answer is accepted"
    );
    assert_eq!(
        arith(ArithOp::Sum, &[big, "1"], big),
        Verdict::Rejected,
        "and its neighbour is not"
    );
}

#[test]
fn trailing_zeros_are_kept_but_do_not_change_the_value() {
    // A scale is a statement about precision, so it round-trips. It is not a
    // statement about which number this is.
    assert_eq!(q("1.50").to_string(), "1.50");
    assert_eq!(q("1.5").to_string(), "1.5");
    assert_eq!(q("1.50"), q("1.5"));
    assert_eq!(
        arith(ArithOp::Sum, &["1.50", "0.50"], "2"),
        Verdict::Accepted
    );
}

#[test]
fn a_product_keeps_every_digit() {
    assert_eq!(
        arith(ArithOp::Product, &["1.5", "1.5"], "2.25"),
        Verdict::Accepted
    );
    assert_eq!(
        arith(ArithOp::Product, &["0.001", "0.001"], "0.000001"),
        Verdict::Accepted
    );
}

#[test]
fn difference_min_and_max_are_exact() {
    assert_eq!(
        arith(ArithOp::Difference, &["10", "0.1", "0.2"], "9.7"),
        Verdict::Accepted
    );
    assert_eq!(
        arith(ArithOp::Min, &["3", "1.5", "2"], "1.5"),
        Verdict::Accepted
    );
    assert_eq!(
        arith(ArithOp::Max, &["3", "1.5", "2"], "3"),
        Verdict::Accepted
    );
}

#[test]
fn a_result_that_would_not_be_exact_is_refused_rather_than_rounded() {
    // Scales add under multiplication. Past the point where the answer would
    // stop being exact, there is no honest result to record.
    let deep = Quantity::parse(&format!("0.{}1", "0".repeat(usize::from(MAX_SCALE) - 1)))
        .expect("at the maximum scale");
    assert_eq!(deep.scale(), MAX_SCALE);
    assert_eq!(
        deep.checked_mul(deep),
        None,
        "doubling the scale is past exactness, so there is no answer to give"
    );
}

#[test]
fn a_number_beyond_the_scale_bound_is_not_read_at_all() {
    let too_deep = format!("0.{}", "1".repeat(usize::from(MAX_SCALE) + 1));
    assert!(matches!(
        Quantity::parse(&too_deep),
        Err(QuantityError::ScaleTooLarge { .. })
    ));
}

#[test]
fn text_that_is_not_a_fixed_point_decimal_is_refused() {
    for text in ["", "1.2.3", "NaN", "inf", "1e9", "01", "1.", ".5", "- 1"] {
        assert!(
            matches!(Quantity::parse(text), Err(QuantityError::Malformed { .. })),
            "{text:?} should not read as a fixed-point decimal"
        );
    }
}

#[test]
fn a_quantity_round_trips_through_json_as_a_string() {
    // Never as a JSON number: a number invites a float back in somewhere between
    // here and the digest.
    let encoded = serde_json::to_string(&q("1.250")).expect("encodes");
    assert_eq!(encoded, "\"1.250\"");
    assert_eq!(
        serde_json::from_str::<Quantity>(&encoded).expect("decodes"),
        q("1.250")
    );
}
