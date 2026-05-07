//! `.agentrail/` tracking enforcement.
//!
//! Detects the most common saga-data-loss footgun: agents leaving the
//! `.agentrail/` directory uncommitted (or worse, gitignored) after
//! finishing a step. Used by `agentrail next` (warn at session start)
//! and `agentrail complete` (remind after writing) so the binary itself
//! reinforces the rule that prose alone hasn't been enough to hold.

use std::path::Path;
use std::process::Command;

/// Per-call snapshot of `.agentrail/` (and `.agentrail-archive/`)
/// tracking state. `is_clean()` is the headline.
#[derive(Debug, Clone, Default)]
pub struct TrackingReport {
    /// `.agentrail/` (or a parent) appears in `.gitignore` — the worst
    /// case: the saga record is invisible to git entirely.
    pub gitignored: bool,
    /// Files git has never seen.
    pub untracked: Vec<String>,
    /// Tracked files with unstaged modifications.
    pub modified: Vec<String>,
    /// Files staged in the index but not yet committed.
    pub staged: Vec<String>,
}

impl TrackingReport {
    pub fn is_clean(&self) -> bool {
        !self.gitignored
            && self.untracked.is_empty()
            && self.modified.is_empty()
            && self.staged.is_empty()
    }

    /// "Will the next agentrail session see this work?" — gitignored or
    /// untracked files definitely will not. Modified-tracked files will
    /// still be present in the working tree but won't show up in
    /// `git log` output until committed.
    pub fn has_invisible_state(&self) -> bool {
        self.gitignored || !self.untracked.is_empty()
    }
}

/// Inspect the current state of `.agentrail/` in `repo_path`. Returns
/// `None` if `repo_path` is not inside a git repo (no rule to enforce).
pub fn check(repo_path: &Path) -> Option<TrackingReport> {
    if !is_git_repo(repo_path) {
        return None;
    }
    let gitignored = is_agentrail_gitignored(repo_path);

    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .arg("status")
        .arg("--porcelain")
        // Expand untracked DIRECTORIES into individual files. Without
        // this, `.agentrail/` shows up as a single line and we lose the
        // ability to list specific paths in the warning.
        .arg("--untracked-files=all")
        .arg("--")
        .arg(".agentrail")
        .arg(".agentrail-archive")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }

    let mut report = TrackingReport {
        gitignored,
        ..Default::default()
    };
    for line in String::from_utf8_lossy(&output.stdout).lines() {
        if line.len() < 4 {
            continue;
        }
        let xy = &line[..2];
        // Porcelain v1 prefixes the path with a space at byte 2.
        let path = line[3..].to_string();
        if xy == "??" {
            report.untracked.push(path);
            continue;
        }
        let mut chars = xy.chars();
        let x = chars.next().unwrap_or(' ');
        let y = chars.next().unwrap_or(' ');
        if x != ' ' && x != '?' {
            report.staged.push(path.clone());
        }
        if y != ' ' && y != '?' {
            report.modified.push(path);
        }
    }
    Some(report)
}

/// Format the tracking state as a multi-line warning suitable for
/// printing at the top of `agentrail next` output. Returns `None` when
/// the working tree is clean (or the repo isn't git-tracked).
pub fn warning(repo_path: &Path) -> Option<String> {
    let report = check(repo_path)?;
    if report.is_clean() {
        return None;
    }

    let mut msg = String::new();
    if report.gitignored {
        msg.push_str(
            "⚠ CRITICAL: .agentrail/ is gitignored — the saga record is invisible to git.\n  Remove the .agentrail/ exclusion from .gitignore, then `git add .agentrail/`.\n",
        );
    }
    if !report.untracked.is_empty() {
        msg.push_str(&format!(
            "⚠ .agentrail/ has {} untracked file(s) — the next session will not see this work:\n",
            report.untracked.len()
        ));
        list_paths(&mut msg, &report.untracked, 8);
    }
    if !report.modified.is_empty() {
        msg.push_str(&format!(
            "⚠ .agentrail/ has {} modified-but-uncommitted file(s):\n",
            report.modified.len()
        ));
        list_paths(&mut msg, &report.modified, 8);
    }
    if !report.staged.is_empty() && report.untracked.is_empty() && report.modified.is_empty() {
        // Only staged — less urgent (about to be committed) but still flag.
        msg.push_str(&format!(
            "ℹ .agentrail/ has {} staged-but-uncommitted file(s) — `git commit` to lock them in.\n",
            report.staged.len()
        ));
    }
    msg.push_str(
        "  Track and commit `.agentrail/` files at every step boundary — they ARE the saga record.\n  Quick fix:  git add .agentrail/ .agentrail-archive/ && git commit -m \"saga: <summary>\"",
    );
    Some(msg)
}

/// Reminder printed at the END of `agentrail complete`, after the
/// command has just written `step.toml` / `summary.md` / a fresh next
/// step. Always emits if there's any non-clean state, because complete
/// reliably leaves the working tree dirty in `.agentrail/`.
pub fn complete_reminder(repo_path: &Path) -> Option<String> {
    let report = check(repo_path)?;
    if report.is_clean() {
        return None;
    }
    let total = report.untracked.len() + report.modified.len() + report.staged.len();
    let mut msg = format!(
        "Reminder: this `complete` left {} file(s) uncommitted in .agentrail/.",
        total
    );
    if report.gitignored {
        msg.push_str(
            "\n  ⚠ .agentrail/ is gitignored — fix this FIRST or the saga record will be lost.",
        );
    }
    msg.push_str(
        "\n  Commit them now so the next session sees this step:\n    git add .agentrail/ .agentrail-archive/\n    git commit -m \"step <slug>: <summary>\"\n  Without this, `agentrail next` in a future session will run on stale state.",
    );
    Some(msg)
}

fn list_paths(msg: &mut String, paths: &[String], limit: usize) {
    for p in paths.iter().take(limit) {
        msg.push_str("    ");
        msg.push_str(p);
        msg.push('\n');
    }
    if paths.len() > limit {
        msg.push_str(&format!("    ... and {} more\n", paths.len() - limit));
    }
}

fn is_git_repo(path: &Path) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("rev-parse")
        .arg("--git-dir")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn is_agentrail_gitignored(path: &Path) -> bool {
    // `git check-ignore` exits 0 when the path is ignored, 1 when not, 128 on error.
    Command::new("git")
        .arg("-C")
        .arg(path)
        .arg("check-ignore")
        .arg("-q")
        .arg(".agentrail/")
        .output()
        .map(|o| o.status.code() == Some(0))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn init_git(path: &Path) {
        Command::new("git")
            .arg("-C")
            .arg(path)
            .arg("init")
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(path)
            .arg("config")
            .arg("user.email")
            .arg("t@t")
            .output()
            .unwrap();
        Command::new("git")
            .arg("-C")
            .arg(path)
            .arg("config")
            .arg("user.name")
            .arg("t")
            .output()
            .unwrap();
    }

    #[test]
    fn check_returns_none_outside_git_repo() {
        let tmp = tempdir().unwrap();
        assert!(check(tmp.path()).is_none());
    }

    #[test]
    fn check_clean_repo_with_no_agentrail_is_clean() {
        let tmp = tempdir().unwrap();
        init_git(tmp.path());
        let report = check(tmp.path()).unwrap();
        assert!(report.is_clean());
        assert!(warning(tmp.path()).is_none());
    }

    #[test]
    fn check_detects_untracked_agentrail_files() {
        let tmp = tempdir().unwrap();
        init_git(tmp.path());
        std::fs::create_dir_all(tmp.path().join(".agentrail/steps/001-foo")).unwrap();
        std::fs::write(tmp.path().join(".agentrail/saga.toml"), "name = \"x\"\n").unwrap();
        std::fs::write(
            tmp.path().join(".agentrail/steps/001-foo/step.toml"),
            "x",
        )
        .unwrap();

        let report = check(tmp.path()).unwrap();
        assert!(!report.is_clean());
        assert!(report.has_invisible_state());
        assert_eq!(report.untracked.len(), 2);
        assert!(!report.gitignored);
    }

    #[test]
    fn check_detects_gitignored_agentrail() {
        let tmp = tempdir().unwrap();
        init_git(tmp.path());
        std::fs::write(tmp.path().join(".gitignore"), ".agentrail/\n").unwrap();
        std::fs::create_dir_all(tmp.path().join(".agentrail")).unwrap();
        std::fs::write(tmp.path().join(".agentrail/saga.toml"), "x").unwrap();

        let report = check(tmp.path()).unwrap();
        assert!(report.gitignored, "should detect .agentrail/ in .gitignore");
        let warn = warning(tmp.path()).unwrap();
        assert!(warn.contains("CRITICAL"));
        assert!(warn.contains(".gitignore"));
    }

    #[test]
    fn warning_lists_untracked_files() {
        let tmp = tempdir().unwrap();
        init_git(tmp.path());
        std::fs::create_dir_all(tmp.path().join(".agentrail")).unwrap();
        std::fs::write(tmp.path().join(".agentrail/saga.toml"), "x").unwrap();
        std::fs::write(tmp.path().join(".agentrail/plan.md"), "x").unwrap();

        let warn = warning(tmp.path()).unwrap();
        assert!(warn.contains("untracked file"));
        assert!(warn.contains("saga.toml"));
        assert!(warn.contains("plan.md"));
        assert!(warn.contains("git add"));
    }

    #[test]
    fn complete_reminder_emits_when_dirty_silent_when_clean() {
        let tmp = tempdir().unwrap();
        init_git(tmp.path());
        // Clean repo → no reminder.
        assert!(complete_reminder(tmp.path()).is_none());

        // Dirty repo → reminder.
        std::fs::create_dir_all(tmp.path().join(".agentrail")).unwrap();
        std::fs::write(tmp.path().join(".agentrail/saga.toml"), "x").unwrap();
        let r = complete_reminder(tmp.path()).unwrap();
        assert!(r.contains("Reminder"));
        assert!(r.contains("git add"));
    }
}
