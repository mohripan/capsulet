//! The template is a working pack: it passes its own test, and once signed it
//! installs.
//!
//! Built only with the `container-tests` feature, because the template's
//! validator runs in a container; the `verifier-isolation` gate enables it and
//! holds every test here to running.

use std::fs;
use std::path::{Path, PathBuf};

use capsulet_pack::{TrustStore, admit, files_of, generate, read, sign, test};

fn template() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../packs/template")
}

fn copy(from: &Path, to: &Path) {
    for entry in fs::read_dir(from).expect("read") {
        let entry = entry.expect("entry");
        let target = to.join(entry.file_name());
        if entry.file_type().expect("type").is_dir() {
            fs::create_dir_all(&target).expect("dir");
            copy(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).expect("copy");
        }
    }
}

#[test]
fn the_template_passes_its_own_test() {
    let pack = read(&template()).expect("the template reads");
    let reports = test(&pack).expect("the template's validators read");
    assert!(!reports.is_empty(), "the template registers no validator");
    for report in &reports {
        assert!(
            !report.cases.is_empty(),
            "{} has no cases",
            report.registration
        );
        for case in &report.cases {
            assert!(
                case.passed,
                "{}: {}: {}",
                report.registration, case.name, case.detail
            );
            assert!(case.detail.contains("(isolated)"), "{}", case.detail);
        }
    }
}

#[test]
fn the_template_lists_its_files_and_once_signed_installs() {
    let committed = read(&template()).expect("the template reads");
    assert_eq!(
        committed.manifest.files,
        files_of(&template()).expect("its files"),
        "packs/template/pack.json lists every file as it is: run `capsulet pack sign`, or refresh it"
    );
    assert!(
        committed.signature.is_none(),
        "the template is committed unsigned"
    );

    let directory = tempfile::tempdir().expect("dir");
    copy(&template(), directory.path());
    let (key, _) = generate().expect("a key");
    sign(directory.path(), &key).expect("signs");
    let mut trust = TrustStore::new();
    trust.keys.push(
        key.trusted_for(vec!["template".to_string()])
            .expect("a public key"),
    );
    let admitted = admit(&read(directory.path()).expect("reads"), &trust, &[]).expect("installs");
    assert_eq!(
        (admitted.name.as_str(), admitted.version.as_str()),
        ("template", "0.1.0")
    );
}
