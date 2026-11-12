//! `release check-chart`: renders the chart and proves every workload runs the
//! image `release.toml` names.
//!
//! Two renders: with the default values, each Capsulet workload must run
//! `<registry>/<namespace>/<image>:<version>`; with a digest set for every
//! component, each must run exactly that digest. Every other container must run
//! one of the third-party images the values pin by digest.

use std::{
    collections::{BTreeMap, BTreeSet},
    path::Path,
    process::Command,
};

use serde::Deserialize;
use serde_yaml::Value as Yaml;

use super::{
    RELEASE_FILE, Release,
    check::{CHART_VALUES, DEPENDENCY_IMAGES, dependency_reference, read_yaml},
};

const CHART_DIRECTORY: &str = "charts/capsulet";

/// What a workload's container runs, and where it came from.
struct Container {
    workload: String,
    component: Option<String>,
    image: String,
}

pub(super) fn check(root: &Path, release: &Release) -> Result<Vec<String>, String> {
    let values = read_yaml(root, CHART_VALUES)?;
    let mut dependencies = BTreeSet::new();
    for dependency in DEPENDENCY_IMAGES {
        dependencies.insert(
            dependency_reference(&values[*dependency]["image"])
                .map_err(|problem| format!("{CHART_VALUES}: {dependency}.image {problem}"))?,
        );
    }
    let mut findings = Vec::new();
    let defaults = render(root, &[])?;
    expect(release, &defaults, None, &dependencies, &mut findings);
    let digests: BTreeMap<&str, String> = release
        .images
        .components
        .keys()
        .enumerate()
        .map(|(index, key)| (key.as_str(), format!("sha256:{index:064x}")))
        .collect();
    let overrides: Vec<String> = digests
        .iter()
        .map(|(key, digest)| {
            let values_key = release.images.components[*key].values_key(key);
            format!("{values_key}.image.digest={digest}")
        })
        .collect();
    let pinned = render(root, &overrides)?;
    expect(
        release,
        &pinned,
        Some(&digests),
        &dependencies,
        &mut findings,
    );
    Ok(findings)
}

fn render(root: &Path, overrides: &[String]) -> Result<Vec<Container>, String> {
    let mut command = Command::new("helm");
    command
        .args(["template", "capsulet"])
        .arg(root.join(CHART_DIRECTORY))
        .current_dir(root);
    for value in overrides {
        command.args(["--set", value]);
    }
    let output = command
        .output()
        .map_err(|error| format!("could not run helm: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "helm template failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    let text = String::from_utf8_lossy(&output.stdout);
    let mut containers = Vec::new();
    for document in serde_yaml::Deserializer::from_str(&text) {
        let manifest = Yaml::deserialize(document)
            .map_err(|error| format!("helm template printed invalid YAML: {error}"))?;
        collect(&manifest, &mut containers);
    }
    Ok(containers)
}

/// The containers of a workload manifest, init containers included.
fn collect(manifest: &Yaml, containers: &mut Vec<Container>) {
    let kind = manifest["kind"].as_str().unwrap_or_default();
    let pod = match kind {
        "Deployment" | "StatefulSet" | "DaemonSet" | "ReplicaSet" | "Job" => {
            &manifest["spec"]["template"]["spec"]
        }
        "CronJob" => &manifest["spec"]["jobTemplate"]["spec"]["template"]["spec"],
        "Pod" => &manifest["spec"],
        _ => return,
    };
    let workload = format!(
        "{kind}/{}",
        manifest["metadata"]["name"].as_str().unwrap_or_default()
    );
    let component = manifest["metadata"]["labels"]["app.kubernetes.io/component"]
        .as_str()
        .map(str::to_string);
    for list in ["initContainers", "containers"] {
        for container in pod[list].as_sequence().into_iter().flatten() {
            containers.push(Container {
                workload: workload.clone(),
                component: component.clone(),
                image: container["image"].as_str().unwrap_or_default().to_string(),
            });
        }
    }
}

fn expect(
    release: &Release,
    containers: &[Container],
    digests: Option<&BTreeMap<&str, String>>,
    dependencies: &BTreeSet<String>,
    findings: &mut Vec<String>,
) {
    let render = if digests.is_some() {
        "with every component's digest set"
    } else {
        "with the default values"
    };
    let mut seen = BTreeSet::new();
    for container in containers {
        let owner = release.images.components.iter().find(|(_, component)| {
            let image = release.image(component);
            container.image == image
                || container
                    .image
                    .strip_prefix(&image)
                    .is_some_and(|rest| rest.starts_with([':', '@']))
        });
        let Some((key, component)) = owner else {
            if !dependencies.contains(&container.image) {
                findings.push(format!(
                    "{} ({render}) runs {:?}, which is neither a component image in \
                     {RELEASE_FILE} nor a dependency {CHART_VALUES} pins by digest",
                    container.workload, container.image
                ));
            }
            continue;
        };
        seen.insert(key.as_str());
        let mut expected = release.tagged_image(component);
        if let Some(digest) = digests.and_then(|digests| digests.get(key.as_str())) {
            expected = format!("{expected}@{digest}");
        }
        if container.image != expected {
            findings.push(format!(
                "{} ({render}) runs {:?}, {RELEASE_FILE} says {expected:?}",
                container.workload, container.image
            ));
        }
        // A component's own workload runs that component, not a neighbour.
        if let Some(label) = &container.component
            && release.images.components.contains_key(label)
            && label != key
        {
            findings.push(format!(
                "{} is the {label} component but runs the {key} image",
                container.workload
            ));
        }
    }
    for key in release.images.components.keys() {
        if !seen.contains(key.as_str()) {
            findings.push(format!(
                "no workload ({render}) runs the {key} component's image"
            ));
        }
    }
}
