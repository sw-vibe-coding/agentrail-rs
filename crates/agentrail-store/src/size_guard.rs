//! Size guard for the saga record.
//!
//! The saga record is committed at every step, so one large file under
//! `.agentrail/` is stored again in full each time it changes, and a file
//! past GitHub's 100 MiB limit makes the whole push fail — after the
//! commits holding it already exist. This module finds such files before
//! they are staged: `next`, `status` and `complete` print the report, the
//! `git add` lines agentrail suggests leave blocked files out, and
//! `agentrail check-size` exits non-zero so it can gate a pre-commit hook.

use std::path::Path;
use std::process::Command;

/// Files at or above this size get a warning.
pub const WARN_BYTES: u64 = 10 * 1024 * 1024;
/// Files at or above this size must not be committed. GitHub rejects
/// 100 MiB; the margin leaves room for a log that is still growing.
pub const BLOCK_BYTES: u64 = 95 * 1024 * 1024;

const GUARDED: [&str; 2] = [".agentrail", ".agentrail-archive"];

/// One file at or over [`WARN_BYTES`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Oversize {
    /// Path relative to the repository root.
    pub path: String,
    pub bytes: u64,
}

impl Oversize {
    pub fn blocked(&self) -> bool {
        self.bytes >= BLOCK_BYTES
    }
}

/// Which files to look at.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Scope {
    /// Everything git could commit: tracked files plus untracked files
    /// that are not ignored.
    Committable,
    /// Only what is staged in the index (for a pre-commit hook).
    Staged,
}

/// Files under `.agentrail/` and `.agentrail-archive/` at or over
/// [`WARN_BYTES`], largest first. Ignored files are skipped: git will
/// not commit them. Empty outside a git repository.
pub fn scan(repo_path: &Path, scope: Scope) -> Vec<Oversize> {
    scan_with(repo_path, scope, WARN_BYTES)
}

fn scan_with(repo_path: &Path, scope: Scope, warn_bytes: u64) -> Vec<Oversize> {
    let args: &[&str] = match scope {
        Scope::Committable => &[
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
        ],
        Scope::Staged => &[
            "diff",
            "--cached",
            "--name-only",
            "-z",
            "--diff-filter=ACMR",
            "--",
        ],
    };
    let Ok(out) = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(args)
        .args(GUARDED)
        .output()
    else {
        return Vec::new();
    };
    if !out.status.success() {
        return Vec::new();
    }
    let mut found: Vec<Oversize> = String::from_utf8_lossy(&out.stdout)
        .split('\0')
        .filter(|p| !p.is_empty())
        .filter_map(|p| {
            let bytes = repo_path.join(p).metadata().ok()?.len();
            (bytes >= warn_bytes).then(|| Oversize {
                path: p.to_string(),
                bytes,
            })
        })
        .collect();
    found.sort_by(|a, b| b.bytes.cmp(&a.bytes).then_with(|| a.path.cmp(&b.path)));
    found.dedup();
    found
}

/// Multi-line report of `found`, or `None` when it is empty.
pub fn format_report(found: &[Oversize]) -> Option<String> {
    if found.is_empty() {
        return None;
    }
    let mut msg = String::new();
    for f in found.iter().filter(|f| f.blocked()) {
        msg.push_str(&format!(
            "✖ {} is {} — over the {} limit. Do NOT commit it: GitHub rejects files of 100 MiB\n  and the push fails after the commit already holds it.\n",
            f.path,
            agentrail_core::human_size(f.bytes),
            agentrail_core::human_size(BLOCK_BYTES),
        ));
    }
    for f in found.iter().filter(|f| !f.blocked()) {
        msg.push_str(&format!(
            "⚠ {} is {} (over {}) — every commit that changes it stores it again in full.\n",
            f.path,
            agentrail_core::human_size(f.bytes),
            agentrail_core::human_size(WARN_BYTES),
        ));
    }
    msg.push_str(
        "  The saga record should stay small. Session logs belong outside the repository\n  (`agentrail sessions migrate`); keep other large artifacts out of .agentrail/.",
    );
    Some(msg)
}

/// Report for `next` / `status` / `complete`: everything git could commit.
pub fn warning(repo_path: &Path) -> Option<String> {
    format_report(&scan(repo_path, Scope::Committable))
}

/// The `git add` line agentrail suggests for the saga record, with every
/// blocked file excluded by pathspec so following the suggestion cannot
/// stage one.
pub fn git_add_line(repo_path: &Path) -> String {
    git_add_line_for(&scan(repo_path, Scope::Committable))
}

fn git_add_line_for(found: &[Oversize]) -> String {
    let mut line = String::from("git add .agentrail/ .agentrail-archive/");
    for f in found.iter().filter(|f| f.blocked()) {
        line.push_str(&format!(" ':(exclude){}'", f.path));
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn git(path: &Path, args: &[&str]) {
        let out = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?} failed");
    }

    fn repo_with(files: &[(&str, usize)]) -> tempfile::TempDir {
        let tmp = tempdir().unwrap();
        git(tmp.path(), &["init", "-q"]);
        git(tmp.path(), &["config", "user.email", "t@t"]);
        git(tmp.path(), &["config", "user.name", "t"]);
        for (path, len) in files {
            let p = tmp.path().join(path);
            std::fs::create_dir_all(p.parent().unwrap()).unwrap();
            std::fs::write(p, vec![b'x'; *len]).unwrap();
        }
        tmp
    }

    #[test]
    fn scan_is_empty_outside_git() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".agentrail")).unwrap();
        std::fs::write(tmp.path().join(".agentrail/big"), vec![b'x'; 200]).unwrap();
        assert!(scan_with(tmp.path(), Scope::Committable, 100).is_empty());
    }

    #[test]
    fn scan_finds_large_untracked_and_tracked_files_largest_first() {
        let tmp = repo_with(&[
            (".agentrail/saga.toml", 10),
            (".agentrail/big-untracked", 200),
            (".agentrail-archive/old/bigger-tracked", 300),
            ("src/huge-but-not-guarded", 5000),
        ]);
        git(tmp.path(), &["add", ".agentrail-archive"]);
        git(tmp.path(), &["commit", "-q", "-m", "x"]);

        let found = scan_with(tmp.path(), Scope::Committable, 100);
        let paths: Vec<&str> = found.iter().map(|f| f.path.as_str()).collect();
        assert_eq!(
            paths,
            vec![
                ".agentrail-archive/old/bigger-tracked",
                ".agentrail/big-untracked"
            ]
        );
        assert_eq!(found[0].bytes, 300);
    }

    #[test]
    fn scan_skips_ignored_files() {
        let tmp = repo_with(&[(".agentrail/sessions/log.jsonl", 200)]);
        std::fs::write(tmp.path().join(".gitignore"), "/.agentrail/sessions/\n").unwrap();
        assert!(scan_with(tmp.path(), Scope::Committable, 100).is_empty());
    }

    #[test]
    fn staged_scope_sees_only_the_index() {
        let tmp = repo_with(&[(".agentrail/staged", 200), (".agentrail/unstaged", 200)]);
        git(tmp.path(), &["add", ".agentrail/staged"]);
        let found = scan_with(tmp.path(), Scope::Staged, 100);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].path, ".agentrail/staged");
    }

    #[test]
    fn report_separates_blocked_from_warned() {
        assert!(format_report(&[]).is_none());
        let found = vec![
            Oversize {
                path: ".agentrail/sessions/a.jsonl".into(),
                bytes: 105 * 1024 * 1024,
            },
            Oversize {
                path: ".agentrail/steps/001-x/dump.bin".into(),
                bytes: 12 * 1024 * 1024,
            },
        ];
        assert!(found[0].blocked());
        assert!(!found[1].blocked());
        let msg = format_report(&found).unwrap();
        assert!(msg.contains("✖ .agentrail/sessions/a.jsonl is 105.0 MiB"));
        assert!(msg.contains("Do NOT commit"));
        assert!(msg.contains("⚠ .agentrail/steps/001-x/dump.bin is 12.0 MiB"));
    }

    #[test]
    fn git_add_line_excludes_only_blocked_files() {
        assert_eq!(
            git_add_line_for(&[]),
            "git add .agentrail/ .agentrail-archive/"
        );
        let found = vec![
            Oversize {
                path: ".agentrail/sessions/a.jsonl".into(),
                bytes: BLOCK_BYTES,
            },
            Oversize {
                path: ".agentrail/warned".into(),
                bytes: WARN_BYTES,
            },
        ];
        assert_eq!(
            git_add_line_for(&found),
            "git add .agentrail/ .agentrail-archive/ ':(exclude).agentrail/sessions/a.jsonl'"
        );
    }
}
