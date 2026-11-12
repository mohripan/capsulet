//! `release check-chart`: renders the chart and proves every workload runs the
//! image `release.toml` names.
//!
//! Two renders: with the default values, each Capsulet workload must run
//! `<registry>/<namespace>/<image>:<version>`; with a digest set for every
//! component, each must run exactly that digest. Every other container must run
//! one of the third-party images the values pin by digest.
//!
//! The default render is made twice more, and both must have the same
//! [`render_digest`]: the chart's output depends only on the checkout and the
//! values, apart from what it generates at random for a new install.

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

/// The findings, and the digest of the default render.
pub(super) fn check(root: &Path, release: &Release) -> Result<(Vec<String>, String), String> {
    let values = read_yaml(root, CHART_VALUES)?;
    let mut dependencies = BTreeSet::new();
    for dependency in DEPENDENCY_IMAGES {
        dependencies.insert(
            dependency_reference(&values[*dependency]["image"])
                .map_err(|problem| format!("{CHART_VALUES}: {dependency}.image {problem}"))?,
        );
    }
    let mut findings = Vec::new();
    let documents = render(root, &[])?;
    let digest = render_digest(&documents);
    let again = render_digest(&render(root, &[])?);
    if again != digest {
        findings.push(format!(
            "two renders with the default values differ beyond the generated secrets: \
             {digest} and {again}"
        ));
    }
    findings.extend(workload_findings(&documents, release, &values));
    let defaults = containers(&documents);
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
        .flat_map(|(key, digest)| {
            let values_key = release.images.components[*key].values_key(key);
            [
                "--set".to_string(),
                format!("{values_key}.image.digest={digest}"),
            ]
        })
        .collect();
    let pinned = containers(&render(root, &overrides)?);
    expect(
        release,
        &pinned,
        Some(&digests),
        &dependencies,
        &mut findings,
    );
    Ok((findings, digest))
}

/// `sha256:<hex>` of the rendered documents with what the chart generates at
/// random for a new install masked: every Secret's values, and the `checksum/`
/// annotations that roll pods when those change. Everything else, which is
/// every workload, image, probe, security context and setting, is hashed as
/// rendered.
pub(crate) fn render_digest(documents: &[Yaml]) -> String {
    use sha2::{Digest, Sha256};
    let masked: Vec<Yaml> = documents
        .iter()
        .cloned()
        .map(|mut document| {
            mask_generated(&mut document);
            document
        })
        .collect();
    let text = serde_yaml::to_string(&masked).unwrap_or_default();
    let digest = Sha256::digest(text.as_bytes());
    let hex: String = digest
        .iter()
        .fold(String::with_capacity(64), |mut hex, byte| {
            use std::fmt::Write as _;
            let _ = write!(hex, "{byte:02x}");
            hex
        });
    format!("sha256:{hex}")
}

fn mask_generated(document: &mut Yaml) {
    if document["kind"].as_str() == Some("Secret") {
        for field in ["data", "stringData"] {
            if let Some(values) = document.get_mut(field).and_then(Yaml::as_mapping_mut) {
                for value in values.values_mut() {
                    *value = Yaml::String("<generated>".to_string());
                }
            }
        }
    }
    mask_checksums(document);
}

fn mask_checksums(value: &mut Yaml) {
    match value {
        Yaml::Mapping(mapping) => {
            if let Some(annotations) = mapping
                .get_mut("annotations")
                .and_then(Yaml::as_mapping_mut)
            {
                for (key, value) in annotations.iter_mut() {
                    if key.as_str().is_some_and(|key| key.starts_with("checksum/")) {
                        *value = Yaml::String("<generated>".to_string());
                    }
                }
            }
            for value in mapping.values_mut() {
                mask_checksums(value);
            }
        }
        Yaml::Sequence(items) => items.iter_mut().for_each(mask_checksums),
        _ => {}
    }
}

/// Renders the chart with extra `helm template` arguments, as parsed documents.
pub(crate) fn render(root: &Path, arguments: &[String]) -> Result<Vec<Yaml>, String> {
    let output = Command::new("helm")
        .args(["template", "capsulet"])
        .arg(root.join(CHART_DIRECTORY))
        .args(arguments)
        .current_dir(root)
        .output()
        .map_err(|error| format!("could not run helm: {error}"))?;
    if !output.status.success() {
        return Err(format!(
            "helm template failed: {}",
            String::from_utf8_lossy(&output.stderr).trim()
        ));
    }
    parse_documents(&String::from_utf8_lossy(&output.stdout))
}

pub(crate) fn parse_documents(text: &str) -> Result<Vec<Yaml>, String> {
    serde_yaml::Deserializer::from_str(text)
        .map(|document| {
            Yaml::deserialize(document)
                .map_err(|error| format!("helm template printed invalid YAML: {error}"))
        })
        .filter(|document| !matches!(document, Ok(Yaml::Null)))
        .collect()
}

/// Every image a workload's containers run.
pub(crate) fn workload_images(documents: &[Yaml]) -> Vec<String> {
    containers(documents)
        .into_iter()
        .map(|container| container.image)
        .collect()
}

fn containers(documents: &[Yaml]) -> Vec<Container> {
    let mut containers = Vec::new();
    for manifest in documents {
        collect(manifest, &mut containers);
    }
    containers
}

/// The pod spec of a workload manifest, and its kind and name.
fn pod_spec(manifest: &Yaml) -> Option<(&str, &str, &Yaml)> {
    let kind = manifest["kind"].as_str()?;
    let pod = match kind {
        "Deployment" | "StatefulSet" | "DaemonSet" | "ReplicaSet" | "Job" => {
            &manifest["spec"]["template"]["spec"]
        }
        "CronJob" => &manifest["spec"]["jobTemplate"]["spec"]["template"]["spec"],
        "Pod" => &manifest["spec"],
        _ => return None,
    };
    Some((
        kind,
        manifest["metadata"]["name"].as_str().unwrap_or_default(),
        pod,
    ))
}

/// The components whose pods talk to the Kubernetes API: the worker creates
/// and watches execution Jobs, and the evaluator removes finished ones. Every
/// other pod runs without a service-account token.
const KUBERNETES_API_CLIENTS: &[&str] = &["worker", "evaluator"];

/// What every workload the chart renders must have before it is installed
/// anywhere: a non-root, seccomp-confined pod; containers that cannot escalate
/// privileges and hold no capabilities, on a read-only root filesystem where
/// the component is Capsulet's own; a service account of its own, with a token
/// only where the pod uses the Kubernetes API; readiness and liveness probes
/// on every long-running container; and only secrets the chart renders or the
/// values name.
pub(crate) fn workload_findings(
    documents: &[Yaml],
    release: &Release,
    values: &Yaml,
) -> Vec<String> {
    let mut findings = Vec::new();
    let own: BTreeSet<&str> = release
        .images
        .components
        .keys()
        .map(String::as_str)
        .chain(["migrate"])
        .collect();
    for manifest in documents {
        let Some((kind, name, pod)) = pod_spec(manifest) else {
            continue;
        };
        let workload = format!("{kind}/{name}");
        let component = manifest["metadata"]["labels"]["app.kubernetes.io/component"]
            .as_str()
            .unwrap_or_default();
        let ours = own.contains(component);
        let long_running = matches!(kind, "Deployment" | "StatefulSet" | "DaemonSet");
        let pod_security = &pod["securityContext"];
        let all_containers: Vec<&Yaml> = ["initContainers", "containers"]
            .iter()
            .flat_map(|list| pod[*list].as_sequence().into_iter().flatten())
            .collect();
        let each = |check: &dyn Fn(&Yaml) -> bool| {
            all_containers
                .iter()
                .all(|container| check(&container["securityContext"]))
        };
        if pod_security["runAsNonRoot"].as_bool() != Some(true)
            && !each(&|security| security["runAsNonRoot"].as_bool() == Some(true))
        {
            findings.push(format!(
                "{workload} may run as root (`runAsNonRoot` is not true)"
            ));
        }
        if pod_security["seccompProfile"]["type"].as_str() != Some("RuntimeDefault")
            && !each(&|security| {
                security["seccompProfile"]["type"].as_str() == Some("RuntimeDefault")
            })
        {
            findings.push(format!("{workload} has no RuntimeDefault seccomp profile"));
        }
        for container in &all_containers {
            let label = format!(
                "{workload} container {}",
                container["name"].as_str().unwrap_or_default()
            );
            let security = &container["securityContext"];
            if security["allowPrivilegeEscalation"].as_bool() != Some(false) {
                findings.push(format!("{label} may escalate privileges"));
            }
            let drops_all = security["capabilities"]["drop"]
                .as_sequence()
                .is_some_and(|drop| {
                    drop.iter()
                        .any(|capability| capability.as_str() == Some("ALL"))
                });
            if !drops_all {
                findings.push(format!("{label} does not drop every capability"));
            }
            if ours && security["readOnlyRootFilesystem"].as_bool() != Some(true) {
                findings.push(format!("{label} writes to its root filesystem"));
            }
            if long_running
                && (container["readinessProbe"].is_null() || container["livenessProbe"].is_null())
            {
                findings.push(format!("{label} lacks a readiness or a liveness probe"));
            }
        }
        let mounts_token = pod["automountServiceAccountToken"].as_bool() != Some(false);
        if mounts_token && !KUBERNETES_API_CLIENTS.contains(&component) {
            findings.push(format!(
                "{workload} mounts a service-account token it has no use for"
            ));
        }
        // A token is only worth what its account may do: one that talks to the
        // API holds an account of its own, never the namespace's default.
        let account = pod["serviceAccountName"].as_str().unwrap_or("default");
        if mounts_token && account == "default" {
            findings.push(format!(
                "{workload} mounts the token of the namespace's default service account"
            ));
        }
    }
    findings.extend(secret_findings(documents, values));
    findings
}

/// Every secret a workload reads is one the chart renders, or one the values
/// name for the operator to provide: a secret named nowhere is a hidden
/// install requirement.
fn secret_findings(documents: &[Yaml], values: &Yaml) -> Vec<String> {
    let rendered: BTreeSet<String> = documents
        .iter()
        .filter(|manifest| manifest["kind"].as_str() == Some("Secret"))
        .filter_map(|manifest| manifest["metadata"]["name"].as_str().map(str::to_string))
        .collect();
    let mut named = BTreeSet::new();
    named_secrets(values, false, &mut named);
    let mut findings = Vec::new();
    for (workload, secret) in referenced_secrets(documents) {
        if !rendered.contains(&secret) && !named.contains(&secret) {
            findings.push(format!(
                "{workload} reads the secret {secret}, which the chart neither renders nor names \
                 in its values"
            ));
        }
    }
    findings
}

/// Secret names the values set for the operator to provide: every
/// `existingSecret`, and every `name` of a `...Secret` block.
fn named_secrets(value: &Yaml, in_secret_block: bool, found: &mut BTreeSet<String>) {
    if let Some(mapping) = value.as_mapping() {
        for (key, child) in mapping {
            let key = key.as_str().unwrap_or_default();
            if let Some(name) = child.as_str()
                && !name.is_empty()
                && (key == "existingSecret" || (in_secret_block && key == "name"))
            {
                found.insert(name.to_string());
            }
            named_secrets(child, key.ends_with("Secret"), found);
        }
    }
}

/// `(workload, secret)` for every secret a workload's pod reads.
pub(crate) fn referenced_secrets(documents: &[Yaml]) -> BTreeSet<(String, String)> {
    let mut found = BTreeSet::new();
    for manifest in documents {
        let Some((kind, name, pod)) = pod_spec(manifest) else {
            continue;
        };
        let workload = format!("{kind}/{name}");
        let mut add = |secret: Option<&str>| {
            if let Some(secret) = secret.filter(|secret| !secret.is_empty()) {
                found.insert((workload.clone(), secret.to_string()));
            }
        };
        for volume in pod["volumes"].as_sequence().into_iter().flatten() {
            add(volume["secret"]["secretName"].as_str());
        }
        for list in ["initContainers", "containers"] {
            for container in pod[list].as_sequence().into_iter().flatten() {
                for variable in container["env"].as_sequence().into_iter().flatten() {
                    add(variable["valueFrom"]["secretKeyRef"]["name"].as_str());
                }
                for source in container["envFrom"].as_sequence().into_iter().flatten() {
                    add(source["secretRef"]["name"].as_str());
                }
            }
        }
    }
    found
}

/// The containers of a workload manifest, init containers included.
fn collect(manifest: &Yaml, containers: &mut Vec<Container>) {
    let Some((kind, name, pod)) = pod_spec(manifest) else {
        return;
    };
    let workload = format!("{kind}/{name}");
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
