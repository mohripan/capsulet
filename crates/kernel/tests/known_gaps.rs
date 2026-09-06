//! Gaps between what the kernel claims and what it enforces.
//!
//! See `crates/ir/tests/known_gaps.rs` for the convention: tests here describe
//! current wrong behaviour so it is visible, and the task that fixes a gap
//! inverts its assertion.
//!
//! Tracked by `docs/superpowers/plans/2026-09-06-correctness-kernel-robustness.md`.

use capsulet_kernel::check;
use capsulet_kernel::ir::{Proposal, Proposition, Rule};
use capsulet_kernel::snapshot::Snapshot;

/// Gap 3: `check` is not total.
///
/// `lib.rs` says "every check is total: `check` always terminates with a
/// verdict", and `check`'s own doc says "Always terminates." Neither holds:
/// `derive` recurses through `Rule::Trust` and `Rule::Interpret` with no depth
/// bound, so a sufficiently nested derivation exhausts the stack.
///
/// This is not a panic that a caller could catch. The process exits with
/// `STATUS_STACK_OVERFLOW` (`0xc00000fd`), taking the worker with it. `Rule` is
/// `Deserialize`, so the shape arrives from a proposer — and serde's own
/// recursion hits the same wall before the kernel is even entered, which is why
/// Task 1 bounds the parse boundary as well as `derive`.
///
/// Ignored because it does exactly what it describes: it kills the test binary,
/// and every other test in this crate with it. Run it deliberately with
/// `cargo test -p capsulet-kernel --test known_gaps -- --ignored`.
///
/// Fixed by Task 1, after which this becomes `crates/kernel/tests/depth.rs`
/// asserting a `DepthExceeded` rejection, and runs un-ignored.
#[test]
#[ignore = "exits the process with STATUS_STACK_OVERFLOW; see Task 1"]
fn gap_check_does_not_terminate_on_a_deeply_nested_derivation() {
    let mut derivation = Rule::Attest {
        claim_id: "nonexistent".to_string(),
    };
    for _ in 0..100_000 {
        derivation = Rule::Trust {
            premise: Box::new(derivation),
            min_authority: "primary".to_string(),
        };
    }

    let proposal = Proposal {
        goal: Proposition::new("s", "p", "o"),
        derivation,
    };

    // The claim under test: this returns a certificate. Any certificate.
    let certificate = check(&proposal, &Snapshot::new());

    unreachable!(
        "GAP: control never reaches here; the stack overflows first. Verdict would be {:?}",
        certificate.verdict
    );
}
