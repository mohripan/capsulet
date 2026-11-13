//! The reference packs pass their own tests, in real containers.
//!
//! Built only with the `container-tests` feature, because each pack's
//! validators run in a container; the `verifier-isolation` gate enables it and
//! holds every test here to running. What the packs answer, and how, is tested
//! without Docker beside each pack's own test file.

use std::path::{Path, PathBuf};

use capsulet_pack::{read, test};

fn pack(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../packs")
        .join(name)
}

fn passes_its_own_test(name: &str) {
    let pack = read(&pack(name)).expect("the pack reads");
    let reports = test(&pack).expect("its validators read");
    assert!(!reports.is_empty(), "{name} registers no validator");
    for report in &reports {
        assert!(
            !report.cases.is_empty(),
            "{} has no cases",
            report.registration
        );
        for case in &report.cases {
            assert!(
                case.passed,
                "{name}: {}: {}: {}",
                report.registration, case.name, case.detail
            );
            assert!(case.detail.contains("(isolated)"), "{}", case.detail);
        }
    }
}

#[test]
fn the_contract_clauses_pack_passes_its_own_test() {
    passes_its_own_test("contract-clauses");
}
