//! Reads and snapshots Claude Code session JSONL files from ~/.claude/projects/.

use agentrail_core::error::{Error, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Derive the Claude projects directory name from a working directory path.
pub fn projects_dir_name(cwd: &Path) -> String {
    let abs = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    mangle_path(&abs)
}

fn projects_dir_name_raw(cwd: &Path) -> String {
    mangle_path(cwd)
}

fn mangle_path(path: &Path) -> String {
    let name = path.to_string_lossy().replace('/', "-");
    if name.len() > 1 {
        name.trim_end_matches('-').to_string()
    } else {
        name
    }
}

/// Return the full path to the Claude projects directory for a given cwd.
pub fn claude_projects_dir(cwd: &Path) -> Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| Error::Other("Cannot determine home directory".into()))?;
    let projects = home.join(".claude").join("projects");

    let canonical_dir = projects.join(projects_dir_name(cwd));
    if canonical_dir.is_dir() {
        return Ok(canonical_dir);
    }

    let raw_dir = projects.join(projects_dir_name_raw(cwd));
    if raw_dir.is_dir() {
        return Ok(raw_dir);
    }

    Ok(canonical_dir)
}

/// Find all session JSONL files, sorted by modification time (most recent last).
pub fn find_session_files(projects_dir: &Path) -> Result<Vec<PathBuf>> {
    if !projects_dir.is_dir() {
        return Ok(vec![]);
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(projects_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "jsonl"))
        .map(|e| e.path())
        .collect();

    files.sort_by_key(|p| {
        p.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });
    Ok(files)
}

/// Extract human-readable conversation text from a JSONL file.
pub fn extract_conversation(jsonl_path: &Path) -> Result<String> {
    let content = std::fs::read_to_string(jsonl_path)?;
    let mut output = String::new();

    for line in content.lines() {
        let Some(v) = serde_json::from_str::<Value>(line).ok() else {
            continue;
        };
        let Some(record_type) = v.get("type").and_then(|t| t.as_str()) else {
            continue;
        };
        let ts = v.get("timestamp").and_then(|t| t.as_str()).unwrap_or("?");

        match record_type {
            "user" => {
                if let Some(msg) = v.get("message") {
                    let text = extract_text_content(msg);
                    if !text.is_empty() {
                        output.push_str(&format!("[{ts}] USER:\n{text}\n\n"));
                    }
                }
            }
            "assistant" => {
                if let Some(msg) = v.get("message") {
                    let text = extract_assistant_blocks(msg);
                    if !text.is_empty() {
                        output.push_str(&format!("[{ts}] ASSISTANT:\n{text}\n\n"));
                    }
                }
            }
            _ => {}
        }
    }

    Ok(output)
}

fn extract_text_content(msg: &Value) -> String {
    if let Some(content) = msg.get("content") {
        if let Some(s) = content.as_str() {
            return s.trim().to_string();
        }
        if let Some(arr) = content.as_array() {
            let mut parts = Vec::new();
            for item in arr {
                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        parts.push(trimmed.to_string());
                    }
                }
            }
            return parts.join("\n");
        }
    }
    String::new()
}

fn extract_assistant_blocks(msg: &Value) -> String {
    if let Some(arr) = msg.get("content").and_then(|c| c.as_array()) {
        let mut parts = Vec::new();
        for item in arr {
            if item.get("type").and_then(|t| t.as_str()) == Some("text")
                && let Some(text) = item.get("text").and_then(|t| t.as_str())
            {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
            }
            if item.get("type").and_then(|t| t.as_str()) == Some("tool_use")
                && let Some(name) = item.get("name").and_then(|n| n.as_str())
            {
                parts.push(format!("[tool: {name}]"));
            }
        }
        return parts.join("\n");
    }
    String::new()
}

/// Environment variable overriding the root of the session-log store.
pub const SESSIONS_DIR_ENV: &str = "AGENTRAIL_SESSIONS_DIR";

/// Root of the session-log store: `$AGENTRAIL_SESSIONS_DIR`, else
/// `~/.agentrail/sessions/`.
///
/// Session logs live OUTSIDE the repository. They quote every file the
/// agent read and every command's output, and they only grow, so a log
/// committed with each step is stored again in full each time. Outside
/// the working tree nothing can commit one by accident: not `git add -f`,
/// not `agentrail archive`, not `agentrail snapshot`.
pub fn session_store_root() -> Result<PathBuf> {
    if let Some(dir) = std::env::var_os(SESSIONS_DIR_ENV)
        && !dir.is_empty()
    {
        return Ok(PathBuf::from(dir));
    }
    let home =
        dirs::home_dir().ok_or_else(|| Error::Other("Cannot determine home directory".into()))?;
    Ok(home.join(".agentrail").join("sessions"))
}

/// The session-log store for one repository: `<root>/<mangled repo path>/`,
/// keyed the same way Claude Code keys `~/.claude/projects/`.
pub fn session_store_dir(cwd: &Path) -> Result<PathBuf> {
    Ok(session_store_root()?.join(projects_dir_name(cwd)))
}

/// Snapshot the latest session JSONL into the repository's session store
/// (outside the repository; see [`session_store_root`]).
pub fn snapshot_session(cwd: &Path) -> Result<(PathBuf, usize)> {
    let projects_dir = claude_projects_dir(cwd)?;
    snapshot_session_into(&projects_dir, &session_store_dir(cwd)?)
}

/// Snapshot the latest session JSONL in `projects_dir` into `store_dir`.
pub fn snapshot_session_into(projects_dir: &Path, store_dir: &Path) -> Result<(PathBuf, usize)> {
    let session_files = find_session_files(projects_dir)?;

    let latest = session_files.last().ok_or_else(|| {
        Error::Other(format!(
            "No session JSONL files found in {}",
            projects_dir.display()
        ))
    })?;

    std::fs::create_dir_all(store_dir)?;

    let session_name = latest
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let snapshot_path = store_dir.join(format!("{session_name}.jsonl"));
    let new_lines = append_new_lines(latest, &snapshot_path)?;

    Ok((snapshot_path, new_lines))
}

fn append_new_lines(source: &Path, snapshot: &Path) -> Result<usize> {
    let source_content = std::fs::read_to_string(source)?;
    let source_lines: Vec<&str> = source_content.lines().collect();

    let existing_lines = if snapshot.is_file() {
        std::fs::read_to_string(snapshot)?.lines().count()
    } else {
        0
    };

    let new_lines = source_lines.len().saturating_sub(existing_lines);

    if new_lines > 0 {
        let new_content: String = source_lines[existing_lines..]
            .iter()
            .map(|l| format!("{l}\n"))
            .collect();

        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(snapshot)?;
        file.write_all(new_content.as_bytes())?;
    }

    Ok(new_lines)
}

/// Session logs still inside the repository: files under
/// `.agentrail/sessions/` and `.agentrail-archive/*/sessions/`, left by
/// older agentrail versions. Paths are relative to `repo_path`, sorted.
pub fn in_repo_logs(repo_path: &Path) -> Vec<PathBuf> {
    let mut found = Vec::new();
    for dir in in_repo_session_dirs(repo_path) {
        collect_files(&repo_path.join(&dir), &dir, &mut found);
    }
    found.sort();
    found
}

/// The in-repo `sessions/` directories that exist, relative to `repo_path`.
fn in_repo_session_dirs(repo_path: &Path) -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    let live = Path::new(".agentrail").join("sessions");
    if repo_path.join(&live).is_dir() {
        dirs.push(live);
    }
    let archive = Path::new(".agentrail-archive");
    if let Ok(entries) = std::fs::read_dir(repo_path.join(archive)) {
        let mut archived: Vec<PathBuf> = entries
            .filter_map(|e| e.ok())
            .map(|e| archive.join(e.file_name()).join("sessions"))
            .filter(|rel| repo_path.join(rel).is_dir())
            .collect();
        archived.sort();
        dirs.extend(archived);
    }
    dirs
}

fn collect_files(abs: &Path, rel: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(abs) else {
        return;
    };
    for entry in entries.filter_map(|e| e.ok()) {
        let rel = rel.join(entry.file_name());
        let path = entry.path();
        if path.is_dir() {
            collect_files(&path, &rel, out);
        } else {
            out.push(rel);
        }
    }
}

/// Warning for `next` / `complete` / `archive` when session logs are
/// still inside the repository. `None` when there are none.
pub fn in_repo_warning(repo_path: &Path) -> Option<String> {
    let logs = in_repo_logs(repo_path);
    if logs.is_empty() {
        return None;
    }
    let bytes: u64 = logs
        .iter()
        .filter_map(|p| repo_path.join(p).metadata().ok())
        .map(|m| m.len())
        .sum();
    Some(format!(
        "⚠ {} session log(s) ({}) are still inside the repository, e.g. {}\n  \
         Session logs quote every file read and every command's output, and they only grow:\n  \
         committed, they publish all of it and store the whole log again at every step.\n  \
         Move them out and stop tracking them:  agentrail sessions migrate",
        logs.len(),
        agentrail_core::human_size(bytes),
        logs[0].display()
    ))
}

/// What `migrate_in_repo_logs` did.
#[derive(Debug, Default)]
pub struct MigrateReport {
    /// Logs moved into the store (repo-relative source paths).
    pub moved: Vec<PathBuf>,
    /// Logs dropped because the store already held a copy at least as
    /// long (logs only grow, so the longer copy contains the shorter).
    pub superseded: Vec<PathBuf>,
    /// Whether any of them were tracked by git and are now staged for
    /// removal from the index.
    pub untracked_from_git: bool,
    /// Where they went.
    pub store_dir: PathBuf,
}

/// Move every in-repo session log into `store_dir` and, in a git repo,
/// stage their removal from the index (`git rm --cached`). Working
/// copies are never deleted without a copy at least as long in the store.
pub fn migrate_in_repo_logs(repo_path: &Path, store_dir: &Path) -> Result<MigrateReport> {
    let mut report = MigrateReport {
        store_dir: store_dir.to_path_buf(),
        ..Default::default()
    };
    let dirs = in_repo_session_dirs(repo_path);
    let logs = in_repo_logs(repo_path);
    if logs.is_empty() && dirs.is_empty() {
        return Ok(report);
    }

    // Untrack first, while the paths still exist: `--cached` leaves the
    // working copies alone, and `--ignore-unmatch` makes untracked (or
    // non-git) a no-op rather than an error.
    for dir in &dirs {
        let tracked = std::process::Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(["ls-files", "--"])
            .arg(dir)
            .output()
            .map(|o| o.status.success() && !o.stdout.is_empty())
            .unwrap_or(false);
        if !tracked {
            continue;
        }
        let out = std::process::Command::new("git")
            .arg("-C")
            .arg(repo_path)
            .args(["rm", "-r", "--cached", "--quiet", "--ignore-unmatch", "--"])
            .arg(dir)
            .output()?;
        if !out.status.success() {
            return Err(Error::Other(format!(
                "git rm --cached {} failed: {}",
                dir.display(),
                String::from_utf8_lossy(&out.stderr).trim()
            )));
        }
        report.untracked_from_git = true;
    }

    std::fs::create_dir_all(store_dir)?;
    for rel in logs {
        let src = repo_path.join(&rel);
        let Some(name) = rel.file_name() else {
            continue;
        };
        let dest = store_dir.join(name);
        let src_len = src.metadata()?.len();
        if dest.is_file() && dest.metadata()?.len() >= src_len {
            std::fs::remove_file(&src)?;
            report.superseded.push(rel);
            continue;
        }
        move_file(&src, &dest)?;
        report.moved.push(rel);
    }

    // Remove the emptied directories; a non-empty one is left in place.
    for dir in &dirs {
        remove_empty_dirs(&repo_path.join(dir));
    }
    Ok(report)
}

/// Rename, falling back to copy + delete across filesystems.
fn move_file(src: &Path, dest: &Path) -> Result<()> {
    if std::fs::rename(src, dest).is_ok() {
        return Ok(());
    }
    std::fs::copy(src, dest)?;
    std::fs::remove_file(src)?;
    Ok(())
}

fn remove_empty_dirs(dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.filter_map(|e| e.ok()) {
            if entry.path().is_dir() {
                remove_empty_dirs(&entry.path());
            }
        }
    }
    // Fails (harmlessly) when not empty.
    std::fs::remove_dir(dir).ok();
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::process::Command;
    use tempfile::tempdir;

    fn git(path: &Path, args: &[&str]) -> String {
        let out = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(args)
            .output()
            .unwrap();
        assert!(out.status.success(), "git {args:?} failed");
        String::from_utf8_lossy(&out.stdout).to_string()
    }

    fn init_git(path: &Path) {
        git(path, &["init", "-q"]);
        git(path, &["config", "user.email", "t@t"]);
        git(path, &["config", "user.name", "t"]);
    }

    #[test]
    fn snapshot_writes_to_the_store_and_appends_only_new_lines() {
        let projects = tempdir().unwrap();
        let store = tempdir().unwrap();
        let store_dir = store.path().join("repo-key");
        let log = projects.path().join("abc.jsonl");
        std::fs::write(&log, "{\"a\":1}\n{\"a\":2}\n").unwrap();

        let (path, new) = snapshot_session_into(projects.path(), &store_dir).unwrap();
        assert_eq!(path, store_dir.join("abc.jsonl"));
        assert_eq!(new, 2);

        std::fs::write(&log, "{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n").unwrap();
        let (_, new) = snapshot_session_into(projects.path(), &store_dir).unwrap();
        assert_eq!(new, 1);
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "{\"a\":1}\n{\"a\":2}\n{\"a\":3}\n"
        );
    }

    #[test]
    fn in_repo_logs_finds_live_and_archived() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        assert!(in_repo_logs(root).is_empty());
        assert!(in_repo_warning(root).is_none());

        std::fs::create_dir_all(root.join(".agentrail/sessions")).unwrap();
        std::fs::write(root.join(".agentrail/sessions/a.jsonl"), "x\n").unwrap();
        std::fs::create_dir_all(root.join(".agentrail-archive/old-1/sessions")).unwrap();
        std::fs::write(
            root.join(".agentrail-archive/old-1/sessions/b.jsonl"),
            "y\n",
        )
        .unwrap();

        let logs = in_repo_logs(root);
        assert_eq!(
            logs,
            vec![
                PathBuf::from(".agentrail/sessions/a.jsonl"),
                PathBuf::from(".agentrail-archive/old-1/sessions/b.jsonl"),
            ]
        );
        let warn = in_repo_warning(root).unwrap();
        assert!(warn.contains("2 session log(s)"));
        assert!(warn.contains("agentrail sessions migrate"));
    }

    #[test]
    fn an_empty_sessions_dir_is_not_a_log() {
        let tmp = tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".agentrail/sessions")).unwrap();
        assert!(in_repo_warning(tmp.path()).is_none());
    }

    #[test]
    fn migrate_moves_logs_out_and_untracks_them() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        let store = tempdir().unwrap();
        init_git(root);
        std::fs::create_dir_all(root.join(".agentrail/sessions")).unwrap();
        std::fs::write(root.join(".agentrail/saga.toml"), "name = \"x\"\n").unwrap();
        std::fs::write(root.join(".agentrail/sessions/a.jsonl"), "1\n2\n").unwrap();
        std::fs::create_dir_all(root.join(".agentrail-archive/old-1/sessions")).unwrap();
        std::fs::write(
            root.join(".agentrail-archive/old-1/sessions/b.jsonl"),
            "y\n",
        )
        .unwrap();
        git(root, &["add", "-A"]);
        git(root, &["commit", "-q", "-m", "saga"]);

        let report = migrate_in_repo_logs(root, store.path()).unwrap();
        assert_eq!(report.moved.len(), 2);
        assert!(report.untracked_from_git);

        // Out of the working tree, into the store, contents intact.
        assert!(!root.join(".agentrail/sessions").exists());
        assert!(!root.join(".agentrail-archive/old-1/sessions").exists());
        assert_eq!(
            std::fs::read_to_string(store.path().join("a.jsonl")).unwrap(),
            "1\n2\n"
        );
        assert!(store.path().join("b.jsonl").is_file());

        // Removal is staged; the rest of the saga record is untouched.
        let tracked = git(root, &["ls-files"]);
        assert!(!tracked.contains("sessions"));
        assert!(tracked.contains(".agentrail/saga.toml"));
        assert!(root.join(".agentrail/saga.toml").is_file());
        assert!(in_repo_logs(root).is_empty());

        // Idempotent.
        let again = migrate_in_repo_logs(root, store.path()).unwrap();
        assert!(again.moved.is_empty() && !again.untracked_from_git);
    }

    #[test]
    fn migrate_keeps_the_longer_copy() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        let store = tempdir().unwrap();
        std::fs::create_dir_all(root.join(".agentrail/sessions")).unwrap();

        // Store already has a longer copy: the repo copy is dropped.
        std::fs::write(store.path().join("a.jsonl"), "1\n2\n3\n").unwrap();
        std::fs::write(root.join(".agentrail/sessions/a.jsonl"), "1\n").unwrap();
        // Store has a shorter copy: the repo copy replaces it.
        std::fs::write(store.path().join("b.jsonl"), "1\n").unwrap();
        std::fs::write(root.join(".agentrail/sessions/b.jsonl"), "1\n2\n").unwrap();

        let report = migrate_in_repo_logs(root, store.path()).unwrap();
        assert_eq!(report.superseded.len(), 1);
        assert_eq!(report.moved.len(), 1);
        assert_eq!(
            std::fs::read_to_string(store.path().join("a.jsonl")).unwrap(),
            "1\n2\n3\n"
        );
        assert_eq!(
            std::fs::read_to_string(store.path().join("b.jsonl")).unwrap(),
            "1\n2\n"
        );
    }
}
