//! `release set <version> (--dry-run | --write) [--allow-dirty]`: rewrites every
//! manifest that carries the product version.
//!
//! Each rewrite replaces one value in place and leaves every other byte of the
//! file alone, so the result is the same however often it runs and reviews as
//! the version change and nothing else. `release check`, which reads the files
//! with real parsers, then has to agree.

use std::{collections::BTreeSet, fs, path::Path};

use semver::Version;

use super::{
    RELEASE_FILE, Release,
    check::{
        self, CARGO_LOCK, CARGO_MANIFEST, CHART, CHART_VALUES, DASHBOARD_LOCK, DASHBOARD_PACKAGE,
        OPENAPI, PYTHON_VERSION_FILE, historical, tracked_files,
    },
    git, report,
};

enum Mode {
    DryRun,
    Write,
}

struct Edit {
    path: String,
    before: String,
    after: String,
}

pub(super) fn run(root: &Path, release: &Release, arguments: &[String]) -> Result<(), String> {
    let usage = || {
        "usage: capsulet-xtask release set <version> (--dry-run | --write) [--allow-dirty]"
            .to_string()
    };
    let mut version = None;
    let mut mode = None;
    let mut allow_dirty = false;
    for argument in arguments {
        match argument.as_str() {
            "--dry-run" if mode.is_none() => mode = Some(Mode::DryRun),
            "--write" if mode.is_none() => mode = Some(Mode::Write),
            "--allow-dirty" => allow_dirty = true,
            value if !value.starts_with("--") && version.is_none() => version = Some(value),
            _ => return Err(usage()),
        }
    }
    let (Some(version), Some(mode)) = (version, mode) else {
        return Err(usage());
    };
    let parsed = Version::parse(version)
        .map_err(|error| format!("{version:?} is not a semantic version: {error}"))?;
    let range = release.compatibility_range()?;
    if !range.matches(&parsed) {
        return Err(format!(
            "{version} is outside compatibility.api {range} in {RELEASE_FILE}. Widen it there \
             first, deliberately: clients built for {range} would meet a {version} API"
        ));
    }
    let edits = edits(root, release, version)?;
    let changed: Vec<&Edit> = edits
        .iter()
        .filter(|edit| edit.before != edit.after)
        .collect();
    if changed.is_empty() {
        println!("[release] already at {version}: nothing to rewrite");
        return Ok(());
    }
    match mode {
        Mode::DryRun => {
            for edit in &changed {
                println!("[release] would rewrite {}", edit.path);
            }
            println!(
                "[release] dry run: {} file(s) would change from {} to {version}; nothing written",
                changed.len(),
                release.product.version
            );
            Ok(())
        }
        Mode::Write => {
            if !allow_dirty {
                let status = git(root, &["status", "--porcelain"])?;
                if !status.is_empty() {
                    return Err(format!(
                        "the worktree has uncommitted changes, so the version change could not \
                         be reviewed on its own:\n{status}\nCommit or stash them first, or pass \
                         --allow-dirty as the release maintainer"
                    ));
                }
            }
            for edit in &changed {
                fs::write(root.join(&edit.path), &edit.after)
                    .map_err(|error| format!("could not write {}: {error}", edit.path))?;
                println!("[release] rewrote {}", edit.path);
            }
            let findings = check::check(root, &Release::load(root)?);
            if findings.is_empty() {
                println!(
                    "[release] {version} everywhere and `release check` agrees; review and commit"
                );
                Ok(())
            } else {
                Err(report(&findings))
            }
        }
    }
}

fn edits(root: &Path, release: &Release, version: &str) -> Result<Vec<Edit>, String> {
    let old = release.product.version.as_str();
    let mut edits = Vec::new();
    let mut edit = |path: &str, rewrite: &dyn Fn(&str) -> Result<String, String>| {
        let before = fs::read_to_string(root.join(path))
            .map_err(|error| format!("{path}: cannot be read: {error}"))?;
        let after = rewrite(&before).map_err(|error| format!("{path}: {error}"))?;
        edits.push(Edit {
            path: path.to_string(),
            before,
            after,
        });
        Ok::<(), String>(())
    };
    edit(RELEASE_FILE, &|text| {
        toml_value(text, "product", "version", version)
    })?;
    edit(CARGO_MANIFEST, &|text| {
        toml_value(text, "workspace.package", "version", version)
    })?;
    let members = member_names(root)?;
    edit(CARGO_LOCK, &|text| lock_versions(text, &members, version))?;
    edit(PYTHON_VERSION_FILE, &|text| {
        assignment(text, "__version__ = ", &format!("\"{version}\""))
    })?;
    edit(DASHBOARD_PACKAGE, &|text| {
        json_value(text, None, 2, "version", version)
    })?;
    edit(DASHBOARD_LOCK, &|text| {
        let top = json_value(text, None, 2, "version", version)?;
        json_value(&top, Some("    \"\": {"), 6, "version", version)
    })?;
    edit(CHART, &|text| {
        let chart = assignment(text, "version: ", version)?;
        assignment(&chart, "appVersion: ", &format!("\"{version}\""))
    })?;
    edit(CHART_VALUES, &|text| image_tag(text, version))?;
    edit(OPENAPI, &|text| {
        json_value(text, Some("  \"info\": {"), 4, "version", version)
    })?;
    // Docs and examples that show a tagged image show the current one.
    let derived: BTreeSet<String> = edits.iter().map(|edit| edit.path.clone()).collect();
    let images: Vec<String> = release
        .images
        .components
        .values()
        .map(|component| release.image(component))
        .collect();
    for file in tracked_files(root)? {
        if historical(&file) || derived.contains(&file) {
            continue;
        }
        let Ok(before) = fs::read_to_string(root.join(&file)) else {
            continue;
        };
        let mut after = before.clone();
        for image in &images {
            after = retag(&after, image, old, version);
        }
        if after != before {
            edits.push(Edit {
                path: file,
                before,
                after,
            });
        }
    }
    Ok(edits)
}

/// The package names of the workspace members.
fn member_names(root: &Path) -> Result<BTreeSet<String>, String> {
    let manifest: toml::Table = fs::read_to_string(root.join(CARGO_MANIFEST))
        .map_err(|error| format!("{CARGO_MANIFEST}: {error}"))?
        .parse()
        .map_err(|error| format!("{CARGO_MANIFEST}: {error}"))?;
    let mut names = BTreeSet::new();
    for member in manifest["workspace"]["members"]
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(toml::Value::as_str)
    {
        let path = format!("{member}/Cargo.toml");
        let package: toml::Table = fs::read_to_string(root.join(&path))
            .map_err(|error| format!("{path}: {error}"))?
            .parse()
            .map_err(|error| format!("{path}: {error}"))?;
        if let Some(name) = package
            .get("package")
            .and_then(|package| package.get("name"))
            .and_then(toml::Value::as_str)
        {
            names.insert(name.to_string());
        }
    }
    Ok(names)
}

/// Replaces the quoted value of `key` in `[table]`.
fn toml_value(text: &str, table: &str, key: &str, value: &str) -> Result<String, String> {
    let mut current = String::new();
    let mut replaced = false;
    let mut output = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            current = trimmed.trim_matches(['[', ']']).trim().to_string();
        }
        let assigns = trimmed
            .strip_prefix(key)
            .is_some_and(|rest| rest.trim_start().starts_with('='));
        if !replaced && current == table && assigns {
            output.push_str(&replace_quoted(line, value)?);
            replaced = true;
        } else {
            output.push_str(line);
        }
    }
    if replaced {
        Ok(output)
    } else {
        Err(format!("no `{key} = \"...\"` in [{table}]"))
    }
}

/// Replaces the first double-quoted string on a line.
fn replace_quoted(line: &str, value: &str) -> Result<String, String> {
    let start = line
        .find('"')
        .ok_or_else(|| format!("no quoted value in {line:?}"))?;
    let length = line[start + 1..]
        .find('"')
        .ok_or_else(|| format!("unterminated value in {line:?}"))?;
    Ok(format!(
        "{}\"{value}\"{}",
        &line[..start],
        &line[start + 1 + length + 1..]
    ))
}

/// Sets the version of every workspace member in `Cargo.lock`, which records
/// them as `[[package]]` blocks with no `source`.
fn lock_versions(text: &str, members: &BTreeSet<String>, version: &str) -> Result<String, String> {
    let mut seen = BTreeSet::new();
    let blocks: Vec<String> = text
        .split("\n\n")
        .map(|block| {
            let name = block
                .lines()
                .find_map(|line| line.strip_prefix("name = \"")?.strip_suffix('"'));
            let member = block.starts_with("[[package]]")
                && name.is_some_and(|name| members.contains(name))
                && !block.contains("\nsource = ");
            if !member {
                return block.to_string();
            }
            seen.insert(name.unwrap_or_default().to_string());
            block
                .split('\n')
                .map(|line| {
                    if line.starts_with("version = \"") {
                        format!("version = \"{version}\"")
                    } else {
                        line.to_string()
                    }
                })
                .collect::<Vec<_>>()
                .join("\n")
        })
        .collect();
    let missing: Vec<&String> = members.difference(&seen).collect();
    if !missing.is_empty() {
        return Err(format!("no entry for workspace member(s) {missing:?}"));
    }
    Ok(blocks.join("\n\n"))
}

/// Replaces the rest of the first line that starts with `prefix`.
fn assignment(text: &str, prefix: &str, value: &str) -> Result<String, String> {
    let mut replaced = false;
    let mut output = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        if !replaced && line.starts_with(prefix) {
            let ending = if line.ends_with("\r\n") {
                "\r\n"
            } else if line.ends_with('\n') {
                "\n"
            } else {
                ""
            };
            output.push_str(prefix);
            output.push_str(value);
            output.push_str(ending);
            replaced = true;
        } else {
            output.push_str(line);
        }
    }
    if replaced {
        Ok(output)
    } else {
        Err(format!("no line starts with {prefix:?}"))
    }
}

/// Replaces the string value of `"key"` on the first line indented exactly
/// `indent` spaces, after the line `after` when one is given.
fn json_value(
    text: &str,
    after: Option<&str>,
    indent: usize,
    key: &str,
    value: &str,
) -> Result<String, String> {
    let prefix = format!("{}\"{key}\": \"", " ".repeat(indent));
    let mut armed = after.is_none();
    let mut replaced = false;
    let mut output = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        if !armed && after.is_some_and(|anchor| line.trim_end() == anchor) {
            armed = true;
        } else if armed && !replaced && line.starts_with(&prefix) {
            let rest = &line[prefix.len()..];
            let end = rest
                .find('"')
                .ok_or_else(|| format!("unterminated value in {line:?}"))?;
            output.push_str(&prefix);
            output.push_str(value);
            output.push_str(&rest[end..]);
            replaced = true;
            continue;
        }
        output.push_str(line);
    }
    if replaced {
        Ok(output)
    } else {
        Err(format!(
            "no {key:?} at indent {indent}{}",
            after
                .map(|anchor| format!(" after {anchor:?}"))
                .unwrap_or_default()
        ))
    }
}

/// Sets `image.tag` in the chart's values: the first `  tag:` inside the
/// top-level `image:` block.
fn image_tag(text: &str, version: &str) -> Result<String, String> {
    let mut inside = false;
    let mut replaced = false;
    let mut output = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        if line.trim_end() == "image:" {
            inside = true;
        } else if inside && !line.starts_with(' ') && !line.trim().is_empty() {
            inside = false;
        } else if inside && !replaced && line.starts_with("  tag: ") {
            output.push_str("  tag: \"");
            output.push_str(version);
            output.push_str("\"\n");
            replaced = true;
            continue;
        }
        output.push_str(line);
    }
    if replaced {
        Ok(output)
    } else {
        Err("no tag in the top-level image block".to_string())
    }
}

/// Replaces `image:old` with `image:new` where `old` is the whole tag.
fn retag(text: &str, image: &str, old: &str, new: &str) -> String {
    let needle = format!("{image}:{old}");
    let mut output = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(start) = rest.find(&needle) {
        let end = start + needle.len();
        let whole = rest[end..]
            .chars()
            .next()
            .is_none_or(|next| !(next.is_ascii_alphanumeric() || matches!(next, '.' | '-' | '+')));
        output.push_str(&rest[..start]);
        if whole {
            output.push_str(image);
            output.push(':');
            output.push_str(new);
        } else {
            output.push_str(&needle);
        }
        rest = &rest[end..];
    }
    output.push_str(rest);
    output
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use super::{assignment, image_tag, json_value, lock_versions, retag, toml_value};

    #[test]
    fn a_toml_value_changes_only_in_its_table() {
        let text = "[package]\nversion = \"1.0.0\"\n\n[product]\nname = \"x\"\nversion = \"0.1.0\" # kept\n";
        assert_eq!(
            toml_value(text, "product", "version", "0.2.0").unwrap(),
            "[package]\nversion = \"1.0.0\"\n\n[product]\nname = \"x\"\nversion = \"0.2.0\" # kept\n"
        );
        assert!(toml_value(text, "missing", "version", "0.2.0").is_err());
    }

    #[test]
    fn only_workspace_members_change_in_the_lock() {
        let text = "[[package]]\nname = \"capsulet-api\"\nversion = \"0.1.0\"\ndependencies = [\n \"serde\",\n]\n\n[[package]]\nname = \"serde\"\nversion = \"0.1.0\"\nsource = \"registry+https://github.com/rust-lang/crates.io-index\"\n";
        let members = BTreeSet::from(["capsulet-api".to_string()]);
        let updated = lock_versions(text, &members, "0.2.0").unwrap();
        assert!(updated.contains("name = \"capsulet-api\"\nversion = \"0.2.0\""));
        assert!(updated.contains("name = \"serde\"\nversion = \"0.1.0\""));
        let missing = BTreeSet::from(["capsulet-ghost".to_string()]);
        assert!(lock_versions(text, &missing, "0.2.0").is_err());
    }

    #[test]
    fn a_json_value_changes_at_its_place_only() {
        let text = "{\n  \"name\": \"d\",\n  \"version\": \"0.1.0\",\n  \"packages\": {\n    \"\": {\n      \"version\": \"0.1.0\"\n    },\n    \"node_modules/x\": {\n      \"version\": \"0.1.0\"\n    }\n  }\n}\n";
        let top = json_value(text, None, 2, "version", "0.2.0").unwrap();
        let both = json_value(&top, Some("    \"\": {"), 6, "version", "0.2.0").unwrap();
        assert_eq!(both.matches("\"0.2.0\"").count(), 2);
        assert!(both.contains("\"node_modules/x\": {\n      \"version\": \"0.1.0\""));
    }

    #[test]
    fn yaml_lines_keep_their_neighbours() {
        let chart = "apiVersion: v2\nversion: 0.1.0\nappVersion: \"0.1.0\"\n";
        let updated = assignment(
            &assignment(chart, "version: ", "0.2.0").unwrap(),
            "appVersion: ",
            "\"0.2.0\"",
        )
        .unwrap();
        assert_eq!(
            updated,
            "apiVersion: v2\nversion: 0.2.0\nappVersion: \"0.2.0\"\n"
        );
        let values = "image:\n  registry: ghcr.io\n  tag: \"0.1.0\"\n\npostgresql:\n  image:\n    tag: \"16\"\n";
        assert_eq!(
            image_tag(values, "0.2.0").unwrap(),
            "image:\n  registry: ghcr.io\n  tag: \"0.2.0\"\n\npostgresql:\n  image:\n    tag: \"16\"\n"
        );
    }

    #[test]
    fn a_tag_changes_only_when_it_is_the_whole_tag() {
        assert_eq!(
            retag(
                "pull ghcr.io/a/b:0.1.0 or ghcr.io/a/b:0.1.0-rc.1.",
                "ghcr.io/a/b",
                "0.1.0",
                "0.2.0"
            ),
            "pull ghcr.io/a/b:0.2.0 or ghcr.io/a/b:0.1.0-rc.1."
        );
    }
}
