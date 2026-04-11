//! Snapshot helper: save a point-in-time copy of `.agentrail/` and
//! `.agentrail-archive/` into the repo's git object store as a
//! proper commit under a dedicated ref namespace, without touching
//! the user's working index.
//!
//! The trick: clap's `GIT_INDEX_FILE` environment variable lets us run
//! `git add` / `git write-tree` against a throwaway index file. The user's
//! real `.git/index` is untouched — no staged-file surprises, no races
//! with pre-commit hooks. We then `commit-tree` + `update-ref` under
//! `refs/agentrail/snapshots/<timestamp>` so the blobs are reachable and
//! survive `git gc`.
//!
//! Restore is left to the user via normal git commands; this module does
//! not write to the working tree.

use agentrail_core::error::{Error, Result};
use std::path::Path;
use std::process::Command;

const REF_PREFIX: &str = "refs/agentrail/snapshots/";
/// Relative paths that the snapshot should capture. Missing paths are
/// silently skipped so a repo with only `.agentrail/` (no archive) works.
const CAPTURE_PATHS: &[&str] = &[".agentrail", ".agentrail-archive"];

#[derive(Debug, Clone)]
pub struct SnapshotInfo {
    pub ref_name: String,
    pub commit: String,
    pub captured_paths: Vec<String>,
}

#[derive(Debug, Clone)]
pub struct SnapshotEntry {
    pub ref_name: String,
    pub commit: String,
    /// The timestamp portion of the ref name (everything after REF_PREFIX).
    pub timestamp: String,
}

/// Take a snapshot of `.agentrail/` (and `.agentrail-archive/` if it
/// exists) into `refs/agentrail/snapshots/<timestamp>`.
///
/// Returns `Error::Other("no git repository")` if `repo_path` is not a
/// git repo, and `Error::Other("nothing to snapshot")` if neither capture
/// path exists.
pub fn take(repo_path: &Path) -> Result<SnapshotInfo> {
    ensure_git_repo(repo_path)?;

    let captured: Vec<&str> = CAPTURE_PATHS
        .iter()
        .copied()
        .filter(|p| repo_path.join(p).exists())
        .collect();
    if captured.is_empty() {
        return Err(Error::Other(
            "nothing to snapshot: no .agentrail/ or .agentrail-archive/ directory found".into(),
        ));
    }

    // Use a throwaway index file so the user's real index is never touched.
    let tmp = tempfile::Builder::new()
        .prefix("agentrail-snapshot-index-")
        .tempfile()
        .map_err(|e| Error::Other(format!("failed to create temp index: {e}")))?;
    // We want the path, not an open handle; git will create/overwrite the
    // file itself. Close the tempfile but keep the path alive via NamedTempFile.
    let index_path = tmp.path().to_path_buf();
    // Remove the empty file so `git read-tree --empty` creates a fresh one;
    // some git versions refuse to overwrite a zero-byte non-index file.
    std::fs::remove_file(&index_path).ok();

    let env = [("GIT_INDEX_FILE", index_path.to_string_lossy().to_string())];

    // 1. Initialize an empty index.
    run_git_env(repo_path, &env, &["read-tree", "--empty"])?;

    // 2. Stage the capture paths into the temp index. Force-add so that
    //    `.gitignore` entries inside .agentrail/ (unlikely, but possible)
    //    do not silently drop files.
    let mut add_args: Vec<&str> = vec!["add", "--force", "--"];
    add_args.extend(captured.iter().copied());
    run_git_env(repo_path, &env, &add_args)?;

    // 3. Write the tree.
    let tree = run_git_env_output(repo_path, &env, &["write-tree"])?;
    let tree = tree.trim().to_string();

    // 4. Create a commit with no parent. We deliberately do not chain
    //    snapshots as parents — each snapshot is an independent record.
    let ts = agentrail_core::timestamp();
    let msg = format!(
        "agentrail snapshot {ts}\n\nPaths captured: {}\n",
        captured.join(", ")
    );
    let commit = run_git_output(repo_path, &["commit-tree", &tree, "-m", &msg])?;
    let commit = commit.trim().to_string();

    // 5. Point a ref at the commit so git won't GC it.
    let ref_name = format!("{REF_PREFIX}{ts}");
    run_git(repo_path, &["update-ref", &ref_name, &commit])?;

    Ok(SnapshotInfo {
        ref_name,
        commit,
        captured_paths: captured.iter().map(|s| s.to_string()).collect(),
    })
}

/// List existing agentrail snapshot refs, newest first.
pub fn list(repo_path: &Path) -> Result<Vec<SnapshotEntry>> {
    ensure_git_repo(repo_path)?;
    let output = run_git_output(
        repo_path,
        &[
            "for-each-ref",
            "--sort=-refname",
            "--format=%(refname) %(objectname)",
            REF_PREFIX,
        ],
    )?;
    let mut entries = Vec::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        let mut parts = line.splitn(2, ' ');
        let ref_name = parts.next().unwrap_or("").to_string();
        let commit = parts.next().unwrap_or("").to_string();
        if ref_name.is_empty() || commit.is_empty() {
            continue;
        }
        let timestamp = ref_name
            .strip_prefix(REF_PREFIX)
            .unwrap_or(&ref_name)
            .to_string();
        entries.push(SnapshotEntry {
            ref_name,
            commit,
            timestamp,
        });
    }
    Ok(entries)
}

fn ensure_git_repo(repo_path: &Path) -> Result<()> {
    let ok = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(["rev-parse", "--git-dir"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false);
    if !ok {
        return Err(Error::Other(format!(
            "{} is not inside a git repository",
            repo_path.display()
        )));
    }
    Ok(())
}

fn run_git(repo_path: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(args)
        .status()
        .map_err(|e| Error::Other(format!("failed to spawn git: {e}")))?;
    if !status.success() {
        return Err(Error::Other(format!("git {:?} failed", args)));
    }
    Ok(())
}

fn run_git_output(repo_path: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo_path)
        .args(args)
        .output()
        .map_err(|e| Error::Other(format!("failed to spawn git: {e}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(Error::Other(format!("git {:?} failed: {}", args, err)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

fn run_git_env(repo_path: &Path, env: &[(&str, String)], args: &[&str]) -> Result<()> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo_path).args(args);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let status = cmd
        .status()
        .map_err(|e| Error::Other(format!("failed to spawn git: {e}")))?;
    if !status.success() {
        return Err(Error::Other(format!("git {:?} failed", args)));
    }
    Ok(())
}

fn run_git_env_output(repo_path: &Path, env: &[(&str, String)], args: &[&str]) -> Result<String> {
    let mut cmd = Command::new("git");
    cmd.arg("-C").arg(repo_path).args(args);
    for (k, v) in env {
        cmd.env(k, v);
    }
    let out = cmd
        .output()
        .map_err(|e| Error::Other(format!("failed to spawn git: {e}")))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(Error::Other(format!("git {:?} failed: {}", args, err)));
    }
    Ok(String::from_utf8_lossy(&out.stdout).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saga;

    fn init_repo(path: &Path) {
        run_git(path, &["init", "-q", "-b", "main"]).unwrap();
        run_git(path, &["config", "user.email", "t@t"]).unwrap();
        run_git(path, &["config", "user.name", "t"]).unwrap();
        run_git(path, &["config", "commit.gpgsign", "false"]).unwrap();
    }

    #[test]
    fn snapshot_requires_git_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let err = take(tmp.path()).unwrap_err();
        assert!(err.to_string().contains("not inside a git repository"));
    }

    #[test]
    fn snapshot_requires_agentrail_dir() {
        let tmp = tempfile::tempdir().unwrap();
        init_repo(tmp.path());
        let err = take(tmp.path()).unwrap_err();
        assert!(err.to_string().contains("nothing to snapshot"));
    }

    #[test]
    fn snapshot_creates_ref_and_preserves_working_state() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        init_repo(p);

        // Need an initial commit so HEAD exists and the real index has a
        // baseline we can check.
        std::fs::write(p.join("readme.md"), "hi").unwrap();
        run_git(p, &["add", "readme.md"]).unwrap();
        run_git(p, &["commit", "-q", "-m", "init"]).unwrap();

        saga::init_saga(p, "test", "# plan").unwrap();

        // Stage something in the real index to prove the snapshot does not
        // disturb it.
        std::fs::write(p.join("staged.txt"), "staged content").unwrap();
        run_git(p, &["add", "staged.txt"]).unwrap();
        let real_index_before = run_git_output(p, &["ls-files", "--stage"]).unwrap();

        let info = take(p).unwrap();
        assert!(info.ref_name.starts_with(REF_PREFIX));
        assert!(info.captured_paths.iter().any(|s| s == ".agentrail"));

        // Real index must be untouched.
        let real_index_after = run_git_output(p, &["ls-files", "--stage"]).unwrap();
        assert_eq!(real_index_before, real_index_after);

        // The snapshot ref must resolve to a commit whose tree contains
        // .agentrail/saga.toml.
        let tree_listing =
            run_git_output(p, &["ls-tree", "-r", "--name-only", &info.ref_name]).unwrap();
        assert!(tree_listing.contains(".agentrail/saga.toml"));
    }

    #[test]
    fn list_returns_snapshot_entries() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        init_repo(p);
        std::fs::write(p.join("readme.md"), "hi").unwrap();
        run_git(p, &["add", "readme.md"]).unwrap();
        run_git(p, &["commit", "-q", "-m", "init"]).unwrap();
        saga::init_saga(p, "test", "# plan").unwrap();

        assert!(list(p).unwrap().is_empty());

        let info = take(p).unwrap();
        let entries = list(p).unwrap();
        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].ref_name, info.ref_name);
        assert_eq!(entries[0].commit, info.commit);
    }
}
