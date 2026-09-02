//! The checked-in bytes every stored digest depends on.
//!
//! Each fixture is canonical bytes plus the digest they must produce. If this
//! test fails, the encoder changed, and every digest already written by a
//! previous build is now unreproducible. The fix is a schema major bump and a
//! compatibility reader, not an edited fixture.

use std::fs;
use std::path::{Path, PathBuf};

use capsulet_ir::{Digest, verify_canonical};

fn golden_directory() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/golden")
}

fn fixtures() -> Vec<PathBuf> {
    let mut paths: Vec<PathBuf> = fs::read_dir(golden_directory())
        .expect("the golden directory is readable")
        .map(|entry| entry.expect("golden entries are readable").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "json")
        })
        .collect();
    paths.sort();
    paths
}

#[test]
fn the_corpus_is_not_empty() {
    assert!(
        !fixtures().is_empty(),
        "golden fixtures anchor the encoding; an empty corpus proves nothing"
    );
}

#[test]
fn every_fixture_is_canonical_and_digests_to_its_recorded_value() {
    for path in fixtures() {
        let bytes = fs::read(&path).expect("fixture is readable");
        let value = verify_canonical(&bytes)
            .unwrap_or_else(|error| panic!("{} is not canonical: {error}", path.display()));

        assert_eq!(
            value.to_canonical_bytes(),
            bytes,
            "{} does not re-encode to its own bytes",
            path.display()
        );

        let expected = fs::read_to_string(path.with_extension("digest"))
            .unwrap_or_else(|_| panic!("{} has no recorded digest", path.display()));
        let expected: Digest = expected.trim().parse().unwrap_or_else(|error| {
            panic!("{} records a malformed digest: {error}", path.display())
        });

        assert_eq!(
            Digest::of(&bytes),
            expected,
            "{} no longer digests to its recorded value",
            path.display()
        );
    }
}
