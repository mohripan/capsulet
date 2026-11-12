//! `release images`: every component's image, built the way CI builds it.
//!
//! The plan comes from `release.toml` alone: the image name, the Dockerfile
//! and target, and the build arguments the Dockerfiles turn into OCI labels.
//! `--plan` prints it as the GitHub Actions matrix the supply-chain workflow
//! builds from, so CI and this gate cannot name or label an image differently.
//! Without `--plan`, each image is built locally, its labels and user are
//! checked, and every image the run built is removed again.

use std::{collections::BTreeMap, path::Path, process::Command};

use serde::Serialize;
use serde_json::Value;

use super::{BuildIdentity, Release};

#[derive(Serialize)]
pub(super) struct Entry {
    component: String,
    /// `<registry>/<namespace>/<image>`, without a tag.
    image: String,
    title: String,
    version: String,
    dockerfile: String,
    context: String,
    /// Empty for a Dockerfile with a single final stage.
    target: String,
    /// `KEY=value` lines, the form `docker/build-push-action` takes.
    build_args: String,
}

pub(super) fn plan(release: &Release, identity: &BuildIdentity) -> Vec<Entry> {
    let build_args = identity
        .build_args()
        .iter()
        .map(|(key, value)| format!("{key}={value}"))
        .collect::<Vec<_>>()
        .join("\n");
    release
        .images
        .components
        .iter()
        .map(|(key, component)| Entry {
            component: key.clone(),
            image: release.image(component),
            title: component.image.clone(),
            version: release.product.version.clone(),
            dockerfile: component.dockerfile.clone(),
            context: component.context.clone(),
            target: component.target.clone().unwrap_or_default(),
            build_args: build_args.clone(),
        })
        .collect()
}

pub(super) fn print_plan(root: &Path, release: &Release) -> Result<(), String> {
    let identity = BuildIdentity::current(root)?;
    let matrix = serde_json::json!({ "include": plan(release, &identity) });
    println!(
        "{}",
        serde_json::to_string(&matrix).map_err(|error| error.to_string())?
    );
    Ok(())
}

pub(super) fn build_and_check(root: &Path, release: &Release) -> Result<(), String> {
    let identity = BuildIdentity::current(root)?;
    let entries = plan(release, &identity);
    let mut built = Vec::new();
    let result = build_all(root, release, &identity, &entries, &mut built);
    for tag in &built {
        let _ = Command::new("docker")
            .args(["image", "rm", "--force", tag])
            .output();
    }
    if result.is_ok() {
        println!(
            "[release] built {} image(s) labelled {} at {}; removed them again",
            entries.len(),
            identity.version,
            identity.revision
        );
    }
    result
}

pub(super) fn build_all(
    root: &Path,
    release: &Release,
    identity: &BuildIdentity,
    entries: &[Entry],
    built: &mut Vec<String>,
) -> Result<(), String> {
    for entry in entries {
        // A local name nobody pulls or pushes, unique to this run.
        let tag = format!(
            "capsulet-verify/{}:{}-{}",
            entry.title,
            entry.version,
            std::process::id()
        );
        let mut command = Command::new("docker");
        command
            .env("DOCKER_BUILDKIT", "1")
            .args(["build", "--progress", "plain", "--file"])
            .arg(root.join(&entry.dockerfile));
        if !entry.target.is_empty() {
            command.args(["--target", &entry.target]);
        }
        for (key, value) in identity.build_args() {
            command.args(["--build-arg", &format!("{key}={value}")]);
        }
        command.args(["--tag", &tag]).arg(root.join(&entry.context));
        println!("[release] building {} ({})", entry.image, entry.component);
        let status = command
            .status()
            .map_err(|error| format!("could not run docker build: {error}"))?;
        if !status.success() {
            return Err(format!("docker build of {} failed", entry.image));
        }
        built.push(tag.clone());
        inspect(&tag, entry, release, identity)?;
        println!("[release] {} carries its release labels", entry.image);
    }
    Ok(())
}

/// `release inspect <component> <reference>`: an image already built, by CI
/// or by hand, carries the labels this checkout's release identity gives the
/// component and runs as a non-root user. The image must be in the local Docker
/// store, as one CI just built or pulled by digest is.
pub(super) fn inspect_reference(
    root: &Path,
    release: &Release,
    component: &str,
    reference: &str,
) -> Result<(), String> {
    let identity = BuildIdentity::current(root)?;
    let entry = plan(release, &identity)
        .into_iter()
        .find(|entry| entry.component == component)
        .ok_or_else(|| format!("{component} is not a component in release.toml"))?;
    inspect(reference, &entry, release, &identity)?;
    println!(
        "[release] {reference} carries the {component} labels of {} at {} and runs as a non-root \
         user",
        identity.version, identity.revision
    );
    Ok(())
}

/// The image must say what it is, where it came from and when, and must not
/// run as root.
fn inspect(
    tag: &str,
    entry: &Entry,
    release: &Release,
    identity: &BuildIdentity,
) -> Result<(), String> {
    let output = Command::new("docker")
        .args(["image", "inspect", "--format", "{{json .Config}}", tag])
        .output()
        .map_err(|error| format!("could not run docker image inspect: {error}"))?;
    if !output.status.success() {
        return Err(format!("docker image inspect {tag} failed"));
    }
    let config: Value = serde_json::from_slice(&output.stdout)
        .map_err(|error| format!("docker image inspect {tag} printed invalid JSON: {error}"))?;
    let expected = BTreeMap::from([
        ("org.opencontainers.image.title", entry.title.as_str()),
        (
            "org.opencontainers.image.version",
            identity.version.as_str(),
        ),
        (
            "org.opencontainers.image.revision",
            identity.revision.as_str(),
        ),
        (
            "org.opencontainers.image.created",
            identity.created.as_str(),
        ),
        ("org.opencontainers.image.source", identity.source.as_str()),
        (
            "org.opencontainers.image.licenses",
            release.product.license.as_str(),
        ),
    ]);
    let mut problems = Vec::new();
    for (label, value) in expected {
        let actual = config["Labels"][label].as_str();
        if actual != Some(value) {
            problems.push(format!("{label} is {actual:?}, expected {value:?}"));
        }
    }
    let user = config["User"].as_str().unwrap_or_default();
    let name = user.split(':').next().unwrap_or_default();
    if name.is_empty() || name == "0" || name == "root" {
        problems.push(format!("runs as {user:?}; it must run as a non-root user"));
    }
    if problems.is_empty() {
        Ok(())
    } else {
        Err(format!("{}: {}", entry.image, problems.join("; ")))
    }
}
