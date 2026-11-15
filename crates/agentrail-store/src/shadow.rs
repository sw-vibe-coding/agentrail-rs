//! Shadow store: a private place for the session logs.
//!
//! Session logs are kept outside the repository (see [`crate::session`])
//! on one machine. The shadow store is where they go to be kept and
//! shared privately: a directory, usually a clone of a private git
//! repository. `push` writes each log there xz-compressed with a SHA-256
//! sidecar and, if the directory is a git repository, commits and
//! pushes; `pull` restores them.
//!
//! Layout, compatible with the script this replaced:
//!
//! ```text
//! <target>/sessions/<session-id>.jsonl.xz
//! <target>/sessions/<session-id>.jsonl.sha256   (hash of the uncompressed log)
//! ```
//!
//! The target is machine-local configuration (where a clone happens to
//! live), so it is stored beside the logs, not in `.agentrail/`: it
//! survives `agentrail archive` and a private repository's name never
//! lands in a public one.

use agentrail_core::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use crate::digest::sha256_file;

/// Environment variable naming the shadow target; overrides the stored one.
pub const SHADOW_ENV: &str = "AGENTRAIL_SHADOW";
const CONFIG_FILE: &str = "shadow.toml";
const SESSIONS: &str = "sessions";

#[derive(Debug, Default, Serialize, Deserialize)]
struct Config {
    target: Option<PathBuf>,
}

/// The shadow target for the repository whose session store is
/// `store_dir`: `$AGENTRAIL_SHADOW`, else what `set_target` stored.
pub fn configured_target(store_dir: &Path) -> Option<PathBuf> {
    if let Some(dir) = std::env::var_os(SHADOW_ENV)
        && !dir.is_empty()
    {
        return Some(PathBuf::from(dir));
    }
    stored_target(store_dir)
}

/// The target `set_target` stored, ignoring the environment.
pub fn stored_target(store_dir: &Path) -> Option<PathBuf> {
    let text = std::fs::read_to_string(store_dir.join(CONFIG_FILE)).ok()?;
    toml::from_str::<Config>(&text).ok()?.target
}

/// Remember `target` (made absolute) as this repository's shadow store.
/// The directory must exist: a typo should fail here, not at push time.
pub fn set_target(store_dir: &Path, target: &Path) -> Result<PathBuf> {
    let abs = target.canonicalize().map_err(|e| {
        Error::Other(format!(
            "shadow target {} is not an existing directory: {e}",
            target.display()
        ))
    })?;
    if !abs.is_dir() {
        return Err(Error::Other(format!(
            "shadow target {} is not a directory",
            abs.display()
        )));
    }
    std::fs::create_dir_all(store_dir)?;
    let text = toml::to_string_pretty(&Config {
        target: Some(abs.clone()),
    })?;
    std::fs::write(store_dir.join(CONFIG_FILE), text)?;
    Ok(abs)
}

/// Forget the stored target. Returns whether there was one.
pub fn clear_target(store_dir: &Path) -> Result<bool> {
    let path = store_dir.join(CONFIG_FILE);
    if path.is_file() {
        std::fs::remove_file(path)?;
        return Ok(true);
    }
    Ok(false)
}

/// Whether the `xz` program is available.
pub fn xz_available() -> bool {
    Command::new("xz")
        .arg("--version")
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

fn require_xz() -> Result<()> {
    if xz_available() {
        Ok(())
    } else {
        Err(Error::Other(
            "the `xz` program is needed to compress session logs and was not found on PATH".into(),
        ))
    }
}

/// Session log names (`<id>.jsonl`) in a directory, sorted.
fn names_with_suffix(dir: &Path, suffix: &str) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(dir)
        .map(|entries| {
            entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_file())
                .filter_map(|e| e.file_name().into_string().ok())
                .filter(|n| n.ends_with(suffix))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
}

fn sidecar_hash(shadow_sessions: &Path, log_name: &str) -> Option<String> {
    let text = std::fs::read_to_string(shadow_sessions.join(sidecar_name(log_name))).ok()?;
    Some(text.split_whitespace().next()?.to_string())
}

fn sidecar_name(log_name: &str) -> String {
    format!("{}.sha256", log_name)
}

fn xz_name(log_name: &str) -> String {
    format!("{log_name}.xz")
}

/// How one log stands between the local store and the shadow store.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// The shadow copy is this exact log.
    InSync,
    /// Both exist and differ (or the shadow copy has no recorded hash):
    /// `push` would update the shadow copy.
    Differs,
    /// Only in the local store: `push` would add it.
    LocalOnly,
    /// Only in the shadow store: `pull` would restore it.
    ShadowOnly,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Entry {
    /// `<session-id>.jsonl`
    pub name: String,
    pub state: State,
}

/// Compare the local store with the shadow store, log by log.
pub fn status(store_dir: &Path, target: &Path) -> Result<Vec<Entry>> {
    let shadow_sessions = target.join(SESSIONS);
    let local = names_with_suffix(store_dir, ".jsonl");
    let mut entries = Vec::new();
    for name in &local {
        let state = if !shadow_sessions.join(xz_name(name)).is_file() {
            State::LocalOnly
        } else if sidecar_hash(&shadow_sessions, name).as_deref()
            == Some(sha256_file(&store_dir.join(name))?.as_str())
        {
            State::InSync
        } else {
            State::Differs
        };
        entries.push(Entry {
            name: name.clone(),
            state,
        });
    }
    for xz in names_with_suffix(&shadow_sessions, ".jsonl.xz") {
        let name = xz.trim_end_matches(".xz").to_string();
        if !local.contains(&name) {
            entries.push(Entry {
                name,
                state: State::ShadowOnly,
            });
        }
    }
    entries.sort_by(|a, b| a.name.cmp(&b.name));
    Ok(entries)
}

#[derive(Debug, Default)]
pub struct PushReport {
    /// Logs compressed into the shadow store this run.
    pub written: Vec<String>,
    /// Logs whose shadow copy was already current.
    pub unchanged: Vec<String>,
    /// Short hash of the commit made in the shadow repository, if any.
    pub committed: Option<String>,
    /// Whether `git push` ran and succeeded.
    pub pushed: bool,
    /// Why the commit could not be pushed, when it could not.
    pub push_error: Option<String>,
}

/// Compress every local log that the shadow store lacks or holds an
/// older copy of. If the target is a git repository, commit the result
/// and (when `git_push` and a remote exists) push it.
pub fn push(store_dir: &Path, target: &Path, git_push: bool) -> Result<PushReport> {
    if !target.is_dir() {
        return Err(Error::Other(format!(
            "shadow target {} does not exist",
            target.display()
        )));
    }
    require_xz()?;
    let shadow_sessions = target.join(SESSIONS);
    std::fs::create_dir_all(&shadow_sessions)?;

    let mut report = PushReport::default();
    for name in names_with_suffix(store_dir, ".jsonl") {
        let log = store_dir.join(&name);
        let hash = sha256_file(&log)?;
        let dest = shadow_sessions.join(xz_name(&name));
        if dest.is_file() && sidecar_hash(&shadow_sessions, &name).as_deref() == Some(&hash) {
            report.unchanged.push(name);
            continue;
        }
        // Compress to a temp file and rename, so an interrupted run never
        // leaves a truncated archive under the real name.
        let tmp = shadow_sessions.join(format!("{name}.xz.tmp"));
        let out = std::fs::File::create(&tmp)?;
        let status = Command::new("xz")
            .args(["-T0", "-9", "-c"])
            .arg(&log)
            .stdout(out)
            .status()?;
        if !status.success() {
            std::fs::remove_file(&tmp).ok();
            return Err(Error::Other(format!("xz failed compressing {name}")));
        }
        std::fs::rename(&tmp, &dest)?;
        std::fs::write(
            shadow_sessions.join(sidecar_name(&name)),
            format!("{hash}  {name}\n"),
        )?;
        report.written.push(name);
    }

    if target.join(".git").exists() {
        git(target, &["add", "--", SESSIONS])?;
        let staged = !Command::new("git")
            .arg("-C")
            .arg(target)
            .args(["diff", "--cached", "--quiet", "--", SESSIONS])
            .status()?
            .success();
        if staged {
            let msg = format!("sessions: {}", chrono::Utc::now().format("%Y-%m-%dT%H:%MZ"));
            git(target, &["commit", "-q", "-m", &msg, "--", SESSIONS])?;
            report.committed = Some(git(target, &["rev-parse", "--short", "HEAD"])?);
        }
        if git_push && has_remote(target) {
            match git(target, &["push", "-q"]) {
                Ok(_) => report.pushed = true,
                Err(e) => report.push_error = Some(e.to_string()),
            }
        }
    }
    Ok(report)
}

#[derive(Debug, Default)]
pub struct PullReport {
    /// Logs restored into the local store.
    pub restored: Vec<String>,
    /// Logs left alone: the local copy is at least as long.
    pub kept_local: Vec<String>,
    /// Logs that could not be restored, with the reason.
    pub failed: Vec<(String, String)>,
    /// Why `git pull` in the shadow repository failed, when it did. The
    /// restore still runs from what the clone already has.
    pub pull_error: Option<String>,
}

/// Restore logs from the shadow store into the local store. A local copy
/// that is at least as long is never replaced: logs only grow, so it
/// already contains the shadow copy. Each restored log is checked
/// against its recorded SHA-256 when the shadow store has one.
pub fn pull(store_dir: &Path, target: &Path, git_pull: bool) -> Result<PullReport> {
    if !target.is_dir() {
        return Err(Error::Other(format!(
            "shadow target {} does not exist",
            target.display()
        )));
    }
    require_xz()?;
    let mut report = PullReport::default();
    if git_pull
        && target.join(".git").exists()
        && has_remote(target)
        && let Err(e) = git(target, &["pull", "-q", "--ff-only"])
    {
        report.pull_error = Some(e.to_string());
    }

    let shadow_sessions = target.join(SESSIONS);
    std::fs::create_dir_all(store_dir)?;
    for xz in names_with_suffix(&shadow_sessions, ".jsonl.xz") {
        let name = xz.trim_end_matches(".xz").to_string();
        let tmp = store_dir.join(format!("{name}.pull.tmp"));
        let out = std::fs::File::create(&tmp)?;
        let ok = Command::new("xz")
            .args(["-d", "-c"])
            .arg(shadow_sessions.join(&xz))
            .stdout(out)
            .stderr(Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        if !ok {
            std::fs::remove_file(&tmp).ok();
            report
                .failed
                .push((name, "xz could not decompress it".into()));
            continue;
        }
        if let Some(expected) = sidecar_hash(&shadow_sessions, &name)
            && sha256_file(&tmp)? != expected
        {
            std::fs::remove_file(&tmp).ok();
            report
                .failed
                .push((name, "its SHA-256 does not match the recorded one".into()));
            continue;
        }
        let dest = store_dir.join(&name);
        if dest.is_file() && dest.metadata()?.len() >= tmp.metadata()?.len() {
            std::fs::remove_file(&tmp)?;
            report.kept_local.push(name);
            continue;
        }
        std::fs::rename(&tmp, &dest)?;
        report.restored.push(name);
    }
    Ok(report)
}

fn has_remote(repo: &Path) -> bool {
    git(repo, &["remote"])
        .map(|r| !r.is_empty())
        .unwrap_or(false)
}

fn git(repo: &Path, args: &[&str]) -> Result<String> {
    let out = Command::new("git")
        .arg("-C")
        .arg(repo)
        .args(args)
        .output()?;
    if !out.status.success() {
        return Err(Error::Other(format!(
            "git {} failed in {}: {}",
            args.join(" "),
            repo.display(),
            String::from_utf8_lossy(&out.stderr).trim()
        )));
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// These tests need the real `xz`; skip (loudly) where it is absent.
    fn no_xz() -> bool {
        if xz_available() {
            return false;
        }
        eprintln!("skipping: xz not installed");
        true
    }

    fn init_git(path: &Path) {
        git(path, &["init", "-q"]).unwrap();
        git(path, &["config", "user.email", "t@t"]).unwrap();
        git(path, &["config", "user.name", "t"]).unwrap();
    }

    #[test]
    fn target_is_stored_beside_the_logs_and_must_exist() {
        let store = tempdir().unwrap();
        let target = tempdir().unwrap();
        assert!(stored_target(store.path()).is_none());
        assert!(set_target(store.path(), &target.path().join("nope")).is_err());

        let abs = set_target(store.path(), target.path()).unwrap();
        assert_eq!(stored_target(store.path()), Some(abs));
        assert!(clear_target(store.path()).unwrap());
        assert!(stored_target(store.path()).is_none());
        assert!(!clear_target(store.path()).unwrap());
    }

    #[test]
    fn push_then_pull_round_trips_and_push_is_idempotent() {
        if no_xz() {
            return;
        }
        let store = tempdir().unwrap();
        let target = tempdir().unwrap();
        let body = "{\"n\":1}\n".repeat(500);
        std::fs::write(store.path().join("a.jsonl"), &body).unwrap();
        // Not a log: the config file must not be shadowed.
        set_target(store.path(), target.path()).unwrap();

        assert_eq!(
            status(store.path(), target.path()).unwrap(),
            vec![Entry {
                name: "a.jsonl".into(),
                state: State::LocalOnly
            }]
        );

        let report = push(store.path(), target.path(), true).unwrap();
        assert_eq!(report.written, vec!["a.jsonl"]);
        assert!(report.committed.is_none(), "target is not a git repo");
        let xz = target.path().join("sessions/a.jsonl.xz");
        assert!(xz.metadata().unwrap().len() < body.len() as u64 / 4);
        assert!(target.path().join("sessions/a.jsonl.sha256").is_file());
        assert!(!target.path().join("sessions/shadow.toml.xz").exists());
        assert_eq!(
            status(store.path(), target.path()).unwrap()[0].state,
            State::InSync
        );

        let again = push(store.path(), target.path(), true).unwrap();
        assert!(again.written.is_empty());
        assert_eq!(again.unchanged, vec!["a.jsonl"]);

        // A fresh machine: empty store, pull restores the exact log.
        let fresh = tempdir().unwrap();
        let report = pull(fresh.path(), target.path(), true).unwrap();
        assert_eq!(report.restored, vec!["a.jsonl"]);
        assert_eq!(
            std::fs::read_to_string(fresh.path().join("a.jsonl")).unwrap(),
            body
        );
        assert_eq!(
            status(fresh.path(), target.path()).unwrap()[0].state,
            State::InSync
        );
    }

    #[test]
    fn a_grown_log_is_pushed_again_and_pull_never_shortens_a_local_log() {
        if no_xz() {
            return;
        }
        let store = tempdir().unwrap();
        let target = tempdir().unwrap();
        let log = store.path().join("a.jsonl");
        std::fs::write(&log, "1\n").unwrap();
        push(store.path(), target.path(), true).unwrap();

        std::fs::write(&log, "1\n2\n3\n").unwrap();
        assert_eq!(
            status(store.path(), target.path()).unwrap()[0].state,
            State::Differs
        );
        // Pull before pushing: the shadow copy is shorter, local wins.
        let report = pull(store.path(), target.path(), true).unwrap();
        assert_eq!(report.kept_local, vec!["a.jsonl"]);
        assert_eq!(std::fs::read_to_string(&log).unwrap(), "1\n2\n3\n");

        let report = push(store.path(), target.path(), true).unwrap();
        assert_eq!(report.written, vec!["a.jsonl"]);
    }

    #[test]
    fn pull_rejects_a_log_that_fails_its_hash() {
        if no_xz() {
            return;
        }
        let store = tempdir().unwrap();
        let target = tempdir().unwrap();
        std::fs::write(store.path().join("a.jsonl"), "1\n2\n").unwrap();
        push(store.path(), target.path(), true).unwrap();
        std::fs::write(
            target.path().join("sessions/a.jsonl.sha256"),
            "deadbeef  a.jsonl\n",
        )
        .unwrap();

        let fresh = tempdir().unwrap();
        let report = pull(fresh.path(), target.path(), true).unwrap();
        assert!(report.restored.is_empty());
        assert_eq!(report.failed.len(), 1);
        assert!(!fresh.path().join("a.jsonl").exists());
        assert!(!fresh.path().join("a.jsonl.pull.tmp").exists());
    }

    #[test]
    fn a_git_target_is_committed_and_pushed_and_pulled() {
        if no_xz() {
            return;
        }
        let remote = tempdir().unwrap();
        git(remote.path(), &["init", "-q", "--bare"]).unwrap();
        let clone_a = tempdir().unwrap();
        init_git(clone_a.path());
        std::fs::write(clone_a.path().join("README.md"), "shadow\n").unwrap();
        git(clone_a.path(), &["add", "README.md"]).unwrap();
        git(clone_a.path(), &["commit", "-q", "-m", "init"]).unwrap();
        let url = remote.path().to_string_lossy().to_string();
        git(clone_a.path(), &["remote", "add", "origin", &url]).unwrap();
        let branch = git(clone_a.path(), &["branch", "--show-current"]).unwrap();
        git(clone_a.path(), &["push", "-q", "-u", "origin", &branch]).unwrap();

        let store = tempdir().unwrap();
        std::fs::write(store.path().join("a.jsonl"), "1\n2\n").unwrap();
        let report = push(store.path(), clone_a.path(), true).unwrap();
        assert!(report.committed.is_some());
        assert!(report.pushed, "{:?}", report.push_error);
        // Only the sessions went into the commit.
        let files = git(
            clone_a.path(),
            &["show", "--name-only", "--format=", "HEAD"],
        )
        .unwrap();
        assert!(files.lines().all(|l| l.starts_with("sessions/")), "{files}");

        // Nothing new: no empty commit.
        let head = git(clone_a.path(), &["rev-parse", "HEAD"]).unwrap();
        let again = push(store.path(), clone_a.path(), true).unwrap();
        assert!(again.committed.is_none());
        assert_eq!(git(clone_a.path(), &["rev-parse", "HEAD"]).unwrap(), head);

        // --no-push: committed locally only.
        std::fs::write(store.path().join("b.jsonl"), "x\n").unwrap();
        let local_only = push(store.path(), clone_a.path(), false).unwrap();
        assert!(local_only.committed.is_some() && !local_only.pushed);
        git(clone_a.path(), &["push", "-q"]).unwrap();

        // Another machine: an older clone pulls, then restores.
        let clone_b = tempdir().unwrap();
        git(clone_b.path(), &["clone", "-q", &url, "."]).unwrap();
        git(clone_b.path(), &["reset", "-q", "--hard", "HEAD~1"]).unwrap();
        let fresh = tempdir().unwrap();
        let report = pull(fresh.path(), clone_b.path(), true).unwrap();
        assert!(report.pull_error.is_none(), "{:?}", report.pull_error);
        assert_eq!(report.restored, vec!["a.jsonl", "b.jsonl"]);
    }
}
