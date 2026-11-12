//! `release dependency-images`: every third-party image the chart and Compose
//! pull, read with the same parsers `release check` uses and deduplicated by
//! digest, which is what the `dependency-images` gate scans.

use std::path::Path;

use super::check::{
    CHART_VALUES, COMPOSE, DEPENDENCY_IMAGES, dependency_reference, pinned, read_yaml,
    split_reference,
};

/// One third-party image, by digest, and every place that pins it.
#[derive(Debug)]
pub(crate) struct DependencyImage {
    /// `repository:tag@digest`, as the first file that pins it writes it.
    pub(crate) reference: String,
    /// The repository as the chart and Compose write it, such as
    /// `chrislusf/seaweedfs`; an image exception names this.
    pub(crate) repository: String,
    pub(crate) digest: String,
    /// `<file> <field>` for each pin, such as `compose.yaml services.postgres.image`.
    pub(crate) sources: Vec<String>,
}

/// The chart's dependency images and every Compose service that runs an image
/// rather than building one, each pinned by tag and digest. An image pinned in
/// both is listed once.
pub(crate) fn dependency_images(root: &Path) -> Result<Vec<DependencyImage>, String> {
    let mut pins = Vec::new();
    let values = read_yaml(root, CHART_VALUES)?;
    // The keys release check requires, and any other that names a repository,
    // so a third-party image added to the chart cannot go unscanned.
    let mut keys: Vec<String> = DEPENDENCY_IMAGES.iter().map(ToString::to_string).collect();
    for (key, value) in values.as_mapping().into_iter().flatten() {
        if let Some(key) = key.as_str()
            && value["image"]["repository"].is_string()
            && !keys.iter().any(|known| known == key)
        {
            keys.push(key.to_string());
        }
    }
    for key in &keys {
        let reference = dependency_reference(&values[key.as_str()]["image"])
            .map_err(|problem| format!("{CHART_VALUES}: {key}.image {problem}"))?;
        pins.push((reference, format!("{CHART_VALUES} {key}.image")));
    }
    let compose = read_yaml(root, COMPOSE)?;
    let services = compose["services"]
        .as_mapping()
        .ok_or_else(|| format!("{COMPOSE}: has no services"))?;
    for (name, service) in services {
        let (Some(name), Some(image)) = (name.as_str(), service["image"].as_str()) else {
            continue;
        };
        // A service that builds is a Capsulet component, which the image
        // workflows scan.
        if !service["build"].is_null() {
            continue;
        }
        if !pinned(image) {
            return Err(format!(
                "{COMPOSE}: services.{name}.image {image:?} must name a tag other than latest and \
                 a sha256 digest"
            ));
        }
        pins.push((
            image.to_string(),
            format!("{COMPOSE} services.{name}.image"),
        ));
    }
    let mut images: Vec<DependencyImage> = Vec::new();
    for (reference, source) in pins {
        let digest = reference
            .split_once('@')
            .map(|(_, digest)| digest.to_string())
            .unwrap_or_default();
        if let Some(image) = images.iter_mut().find(|image| image.digest == digest) {
            image.sources.push(source);
            continue;
        }
        images.push(DependencyImage {
            repository: repository(&reference).to_string(),
            reference,
            digest,
            sources: vec![source],
        });
    }
    images.sort_by(|left, right| left.reference.cmp(&right.reference));
    Ok(images)
}

/// The repository of an image reference, as the chart and Compose write it:
/// `chrislusf/seaweedfs` for `chrislusf/seaweedfs:4.53@sha256:...`.
pub(crate) fn repository(reference: &str) -> &str {
    split_reference(reference).0
}

/// Prints each image, then where it is pinned.
pub(super) fn print(root: &Path) -> Result<(), String> {
    for image in dependency_images(root)? {
        println!("{}\t{}", image.reference, image.sources.join(", "));
    }
    Ok(())
}
