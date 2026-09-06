//! `check` decides. Every time, for every shape.
//!
//! The crate's first paragraph says every check is total. That claim was false
//! once — a nested derivation ended the process rather than returning — and the
//! fix was a bound. A bound is only worth what the surrounding code does with
//! it, so this walks a wide spread of derivations and asserts the only thing
//! that matters: `check` returned.
//!
//! Not random. A generated corpus that differs between runs turns a failure
//! into a story about a seed, and the point of this suite is that anyone can
//! re-run it and see the same thing.

use capsulet_kernel::quantity::Quantity;
use capsulet_kernel::{
    ArithOp, MAX_DERIVATION_DEPTH, Proposal, Proposition, Rule, Snapshot, check,
};

/// Every leaf a derivation can bottom out in, including ones that dangle.
fn leaves() -> Vec<Rule> {
    let ops = [
        ArithOp::Sum,
        ArithOp::Difference,
        ArithOp::Product,
        ArithOp::Min,
        ArithOp::Max,
    ];
    let mut leaves = vec![
        Rule::Attest {
            claim_id: String::new(),
        },
        Rule::Attest {
            claim_id: "absent".to_string(),
        },
        Rule::Cite {
            evidence_id: String::new(),
            proposition: Proposition::new("s", "p", "o"),
        },
        Rule::Cite {
            evidence_id: "absent".to_string(),
            proposition: Proposition::new("", "", ""),
        },
    ];
    for op in ops {
        // An empty operand list, a single operand, and a claim that disagrees.
        leaves.push(Rule::Arith {
            op,
            operands: vec![],
            claimed: Quantity::integer(0),
            proposition: Proposition::new("total", "is", "0"),
        });
        leaves.push(Rule::Arith {
            op,
            operands: vec![Quantity::integer(1), Quantity::integer(2)],
            claimed: Quantity::integer(99),
            proposition: Proposition::new("total", "is", "99"),
        });
    }
    leaves
}

/// Each way one rule can wrap another.
fn wrap(inner: Rule, which: usize) -> Rule {
    match which % 4 {
        0 => Rule::Trust {
            premise: Box::new(inner),
            min_authority: "high".to_string(),
        },
        1 => Rule::Trust {
            premise: Box::new(inner),
            // Not an authority level, so the rule has to refuse it.
            min_authority: "sideways".to_string(),
        },
        2 => Rule::Interpret {
            premise: Box::new(inner),
            proposition: Proposition::new("s", "p", "o"),
            rationale: "a reading".to_string(),
        },
        _ => Rule::Interpret {
            premise: Box::new(inner),
            proposition: Proposition::new("s", "p", "o"),
            // No rationale, which is its own refusal.
            rationale: String::new(),
        },
    }
}

#[test]
fn every_derivation_shape_reaches_a_verdict() {
    let snapshot = Snapshot::new();
    let mut decided = 0_usize;

    for leaf in leaves() {
        for wrapper in 0..4 {
            for depth in [
                0_u32,
                1,
                2,
                7,
                MAX_DERIVATION_DEPTH - 1,
                MAX_DERIVATION_DEPTH,
                MAX_DERIVATION_DEPTH + 1,
                MAX_DERIVATION_DEPTH * 4,
            ] {
                let mut derivation = leaf.clone();
                for _ in 0..depth {
                    derivation = wrap(derivation, wrapper);
                }

                // The assertion is that this line returns. A verdict of any kind
                // is a pass; ending the process is the failure.
                let certificate = check(
                    &Proposal {
                        goal: Proposition::new("s", "p", "o"),
                        derivation,
                    },
                    &snapshot,
                );
                assert!(
                    !certificate.replay_digest.is_empty(),
                    "a certificate must identify the proposal it decided"
                );
                decided += 1;
            }
        }
    }

    assert!(decided > 400, "the sweep should be wide: {decided}");
}

#[test]
fn an_empty_snapshot_never_accepts_anything() {
    // Nothing is grounded against a snapshot with nothing in it. A verdict of
    // `accepted` here would mean the kernel discharged a step from thin air.
    let snapshot = Snapshot::new();

    for leaf in leaves() {
        let certificate = check(
            &Proposal {
                goal: Proposition::new("s", "p", "o"),
                derivation: leaf,
            },
            &snapshot,
        );
        assert_ne!(
            certificate.verdict,
            capsulet_kernel::Verdict::Accepted,
            "nothing in an empty snapshot can discharge a step"
        );
    }
}
