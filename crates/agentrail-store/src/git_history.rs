use agentrail_core::error::{Error, Result};
use std::path::Path;
use std::process::Command;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Commit {
    pub hash: String,
    pub timestamp: String,
    pub subject: String,
}

/// Read first-parent commit history on the current branch of `repo_path`.
///
/// `since` is an optional git revision (e.g. "HEAD~50", "v1.0") to limit the
/// range; when None, reads from repo root. Returns commits in reverse-chronological
/// order (newest first), matching `git log`'s default.
///
/// Returns an empty Vec if `repo_path` is not a git repo or has no commits.
pub fn read_history(repo_path: &Path, since: Option<&str>) -> Result<Vec<Commit>> {
    if !is_git_repo(repo_path) {
        return Ok(Vec::new());
    }

    let range = since
        .map(|r| format!("{r}..HEAD"))
        .unwrap_or_else(|| "HEAD".to_string());

    // Format: hash\x1ftimestamp\x1fsubject\x1e
    // \x1f = unit separator, \x1e = record separator — neither appears in
    // commit subjects in practice, so no escaping needed.
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .arg("log")
        .arg("--first-parent")
        .arg("--format=%H%x1f%cI%x1f%s%x1e")
        .arg(&range)
        .output()
        .map_err(|e| Error::Other(format!("failed to run git log: {e}")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        // Empty repo or bad range — return empty rather than erroring.
        if stderr.contains("does not have any commits")
            || stderr.contains("unknown revision")
            || stderr.contains("ambiguous argument")
        {
            return Ok(Vec::new());
        }
        return Err(Error::Other(format!("git log failed: {stderr}")));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut commits = Vec::new();
    for record in stdout.split('\x1e') {
        let record = record.trim_start_matches('\n');
        if record.is_empty() {
            continue;
        }
        let mut parts = record.splitn(3, '\x1f');
        let hash = parts.next().unwrap_or("").to_string();
        let timestamp = parts.next().unwrap_or("").to_string();
        let subject = parts.next().unwrap_or("").to_string();
        if hash.is_empty() {
            continue;
        }
        commits.push(Commit {
            hash,
            timestamp,
            subject,
        });
    }

    Ok(commits)
}

/// Get the current HEAD commit hash, or None if not a git repo / no commits.
pub fn head_hash(repo_path: &Path) -> Option<String> {
    if !is_git_repo(repo_path) {
        return None;
    }
    let output = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .arg("rev-parse")
        .arg("HEAD")
        .output()
        .ok()?;
    if !output.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    if s.is_empty() { None } else { Some(s) }
}

/// Return the list of uncommitted paths (staged + unstaged + untracked) in
/// porcelain format. Empty when the tree is clean or not a git repo.
pub fn uncommitted_paths(repo_path: &Path) -> Vec<String> {
    if !is_git_repo(repo_path) {
        return Vec::new();
    }
    let output = match Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .arg("status")
        .arg("--porcelain")
        .output()
    {
        Ok(o) if o.status.success() => o,
        _ => return Vec::new(),
    };
    String::from_utf8_lossy(&output.stdout)
        .lines()
        .map(|l| l.to_string())
        .collect()
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;

    fn init_repo(path: &Path) {
        run(path, &["init", "-q", "-b", "main"]);
        run(path, &["config", "user.email", "t@t"]);
        run(path, &["config", "user.name", "t"]);
        run(path, &["config", "commit.gpgsign", "false"]);
    }

    fn run(path: &Path, args: &[&str]) {
        let ok = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .unwrap()
            .status
            .success();
        assert!(ok, "git {args:?} failed");
    }

    fn commit(path: &Path, file: &str, msg: &str) {
        std::fs::write(path.join(file), msg).unwrap();
        run(path, &["add", file]);
        run(path, &["commit", "-q", "-m", msg]);
    }

    #[test]
    fn empty_repo_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        assert_eq!(read_history(tmp.path(), None).unwrap(), Vec::new());
    }

    #[test]
    fn non_git_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        assert_eq!(read_history(tmp.path(), None).unwrap(), Vec::new());
        assert!(head_hash(tmp.path()).is_none());
    }

    #[test]
    fn reads_first_parent_history_newest_first() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        init_repo(p);
        commit(p, "a.txt", "first");
        commit(p, "b.txt", "second");
        commit(p, "c.txt", "third");

        let history = read_history(p, None).unwrap();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0].subject, "third");
        assert_eq!(history[1].subject, "second");
        assert_eq!(history[2].subject, "first");
        for c in &history {
            assert_eq!(c.hash.len(), 40);
            assert!(!c.timestamp.is_empty());
        }
    }

    #[test]
    fn head_hash_matches_latest_commit() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        init_repo(p);
        commit(p, "a.txt", "only");

        let head = head_hash(p).expect("head");
        let history = read_history(p, None).unwrap();
        assert_eq!(history[0].hash, head);
    }

    #[test]
    fn uncommitted_reports_untracked() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        init_repo(p);
        commit(p, "a.txt", "init");
        std::fs::write(p.join("dirty.txt"), "x").unwrap();
        let lines = uncommitted_paths(p);
        assert!(lines.iter().any(|l| l.contains("dirty.txt")));
    }
}
