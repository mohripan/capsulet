//! What the checkout looked like when a run started, so the end of the run can
//! say whether anything the gates did changed it.
//!
//! A snapshot is the commit checked out plus, for every path `git status`
//! reports (modified, staged, deleted or untracked, never ignored), its status
//! and the SHA-256 of its contents. Comparing two snapshots finds a file a gate
//! wrote, rewrote or deleted, including one that was already modified before
//! the run, which `git status` alone would report the same way both times.

use std::{collections::BTreeMap, fs, path::Path, process::Command};

use sha2::{Digest, Sha256};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Snapshot {
    /// `None` when the checkout is not a Git repository or Git is missing.
    pub(crate) revision: Option<String>,
    entries: BTreeMap<String, String>,
}

impl Snapshot {
    pub(crate) fn take(root: &Path) -> Self {
        let revision = git(root, &["rev-parse", "HEAD"]).map(|out| out.trim().to_string());
        let Some(status) = git(
            root,
            &["status", "--porcelain=v1", "-z", "--untracked-files=all"],
        ) else {
            return Self {
                revision,
                entries: BTreeMap::new(),
            };
        };
        let mut entries = BTreeMap::new();
        let mut fields = status.split('\0').filter(|field| !field.is_empty());
        while let Some(field) = fields.next() {
            let (code, path) = field.split_at(field.len().min(3));
            let code = code.trim_end().to_string();
            // A rename or copy names its source in the next field.
            if code.contains(['R', 'C']) {
                fields.next();
            }
            let contents = fs::read(root.join(path)).map_or_else(
                |_| "absent".to_string(),
                |bytes| hex(&Sha256::digest(&bytes)),
            );
            entries.insert(path.to_string(), format!("{code} {contents}"));
        }
        Self { revision, entries }
    }

    /// Whether `git status` reported nothing at all.
    pub(crate) fn is_clean(&self) -> bool {
        self.revision.is_some() && self.entries.is_empty()
    }

    /// Every path whose status or contents differ between the two snapshots,
    /// and a moved `HEAD`.
    pub(crate) fn changes_since(&self, before: &Self) -> Vec<String> {
        let mut changes = Vec::new();
        if self.revision != before.revision {
            changes.push(format!(
                "HEAD moved from {} to {}",
                before.revision.as_deref().unwrap_or("nothing"),
                self.revision.as_deref().unwrap_or("nothing")
            ));
        }
        let paths: std::collections::BTreeSet<&String> =
            before.entries.keys().chain(self.entries.keys()).collect();
        for path in paths {
            if before.entries.get(path) != self.entries.get(path) {
                changes.push(path.clone());
            }
        }
        changes
    }
}

fn git(root: &Path, arguments: &[&str]) -> Option<String> {
    let output = Command::new("git")
        .arg("-C")
        .arg(root)
        .args(arguments)
        .output()
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn hex(bytes: &[u8]) -> String {
    use std::fmt::Write as _;
    bytes
        .iter()
        .fold(String::with_capacity(64), |mut text, byte| {
            let _ = write!(text, "{byte:02x}");
            text
        })
}

#[cfg(test)]
mod tests {
    use std::{fs, path::Path, process::Command};

    use super::Snapshot;

    fn git(root: &Path, arguments: &[&str]) {
        let status = Command::new("git")
            .arg("-C")
            .arg(root)
            .args(arguments)
            .status()
            .expect("git runs");
        assert!(status.success(), "git {arguments:?}");
    }

    fn repository() -> tempfile::TempDir {
        let directory = tempfile::tempdir().expect("temp directory");
        let root = directory.path();
        git(root, &["init", "--quiet"]);
        git(root, &["config", "user.email", "verify@example.invalid"]);
        git(root, &["config", "user.name", "verify"]);
        git(root, &["config", "core.autocrlf", "false"]);
        fs::write(root.join(".gitignore"), "ignored/\n").expect("write");
        fs::write(root.join("tracked.txt"), "one\n").expect("write");
        git(root, &["add", "."]);
        git(root, &["commit", "--quiet", "-m", "start"]);
        directory
    }

    #[test]
    fn a_clean_checkout_stays_clean_when_only_ignored_files_change() {
        let directory = repository();
        let root = directory.path();
        let before = Snapshot::take(root);
        assert!(before.is_clean());
        fs::create_dir_all(root.join("ignored")).expect("directory");
        fs::write(root.join("ignored/log"), "evidence").expect("write");
        assert_eq!(
            Snapshot::take(root).changes_since(&before),
            [] as [std::string::String; 0]
        );
    }

    #[test]
    fn a_file_written_or_rewritten_during_the_run_is_found() {
        let directory = repository();
        let root = directory.path();
        // Modified before the run: `git status` says ` M` before and after.
        fs::write(root.join("tracked.txt"), "two\n").expect("write");
        let before = Snapshot::take(root);
        assert!(!before.is_clean());
        fs::write(root.join("tracked.txt"), "three\n").expect("write");
        fs::write(root.join("new.txt"), "stray").expect("write");
        assert_eq!(
            Snapshot::take(root).changes_since(&before),
            vec!["new.txt".to_string(), "tracked.txt".to_string()]
        );
    }

    #[test]
    fn a_moved_head_is_a_change() {
        let directory = repository();
        let root = directory.path();
        let before = Snapshot::take(root);
        git(
            root,
            &["commit", "--quiet", "--allow-empty", "-m", "during"],
        );
        let changes = Snapshot::take(root).changes_since(&before);
        assert_eq!(changes.len(), 1);
        assert!(changes[0].starts_with("HEAD moved from "), "{changes:?}");
    }
}
