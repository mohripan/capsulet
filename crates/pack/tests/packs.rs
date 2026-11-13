//! What a project may install, and every way a pack is refused.

use std::fs;
use std::path::Path;

use capsulet_pack::{
    Installed, PackError, PrivateKey, TrustStore, admit, generate, read, refresh, sign,
};

const CONTRACT: &str = "entity Organization:\n  fields:\n    name: string\n\nrelation acquired:\n  from: Organization\n  to: Organization\n  stated_by:\n    - \"{from} acquired {to}\"\n";

const REGISTRATION: &str = r#"{"name": "echo", "version": "1", "basis": {"kind": "replayable_tool"},
    "environment": {"kind": "process", "program": "echo"}}"#;

/// A pack named `sample` at `version`, written into `root` and signed with `key`.
fn write_pack(root: &Path, version: &str, key: Option<&PrivateKey>) {
    fs::create_dir_all(root.join("contracts")).expect("dir");
    fs::create_dir_all(root.join("validators")).expect("dir");
    fs::create_dir_all(root.join("cases")).expect("dir");
    fs::write(root.join("contracts/sample.contract"), CONTRACT).expect("contract");
    fs::write(root.join("validators/echo.json"), REGISTRATION).expect("registration");
    fs::write(root.join("cases/echo.json"), r#"{"cases": []}"#).expect("cases");
    let manifest = serde_json::json!({
        "schema_version": "capsulet.pack/v1",
        "name": "sample",
        "version": version,
        "description": "a pack for these tests",
        "contracts": ["contracts/sample.contract"],
        "validators": [{"registration": "validators/echo.json", "cases": "cases/echo.json"}],
        "files": []
    });
    fs::write(root.join("pack.json"), manifest.to_string()).expect("manifest");
    match key {
        Some(key) => {
            sign(root, key).expect("signs");
        }
        None => {
            refresh(root).expect("refreshes");
        }
    }
}

fn trusting(key: &PrivateKey, packs: &[&str]) -> TrustStore {
    let mut trust = TrustStore::new();
    trust.keys.push(
        key.trusted_for(packs.iter().map(ToString::to_string).collect())
            .expect("a public key"),
    );
    trust
}

fn refused(root: &Path, trust: &TrustStore, installed: &[Installed]) -> PackError {
    admit(&read(root).expect("reads"), trust, installed).expect_err("refused")
}

#[test]
fn a_signed_pack_installs_once_and_installing_it_again_changes_nothing() {
    let directory = tempfile::tempdir().expect("dir");
    let (key, _) = generate().expect("a key");
    write_pack(directory.path(), "1.0.0", Some(&key));
    let trust = trusting(&key, &["sample"]);

    let admitted = admit(&read(directory.path()).expect("reads"), &trust, &[]).expect("installs");
    assert_eq!(
        (admitted.name.as_str(), admitted.version.as_str()),
        ("sample", "1.0.0")
    );
    assert_eq!(admitted.key, key.id);
    assert!(admitted.manifest.starts_with("sha256:"));
    assert!(!admitted.already_installed);

    let installed = [Installed {
        name: "sample".to_string(),
        version: "1.0.0".to_string(),
        manifest: admitted.manifest.clone(),
    }];
    let again =
        admit(&read(directory.path()).expect("reads"), &trust, &installed).expect("installs");
    assert!(again.already_installed);
    assert_eq!(again.manifest, admitted.manifest);
}

#[test]
fn an_unsigned_or_tampered_pack_is_refused() {
    let (key, _) = generate().expect("a key");
    let trust = trusting(&key, &["sample"]);

    let unsigned = tempfile::tempdir().expect("dir");
    write_pack(unsigned.path(), "1.0.0", None);
    assert_eq!(refused(unsigned.path(), &trust, &[]).code(), "unsigned");

    let file = tempfile::tempdir().expect("dir");
    write_pack(file.path(), "1.0.0", Some(&key));
    fs::write(
        file.path().join("contracts/sample.contract"),
        CONTRACT.replace("acquired {to}", "bought {to}"),
    )
    .expect("tamper");
    assert_eq!(refused(file.path(), &trust, &[]).code(), "tampered_file");

    let planted = tempfile::tempdir().expect("dir");
    write_pack(planted.path(), "1.0.0", Some(&key));
    fs::write(planted.path().join("validators/extra.json"), REGISTRATION).expect("plant");
    assert_eq!(refused(planted.path(), &trust, &[]).code(), "unlisted_file");

    // The manifest changed after it was signed: the signature is over another one.
    let manifest = tempfile::tempdir().expect("dir");
    write_pack(manifest.path(), "1.0.0", Some(&key));
    let text = fs::read_to_string(manifest.path().join("pack.json")).expect("manifest");
    fs::write(
        manifest.path().join("pack.json"),
        text.replace("\"1.0.0\"", "\"1.0.1\""),
    )
    .expect("edit");
    assert_eq!(
        refused(manifest.path(), &trust, &[]).code(),
        "signature_for_another_manifest"
    );

    let forged = tempfile::tempdir().expect("dir");
    write_pack(forged.path(), "1.0.0", Some(&key));
    let mut signature: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(forged.path().join("pack.sig")).expect("sig"))
            .expect("json");
    let bytes = signature["signature"].as_str().expect("hex").to_string();
    let flipped = format!(
        "{}{}",
        if bytes.starts_with('0') { "1" } else { "0" },
        &bytes[1..]
    );
    signature["signature"] = serde_json::Value::String(flipped);
    fs::write(forged.path().join("pack.sig"), signature.to_string()).expect("forge");
    assert_eq!(refused(forged.path(), &trust, &[]).code(), "bad_signature");
}

#[test]
fn only_a_key_the_project_trusts_for_this_pack_installs_it() {
    let directory = tempfile::tempdir().expect("dir");
    let (key, _) = generate().expect("a key");
    let (stranger, _) = generate().expect("another key");
    write_pack(directory.path(), "1.0.0", Some(&key));

    assert_eq!(
        refused(directory.path(), &trusting(&stranger, &["*"]), &[]).code(),
        "unknown_key"
    );
    assert_eq!(
        refused(directory.path(), &trusting(&key, &["another-pack"]), &[]).code(),
        "key_not_trusted_for_pack"
    );
    assert!(
        admit(
            &read(directory.path()).expect("reads"),
            &trusting(&key, &["*"]),
            &[]
        )
        .is_ok()
    );

    let mut lying = trusting(&key, &["sample"]);
    lying.keys[0].id = stranger.id.clone();
    assert_eq!(refused(directory.path(), &lying, &[]).code(), "trust_store");
}

#[test]
fn a_version_does_not_go_backwards_or_change_its_content() {
    let (key, _) = generate().expect("a key");
    let trust = trusting(&key, &["sample"]);
    let older = tempfile::tempdir().expect("dir");
    write_pack(older.path(), "1.1.0", Some(&key));
    let newer = Installed {
        name: "sample".to_string(),
        version: "1.2.0".to_string(),
        manifest: "sha256:0000000000000000000000000000000000000000000000000000000000000000"
            .to_string(),
    };
    assert_eq!(
        refused(older.path(), &trust, std::slice::from_ref(&newer)).code(),
        "downgrade"
    );

    let same = Installed {
        version: "1.1.0".to_string(),
        ..newer
    };
    assert_eq!(
        refused(older.path(), &trust, &[same]).code(),
        "version_reused"
    );
}

#[test]
fn a_pack_whose_contract_or_validator_does_not_read_is_refused() {
    let (key, _) = generate().expect("a key");
    let trust = trusting(&key, &["sample"]);

    let contract = tempfile::tempdir().expect("dir");
    write_pack(contract.path(), "1.0.0", None);
    fs::write(
        contract.path().join("contracts/sample.contract"),
        "relation acquired:\n  from: Nothing\n",
    )
    .expect("break");
    sign(contract.path(), &key).expect("signs");
    assert_eq!(refused(contract.path(), &trust, &[]).code(), "bad_contract");

    let validator = tempfile::tempdir().expect("dir");
    write_pack(validator.path(), "1.0.0", None);
    fs::write(
        validator.path().join("validators/echo.json"),
        REGISTRATION.replace(
            r#""program": "echo"}"#,
            r#""program": "echo"}, "capabilities": ["network"]"#,
        ),
    )
    .expect("grant the network");
    sign(validator.path(), &key).expect("signs");
    assert_eq!(
        refused(validator.path(), &trust, &[]).code(),
        "bad_validator"
    );

    let escape = tempfile::tempdir().expect("dir");
    write_pack(escape.path(), "1.0.0", Some(&key));
    let mut manifest: serde_json::Value = serde_json::from_str(
        &fs::read_to_string(escape.path().join("pack.json")).expect("manifest"),
    )
    .expect("json");
    manifest["files"][0]["path"] = serde_json::Value::String("../outside".to_string());
    fs::write(escape.path().join("pack.json"), manifest.to_string()).expect("edit");
    assert_eq!(refused(escape.path(), &trust, &[]).code(), "unsafe_path");
}

#[test]
fn a_pack_travels_as_one_json_document_and_is_checked_the_same_way() {
    let directory = tempfile::tempdir().expect("dir");
    let (key, _) = generate().expect("a key");
    write_pack(directory.path(), "1.0.0", Some(&key));
    let trust = trusting(&key, &["sample"]);
    let on_disk = admit(&read(directory.path()).expect("reads"), &trust, &[]).expect("installs");

    let bundle = read(directory.path()).expect("reads").bundle();
    let travelled: capsulet_pack::Bundle =
        serde_json::from_str(&serde_json::to_string(&bundle).expect("encodes")).expect("decodes");
    let pack = capsulet_pack::Pack::from_bundle(travelled.clone()).expect("a pack");
    assert!(pack.root.is_none());
    let over_the_wire = admit(&pack, &trust, &[]).expect("installs the same");
    assert_eq!(over_the_wire.manifest, on_disk.manifest);

    let mut tampered = travelled;
    let contract = tampered
        .files
        .get_mut("contracts/sample.contract")
        .expect("the contract travels");
    contract.replace_range(0..2, "41");
    let refused = admit(
        &capsulet_pack::Pack::from_bundle(tampered).expect("a pack"),
        &trust,
        &[],
    )
    .expect_err("a changed file is refused however it arrives");
    assert_eq!(refused.code(), "tampered_file");
    assert!(
        capsulet_pack::test(&pack).is_err(),
        "a pack from a bundle has no directory to test in"
    );
}

/// `sample` with one obligations file holding `contract`, signed with `key`.
fn with_obligations(root: &Path, contract: &serde_json::Value, key: &PrivateKey) {
    write_pack(root, "1.0.0", None);
    fs::create_dir_all(root.join("obligations")).expect("dir");
    fs::write(root.join("obligations/sample.json"), contract.to_string()).expect("obligations");
    let path = root.join("pack.json");
    let mut manifest: serde_json::Value =
        serde_json::from_slice(&fs::read(&path).expect("manifest")).expect("json");
    manifest["obligations"] = serde_json::json!(["obligations/sample.json"]);
    fs::write(&path, manifest.to_string()).expect("manifest");
    sign(root, key).expect("signs");
}

fn obligations_of(obligations: &serde_json::Value) -> serde_json::Value {
    serde_json::json!({
        "id": "sample", "version": "1", "inputs": {}, "outputs": {}, "allowed_effects": [],
        "obligations": obligations
    })
}

#[test]
fn a_packs_obligations_install_only_when_their_policy_can_be_met() {
    let (key, _) = generate().expect("a key");
    let trust = trusting(&key, &["sample"]);

    let reading = serde_json::json!({
        "id": "reading", "statement": "the reading holds", "owner": "human",
        "authority": "review:legal", "accepts": ["human_review"]
    });
    let directory = tempfile::tempdir().expect("dir");
    with_obligations(
        directory.path(),
        &obligations_of(&serde_json::json!([reading])),
        &key,
    );
    let pack = read(directory.path()).expect("reads");
    admit(&pack, &trust, &[]).expect("installs");
    let contracts = capsulet_pack::obligations(&pack).expect("its obligations");
    assert_eq!(contracts[0].obligations[0].id.as_str(), "reading");

    let unreviewable = serde_json::json!({
        "id": "reading", "statement": "the reading holds", "owner": "human",
        "accepts": ["human_review"]
    });
    let named_unused = serde_json::json!({
        "id": "reading", "statement": "the reading holds", "owner": "human",
        "authority": "review:legal", "accepts": ["mechanical"]
    });
    for (contract, why) in [
        (serde_json::json!({"id": "sample"}), "not a contract"),
        (
            obligations_of(&serde_json::json!([reading.clone(), reading])),
            "an obligation stated twice",
        ),
        (
            obligations_of(&serde_json::json!([unreviewable])),
            "only a review discharges it, and it names no authority to review it under",
        ),
        (
            obligations_of(&serde_json::json!([named_unused])),
            "it names an authority, and no review discharges it",
        ),
    ] {
        let directory = tempfile::tempdir().expect("dir");
        with_obligations(directory.path(), &contract, &key);
        let error = refused(directory.path(), &trust, &[]);
        assert_eq!(error.code(), "bad_obligations", "{why}: {error}");
    }
}

#[test]
fn a_manifest_without_obligations_digests_as_it_always_did() {
    let directory = tempfile::tempdir().expect("dir");
    write_pack(directory.path(), "1.0.0", None);
    let manifest = read(directory.path()).expect("reads").manifest;
    let encoded = serde_json::to_value(&manifest).expect("encodes");
    assert!(encoded.get("obligations").is_none(), "{encoded}");
}
