//! How a job hands files back: the markers it prints, and collecting what they name.

use std::{fs, path::Path};

use base64::{Engine as _, engine::general_purpose::STANDARD as BASE64};

use super::contract::{CollectedArtifact, InputArtifact};
use super::kubernetes::input_artifact_path;

pub(super) const ARTIFACT_MARKER: &str = "CAPSULET_ARTIFACT";

pub(super) const ARTIFACT_DIR: &str = "/capsulet/artifacts";

pub(super) const INPUT_DIR: &str = "/capsulet/inputs";

pub(super) const MAX_ARTIFACT_NAME_BYTES: usize = 255;

pub(super) const MAX_ARTIFACT_CONTENT_TYPE_BYTES: usize = 255;

pub(super) const MAX_COLLECTED_ARTIFACT_BYTES: usize = 8 * 1024 * 1024;

pub(super) fn materialize_local_inputs(artifacts: &[InputArtifact]) -> Result<(), std::io::Error> {
    let root = Path::new(INPUT_DIR);
    if root.exists() {
        fs::remove_dir_all(root)?;
    }
    for artifact in artifacts {
        let path = input_artifact_path(artifact);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::write(path, &artifact.bytes)?;
        fs::write(root.join(&artifact.name), &artifact.bytes)?;
    }
    Ok(())
}

pub(super) fn collect_artifacts_from(
    path: impl AsRef<Path>,
) -> Result<Vec<CollectedArtifact>, std::io::Error> {
    let path = path.as_ref();
    if !path.exists() {
        return Ok(Vec::new());
    }
    let mut artifacts = Vec::new();
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if !entry.file_type()?.is_file() {
            continue;
        }
        artifacts.push(CollectedArtifact {
            name: entry.file_name().to_string_lossy().into_owned(),
            content_type: "application/octet-stream".to_string(),
            bytes: fs::read(entry.path())?,
        });
    }
    Ok(artifacts)
}

pub(super) fn split_artifact_markers(
    logs: Option<String>,
) -> (Option<String>, Vec<CollectedArtifact>) {
    let Some(logs) = logs else {
        return (None, Vec::new());
    };
    let mut cleaned: Vec<String> = Vec::new();
    let mut artifacts = Vec::new();
    // A payload cut mid-line can still decode as base64, so the unterminated last
    // line is never trusted as a complete artifact.
    let unterminated_line = if logs.ends_with('\n') {
        0
    } else {
        logs.lines().count()
    };
    for (index, line) in logs.lines().enumerate() {
        let parts = line.splitn(4, '\t').collect::<Vec<_>>();
        if parts.len() == 4 && parts[0] == ARTIFACT_MARKER {
            let parsed = if index + 1 == unterminated_line {
                Err(format!(
                    "artifact {} was cut off by the log limit",
                    describe_untrusted(parts[1])
                ))
            } else {
                parse_artifact_marker(parts[1], parts[2], parts[3])
            };
            match parsed {
                Ok(artifact) => artifacts.push(artifact),
                Err(reason) => cleaned.push(format!("capsulet: dropped artifact marker: {reason}")),
            }
            continue;
        }
        cleaned.push(line.to_string());
    }
    let cleaned = cleaned.join("\n");
    if cleaned.is_empty() {
        (None, artifacts)
    } else {
        (Some(format!("{cleaned}\n")), artifacts)
    }
}

/// Builds an artifact from one marker line.
///
/// The marker shares stdout with the job's own program, so every field is
/// untrusted: the name has to survive being turned into an object storage key
/// and an artifact record, and a payload cut short by the log budget must be
/// reported instead of persisted as a complete artifact.
pub(super) fn parse_artifact_marker(
    name: &str,
    content_type: &str,
    encoded: &str,
) -> Result<CollectedArtifact, String> {
    if name.is_empty() || name.len() > MAX_ARTIFACT_NAME_BYTES {
        return Err(format!(
            "name '{}' must be between 1 and {MAX_ARTIFACT_NAME_BYTES} bytes",
            describe_untrusted(name)
        ));
    }
    if name == "."
        || name == ".."
        || name.contains('/')
        || name.contains('\\')
        || name.chars().any(char::is_control)
        || name.trim().is_empty()
    {
        return Err(format!(
            "name '{}' is not a plain file name",
            describe_untrusted(name)
        ));
    }
    if content_type.is_empty()
        || content_type.len() > MAX_ARTIFACT_CONTENT_TYPE_BYTES
        || content_type.chars().any(char::is_control)
    {
        return Err(format!("artifact {name} has an unusable content type"));
    }
    let bytes = BASE64
        .decode(encoded)
        .map_err(|_| format!("artifact {name} payload is not valid base64"))?;
    if bytes.len() > MAX_COLLECTED_ARTIFACT_BYTES {
        return Err(format!(
            "artifact {name} is {} bytes which exceeds the {MAX_COLLECTED_ARTIFACT_BYTES} byte limit",
            bytes.len()
        ));
    }
    Ok(CollectedArtifact {
        name: name.to_string(),
        content_type: content_type.to_string(),
        bytes,
    })
}

pub(super) fn describe_untrusted(value: &str) -> String {
    value.escape_default().take(64).collect()
}

pub(super) fn reset_artifact_dir() -> Result<(), std::io::Error> {
    let path = Path::new(ARTIFACT_DIR);
    if path.exists() {
        fs::remove_dir_all(path)?;
    }
    fs::create_dir_all(path)
}

pub(super) fn collect_local_artifacts() -> Result<Vec<CollectedArtifact>, std::io::Error> {
    collect_artifacts_from(ARTIFACT_DIR)
}
