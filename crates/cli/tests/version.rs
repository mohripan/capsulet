use std::{path::Path, process::Command};

/// `product.version` in the repository's `release.toml`, read from the file so
/// a CLI built from a crate that drifted from the release fails here.
fn release_version() -> String {
    let release =
        std::fs::read_to_string(Path::new(env!("CARGO_MANIFEST_DIR")).join("../../release.toml"))
            .expect("release.toml");
    let mut table = "";
    for line in release.lines().map(str::trim) {
        if line.starts_with('[') {
            table = line;
        } else if table == "[product]"
            && let Some(value) = line.strip_prefix("version = \"")
        {
            return value.trim_end_matches('"').to_string();
        }
    }
    panic!("release.toml has no product.version");
}

#[test]
fn the_cli_reports_the_release_version() {
    let output = Command::new(env!("CARGO_BIN_EXE_capsulet"))
        .arg("--version")
        .output()
        .expect("the capsulet binary runs");
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        format!("capsulet {}", release_version())
    );
}
