//! `release manifest`: what a release published, by digest.
//!
//! The release workflow runs this last, after it has verified every image's
//! signature, provenance and SBOM again. It reads the digest record each image
//! job wrote, holds them to `release.toml` (every component exactly once, under
//! the name the release gives it), and writes one JSON document naming each
//! image by digest, the chart package and the source SBOM by SHA-256, and the
//! revision they were all built from. The workflow then attests that document.

use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use serde::Deserialize;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};

use super::{BuildIdentity, Release, check::is_digest};

/// What one image job recorded: `digest-<component>.json`.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Record {
    component: String,
    image: String,
    digest: String,
    tags: Vec<String>,
    sbom_sha256: String,
}

const USAGE: &str = "usage: capsulet-xtask release manifest --digests <directory> --chart \
                     <directory> --source-sbom <file> --output <file>";

pub(super) fn write(root: &Path, release: &Release, arguments: &[String]) -> Result<(), String> {
    let option = |name: &str| {
        arguments
            .windows(2)
            .find(|pair| pair[0] == name)
            .map(|pair| PathBuf::from(&pair[1]))
            .ok_or_else(|| USAGE.to_string())
    };
    let digests = option("--digests")?;
    let chart = option("--chart")?;
    let source_sbom = option("--source-sbom")?;
    let output = option("--output")?;
    let identity = BuildIdentity::current(root)?;
    let manifest = build(release, &identity, &digests, &chart, &source_sbom)?;
    fs::write(
        &output,
        serde_json::to_string_pretty(&manifest).map_err(|error| error.to_string())? + "\n",
    )
    .map_err(|error| format!("{}: {error}", output.display()))?;
    println!(
        "[release] {} names {} image(s) by digest, the chart and the source SBOM",
        output.display(),
        manifest["images"].as_array().map_or(0, Vec::len)
    );
    Ok(())
}

fn build(
    release: &Release,
    identity: &BuildIdentity,
    digests: &Path,
    chart: &Path,
    source_sbom: &Path,
) -> Result<Value, String> {
    let mut records = BTreeMap::new();
    for path in files(digests, ".json")? {
        let record: Record = serde_json::from_str(
            &fs::read_to_string(&path).map_err(|error| format!("{}: {error}", path.display()))?,
        )
        .map_err(|error| format!("{}: {error}", path.display()))?;
        if records.contains_key(&record.component) {
            return Err(format!("two digest records for {}", record.component));
        }
        records.insert(record.component.clone(), record);
    }
    let mut images = Vec::new();
    for (key, component) in &release.images.components {
        let record = records
            .remove(key)
            .ok_or_else(|| format!("no digest record for the {key} component"))?;
        let image = release.image(component);
        if record.image != image {
            return Err(format!(
                "{key} was published as {}, but release.toml names {image}",
                record.image
            ));
        }
        if !is_digest(&record.digest) {
            return Err(format!(
                "{key}'s digest {:?} is not a sha256 digest",
                record.digest
            ));
        }
        if record.tags.is_empty()
            || record
                .tags
                .iter()
                .any(|tag| !tag.starts_with(&format!("{image}:")))
        {
            return Err(format!(
                "{key}'s tags {:?} are not tags of {image}",
                record.tags
            ));
        }
        if !is_hex_digest(&record.sbom_sha256) {
            return Err(format!(
                "{key}'s SBOM checksum {:?} is not a SHA-256",
                record.sbom_sha256
            ));
        }
        images.push(json!({
            "component": key,
            "image": image,
            "digest": record.digest,
            "reference": format!("{image}@{}", record.digest),
            "tags": record.tags,
            "sbom_sha256": record.sbom_sha256,
        }));
    }
    if let Some(extra) = records.keys().next() {
        return Err(format!(
            "a digest record for {extra}, which is not a component in release.toml"
        ));
    }
    let packages = files(chart, ".tgz")?;
    let [package] = packages.as_slice() else {
        return Err(format!(
            "{} holds {} chart packages, not one",
            chart.display(),
            packages.len()
        ));
    };
    let bundle = PathBuf::from(format!("{}.sigstore.json", package.display()));
    Ok(json!({
        "schema_version": 1,
        "product": release.product.name,
        "version": identity.version,
        "source": identity.source,
        "revision": identity.revision,
        "created": identity.created,
        "images": images,
        "chart": {
            "file": file_name(package),
            "sha256": sha256(package)?,
            "signature_bundle": bundle.exists().then(|| file_name(&bundle)),
        },
        "source_sbom": {
            "file": file_name(source_sbom),
            "sha256": sha256(source_sbom)?,
        },
    }))
}

fn files(directory: &Path, extension: &str) -> Result<Vec<PathBuf>, String> {
    let mut found: Vec<PathBuf> = fs::read_dir(directory)
        .map_err(|error| format!("{}: {error}", directory.display()))?
        .filter_map(Result::ok)
        .map(|entry| entry.path())
        .filter(|path| path.to_string_lossy().ends_with(extension))
        .collect();
    found.sort();
    Ok(found)
}

fn file_name(path: &Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default()
}

fn sha256(path: &Path) -> Result<String, String> {
    use std::fmt::Write as _;
    let bytes = fs::read(path).map_err(|error| format!("{}: {error}", path.display()))?;
    let mut hex = String::with_capacity(64);
    for byte in Sha256::digest(&bytes) {
        let _ = write!(hex, "{byte:02x}");
    }
    Ok(hex)
}

fn is_hex_digest(value: &str) -> bool {
    value.len() == 64 && value.chars().all(|c| matches!(c, '0'..='9' | 'a'..='f'))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::build;
    use crate::release::{BuildIdentity, Release};

    fn release() -> Release {
        toml::from_str(
            r#"
            [product]
            name = "capsulet"
            version = "0.1.0"
            source = "https://github.com/acme/widget"
            license = "Apache-2.0"
            [images]
            registry = "ghcr.io"
            namespace = "acme"
            [images.components.api]
            image = "capsulet-api"
            dockerfile = "crates/Dockerfile"
            target = "api"
            [chart]
            repository = "charts"
            [compatibility]
            api = ">=0.1.0, <0.2.0"
            "#,
        )
        .expect("release")
    }

    fn identity() -> BuildIdentity {
        BuildIdentity {
            version: "0.1.0".to_string(),
            revision: "0".repeat(40),
            created: "2026-01-01T00:00:00Z".to_string(),
            source: "https://github.com/acme/widget".to_string(),
        }
    }

    fn record(image: &str) -> String {
        format!(
            r#"{{"component":"api","image":"{image}","digest":"sha256:{}","tags":["{image}:main"],"sbom_sha256":"{}"}}"#,
            "a".repeat(64),
            "b".repeat(64)
        )
    }

    #[test]
    fn the_manifest_names_each_component_by_digest() {
        let directory = tempfile::tempdir().expect("directory");
        let digests = directory.path().join("digests");
        let chart = directory.path().join("dist");
        fs::create_dir_all(&digests).expect("digests");
        fs::create_dir_all(&chart).expect("chart");
        fs::write(
            digests.join("digest-api.json"),
            record("ghcr.io/acme/capsulet-api"),
        )
        .expect("record");
        fs::write(chart.join("capsulet-0.1.0.tgz"), b"chart").expect("chart");
        let sbom = directory.path().join("sbom.cdx.json");
        fs::write(&sbom, b"{}").expect("sbom");
        let manifest = build(&release(), &identity(), &digests, &chart, &sbom).expect("manifest");
        assert_eq!(
            manifest["images"][0]["reference"],
            format!("ghcr.io/acme/capsulet-api@sha256:{}", "a".repeat(64))
        );
        assert_eq!(manifest["chart"]["file"], "capsulet-0.1.0.tgz");
        assert_eq!(
            manifest["source_sbom"]["sha256"],
            "44136fa355b3678a1146ad16f7e8649e94fb4fc21fe77e8310c060f61caaff8a"
        );
    }

    #[test]
    fn an_image_under_another_name_or_a_missing_component_is_refused() {
        let directory = tempfile::tempdir().expect("directory");
        let digests = directory.path().join("digests");
        let chart = directory.path().join("dist");
        fs::create_dir_all(&digests).expect("digests");
        fs::create_dir_all(&chart).expect("chart");
        fs::write(chart.join("capsulet-0.1.0.tgz"), b"chart").expect("chart");
        let sbom = directory.path().join("sbom.cdx.json");
        fs::write(&sbom, b"{}").expect("sbom");
        let missing = build(&release(), &identity(), &digests, &chart, &sbom).expect_err("missing");
        assert!(
            missing.contains("no digest record for the api component"),
            "{missing}"
        );
        fs::write(
            digests.join("digest-api.json"),
            record("ghcr.io/acme/capsulet/capsulet-api"),
        )
        .expect("record");
        let renamed = build(&release(), &identity(), &digests, &chart, &sbom).expect_err("renamed");
        assert!(
            renamed.contains("but release.toml names ghcr.io/acme/capsulet-api"),
            "{renamed}"
        );
    }
}
