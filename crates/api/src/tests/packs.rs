//! Installing signed domain packs, and definitions that pin them.

use super::*;

/// A signed pack named `sample`, as the bundle the install endpoint takes, and the
/// key that signed it.
fn signed_pack(version: &str) -> (Value, capsulet_pack::PrivateKey) {
    let directory = std::env::temp_dir().join(format!(
        "capsulet-api-pack-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos())
    ));
    std::fs::create_dir_all(directory.join("contracts")).expect("dir");
    std::fs::write(
        directory.join("contracts/sample.contract"),
        "entity Organization:\n  fields:\n    name: string\n",
    )
    .expect("contract");
    std::fs::write(
        directory.join("pack.json"),
        json!({
            "schema_version": "capsulet.pack/v1",
            "name": "sample",
            "version": version,
            "description": "a pack for these tests",
            "contracts": ["contracts/sample.contract"],
            "validators": [],
            "files": []
        })
        .to_string(),
    )
    .expect("manifest");
    let (key, _) = capsulet_pack::generate().expect("a key");
    capsulet_pack::sign(&directory, &key).expect("signs");
    let bundle = capsulet_pack::read(&directory).expect("reads").bundle();
    let _ = std::fs::remove_dir_all(&directory);
    (serde_json::to_value(bundle).expect("encodes"), key)
}

/// The authenticated app, reading pack trust stores from a fresh directory in
/// which the default project trusts `key` for `sample`.
fn pack_app(
    store: FakeStore,
    key: &capsulet_pack::PrivateKey,
) -> (axum::Router, std::path::PathBuf) {
    let trust = std::env::temp_dir().join(format!(
        "capsulet-api-trust-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos())
    ));
    std::fs::create_dir_all(trust.join("default")).expect("dir");
    let mut store_file = capsulet_pack::TrustStore::new();
    store_file.keys.push(
        key.trusted_for(vec!["sample".to_string()])
            .expect("a public key"),
    );
    std::fs::write(
        trust.join("default").join("default.json"),
        serde_json::to_string(&store_file).expect("encodes"),
    )
    .expect("trust store");
    store.ensure_default_projects();
    let auth = AuthConfig::from_json(
        r#"[{"name":"owner","role":"admin","token":"admin-token-0123456789-abcdefghijkl"}]"#,
    )
    .expect("auth config");
    let app = router(
        AppState::new(store, FakeObjectStore::default(), ["mini".to_string()])
            .with_auth(auth)
            .with_pack_trust(&trust),
    );
    (app, trust)
}

#[tokio::test]
async fn a_signed_pack_installs_into_its_project_once_and_is_listed_there_only() {
    let (bundle, key) = signed_pack("1.0.0");
    let (app, trust) = pack_app(FakeStore::default(), &key);

    let (status, first) = send(
        &app,
        admin_request(Method::POST, "/v1/packs", Some(bundle.clone())),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK, "{first}");
    assert_eq!(first["name"], "sample");
    assert_eq!(first["newly_installed"], true);
    assert_eq!(first["signed_by"], key.id.as_str());
    assert_eq!(first["contracts"][0], "contracts/sample.contract");

    let (status, again) = send(&app, admin_request(Method::POST, "/v1/packs", Some(bundle))).await;
    assert_eq!(status, axum::http::StatusCode::OK, "{again}");
    assert_eq!(again["newly_installed"], false);
    assert_eq!(again["manifest"], first["manifest"]);

    let (_, listed) = send(&app, admin_request(Method::GET, "/v1/packs", None)).await;
    assert_eq!(
        listed["packs"].as_array().map(Vec::len),
        Some(1),
        "{listed}"
    );
    let (status, _) = send(
        &app,
        admin_request(Method::GET, "/v1/packs/sample/1.0.0", None),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::OK);

    let mut elsewhere = admin_request(Method::GET, "/v1/packs/sample/1.0.0", None);
    elsewhere.headers_mut().insert(
        "x-capsulet-project-id",
        "another-project".parse().expect("header"),
    );
    let (status, missing) = send(&app, elsewhere).await;
    assert_eq!(status, axum::http::StatusCode::NOT_FOUND, "{missing}");
    assert_eq!(missing["code"], "pack_not_found");
    let _ = std::fs::remove_dir_all(trust);
}

#[tokio::test]
async fn a_tampered_or_untrusted_pack_is_refused_and_nothing_is_installed() {
    let (bundle, key) = signed_pack("1.0.0");
    let store = FakeStore::default();
    let (app, trust) = pack_app(store.clone(), &key);

    let mut tampered = bundle.clone();
    let contract = tampered["files"]["contracts/sample.contract"]
        .as_str()
        .expect("hex")
        .to_string();
    tampered["files"]["contracts/sample.contract"] = Value::String(format!("41{}", &contract[2..]));
    let (status, refused) = send(
        &app,
        admin_request(Method::POST, "/v1/packs", Some(tampered)),
    )
    .await;
    assert_eq!(
        status,
        axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        "{refused}"
    );
    assert_eq!(refused["code"], "pack_refused");
    assert!(
        refused["message"]
            .as_str()
            .is_some_and(|message| message.contains("(tampered_file)")),
        "{refused}"
    );

    let (stranger_bundle, _) = signed_pack("1.0.0");
    let (status, refused) = send(
        &app,
        admin_request(Method::POST, "/v1/packs", Some(stranger_bundle)),
    )
    .await;
    assert_eq!(
        status,
        axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        "{refused}"
    );
    assert!(
        refused["message"]
            .as_str()
            .is_some_and(|message| message.contains("(unknown_key)")),
        "{refused}"
    );

    // An installation with no trust store trusts nobody.
    let (status, refused) = send(
        &authenticated_app(FakeStore::default()),
        admin_request(Method::POST, "/v1/packs", Some(bundle)),
    )
    .await;
    assert_eq!(
        status,
        axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        "{refused}"
    );
    assert!(
        refused["message"]
            .as_str()
            .is_some_and(|message| message.contains("(unknown_key)")),
        "{refused}"
    );
    assert!(store.domain_packs.lock().expect("packs mutex").is_empty());
    let _ = std::fs::remove_dir_all(trust);
}

#[tokio::test]
async fn a_definition_registers_only_when_every_pack_it_pins_is_installed_as_pinned() {
    let (bundle, key) = signed_pack("1.0.0");
    let (app, trust) = pack_app(FakeStore::default(), &key);
    let pinned = |manifest: &str| {
        let mut definition = ir_definition_body(capsulet_ir::definition::AssuranceMode::Observe);
        definition["packs"] =
            json!([{ "name": "sample", "version": "1.0.0", "manifest": manifest }]);
        json!({ "definition": definition })
    };
    let manifest =
        capsulet_pack::Pack::from_bundle(serde_json::from_value(bundle.clone()).expect("a bundle"))
            .expect("a pack")
            .manifest
            .digest()
            .expect("a digest")
            .to_string();

    let (status, refused) = send(
        &app,
        admin_request(Method::POST, "/v1/ir/definitions", Some(pinned(&manifest))),
    )
    .await;
    assert_eq!(
        status,
        axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        "{refused}"
    );
    assert_eq!(refused["code"], "pack_not_installed");

    let (status, _) = send(&app, admin_request(Method::POST, "/v1/packs", Some(bundle))).await;
    assert_eq!(status, axum::http::StatusCode::OK);
    let other = "sha256:0000000000000000000000000000000000000000000000000000000000000000";
    let (status, refused) = send(
        &app,
        admin_request(Method::POST, "/v1/ir/definitions", Some(pinned(other))),
    )
    .await;
    assert_eq!(
        status,
        axum::http::StatusCode::UNPROCESSABLE_ENTITY,
        "{refused}"
    );
    assert!(
        refused["message"]
            .as_str()
            .is_some_and(|message| message.contains("holds it at")),
        "{refused}"
    );

    let (status, registered) = send(
        &app,
        admin_request(Method::POST, "/v1/ir/definitions", Some(pinned(&manifest))),
    )
    .await;
    assert_eq!(status, axum::http::StatusCode::CREATED, "{registered}");
    let _ = std::fs::remove_dir_all(trust);
}
