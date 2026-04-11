//! Audit: compare git history with saga history, report gaps.
//!
//! This module is pure data — it loads sagas (active + archived) and git
//! history, computes matches, and returns a structured report. Rendering
//! (markdown / shell script) is a separate concern in the CLI layer.

use agentrail_core::StepConfig;
use agentrail_core::error::Result;
use chrono::{DateTime, NaiveDateTime};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

use crate::git_history::{self, Commit};
use crate::{saga, step};

const ARCHIVE_DIR: &str = ".agentrail-archive";

/// A reference to a step within some saga, sufficient for reporting.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepRef {
    pub saga_name: String,
    pub number: u32,
    pub slug: String,
    pub description: String,
}

impl StepRef {
    fn from_step(saga_name: &str, cfg: &StepConfig) -> Self {
        Self {
            saga_name: saga_name.to_string(),
            number: cfg.number,
            slug: cfg.slug.clone(),
            description: cfg.description.clone(),
        }
    }
}

#[derive(Debug, Clone)]
pub struct AuditReport {
    pub repo_path: PathBuf,
    pub since: Option<String>,
    pub active_saga_name: Option<String>,
    pub archived_saga_count: usize,
    pub matched: Vec<(Commit, StepRef)>,
    pub orphan_commits: Vec<Commit>,
    pub orphan_steps: Vec<StepRef>,
    pub uncommitted: Vec<String>,
}

impl AuditReport {
    pub fn has_gaps(&self) -> bool {
        !self.orphan_commits.is_empty() || !self.orphan_steps.is_empty()
    }
}

/// Run an audit over a repository.
///
/// Logic:
/// 1. Read first-parent git history in reverse-chronological order (newest
///    first).
/// 2. Collect all commit hashes claimed by archived sagas — these are
///    excluded from gap analysis.
/// 3. For each step in the active saga:
///    - If it has recorded `commits`, mark those matched.
///    - Otherwise, try to match by timestamp window (created_at..completed_at
///      or next step's created_at).
///    - If nothing matches, it's an orphan step.
/// 4. Remaining unclaimed, unmatched commits are orphan commits.
pub fn run(repo_path: &Path, since: Option<&str>) -> Result<AuditReport> {
    let history = git_history::read_history(repo_path, since)?;

    // Collect commits claimed by archived sagas. Archived sagas' steps either
    // have explicit commits (new-style) or we claim the whole time range
    // (legacy). For legacy, we track the latest timestamp and then claim any
    // commit <= that timestamp. This is conservative: old commits are claimed.
    let archive_state = load_archive_state(repo_path)?;
    let mut claimed: HashSet<String> = archive_state.explicit_commits.clone();

    if let Some(latest) = archive_state.latest_legacy_timestamp {
        for c in &history {
            if let Some(ts) = parse_commit_ts(&c.timestamp)
                && ts <= latest
            {
                claimed.insert(c.hash.clone());
            }
        }
    }

    // Load active saga and its steps.
    let (active_name, active_steps) = if saga::saga_exists(repo_path) {
        let cfg = saga::load_saga(repo_path)?;
        let saga_dir = saga::saga_dir(repo_path);
        let steps = step::list_steps(&saga_dir)?;
        (Some(cfg.name), steps.into_iter().map(|(_, s)| s).collect())
    } else {
        (None, Vec::<StepConfig>::new())
    };

    let active_name_str = active_name.clone().unwrap_or_else(|| "(none)".to_string());

    // Index commit hash -> step via recorded commits first.
    let mut matched: Vec<(Commit, StepRef)> = Vec::new();
    let mut orphan_steps: Vec<StepRef> = Vec::new();
    let mut matched_hashes: HashSet<String> = HashSet::new();

    // Sort active steps by number ascending so window fallback can use "next
    // step's created_at" as an upper bound.
    let mut steps_sorted = active_steps.clone();
    steps_sorted.sort_by_key(|s| s.number);

    for (i, step_cfg) in steps_sorted.iter().enumerate() {
        let step_ref = StepRef::from_step(&active_name_str, step_cfg);

        if !step_cfg.commits.is_empty() {
            let mut any_matched = false;
            for hash in &step_cfg.commits {
                if let Some(commit) = history.iter().find(|c| &c.hash == hash) {
                    matched.push((commit.clone(), step_ref.clone()));
                    matched_hashes.insert(hash.clone());
                    any_matched = true;
                }
            }
            if !any_matched {
                // Recorded commits all rebased away / not in current history.
                orphan_steps.push(step_ref);
            }
            continue;
        }

        // Heuristic window match for legacy steps.
        let lower = parse_step_ts(&step_cfg.created_at);
        let upper = step_cfg
            .completed_at
            .as_deref()
            .and_then(parse_step_ts)
            .or_else(|| {
                steps_sorted
                    .get(i + 1)
                    .and_then(|s| parse_step_ts(&s.created_at))
            });

        let window_hit = lower.and_then(|lo| {
            history.iter().find(|c| {
                if matched_hashes.contains(&c.hash) || claimed.contains(&c.hash) {
                    return false;
                }
                match parse_commit_ts(&c.timestamp) {
                    Some(cts) => cts >= lo && upper.is_none_or(|up| cts <= up),
                    None => false,
                }
            })
        });

        match window_hit {
            Some(commit) => {
                matched.push((commit.clone(), step_ref));
                matched_hashes.insert(commit.hash.clone());
            }
            None => orphan_steps.push(step_ref),
        }
    }

    // Orphan commits: everything not claimed by archive and not matched.
    let orphan_commits: Vec<Commit> = history
        .iter()
        .filter(|c| !claimed.contains(&c.hash) && !matched_hashes.contains(&c.hash))
        .cloned()
        .collect();

    let uncommitted = git_history::uncommitted_paths(repo_path);

    Ok(AuditReport {
        repo_path: repo_path.to_path_buf(),
        since: since.map(|s| s.to_string()),
        active_saga_name: active_name,
        archived_saga_count: archive_state.count,
        matched,
        orphan_commits,
        orphan_steps,
        uncommitted,
    })
}

struct ArchiveState {
    count: usize,
    explicit_commits: HashSet<String>,
    /// Latest completed_at/created_at timestamp seen across all legacy
    /// (no-commits) archived steps. Commits at or before this are claimed.
    latest_legacy_timestamp: Option<NaiveDateTime>,
}

fn load_archive_state(repo_path: &Path) -> Result<ArchiveState> {
    let mut state = ArchiveState {
        count: 0,
        explicit_commits: HashSet::new(),
        latest_legacy_timestamp: None,
    };

    let archive_root = repo_path.join(ARCHIVE_DIR);
    if !archive_root.is_dir() {
        return Ok(state);
    }

    for entry in std::fs::read_dir(&archive_root)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let saga_toml = entry.path().join("saga.toml");
        if !saga_toml.is_file() {
            continue;
        }
        state.count += 1;

        let steps = step::list_steps(&entry.path()).unwrap_or_default();
        let mut had_explicit = false;
        for (_, step_cfg) in &steps {
            if !step_cfg.commits.is_empty() {
                had_explicit = true;
                for h in &step_cfg.commits {
                    state.explicit_commits.insert(h.clone());
                }
            }
        }

        // For legacy steps in this archive, take the max completed_at (or
        // created_at as fallback) and fold it into the global upper bound.
        if !had_explicit || steps.iter().any(|(_, s)| s.commits.is_empty()) {
            let mut latest: Option<NaiveDateTime> = None;
            for (_, s) in &steps {
                let ts = s
                    .completed_at
                    .as_deref()
                    .and_then(parse_step_ts)
                    .or_else(|| parse_step_ts(&s.created_at));
                if let Some(t) = ts
                    && latest.is_none_or(|cur| t > cur)
                {
                    latest = Some(t);
                }
            }
            if let Some(t) = latest
                && state.latest_legacy_timestamp.is_none_or(|cur| t > cur)
            {
                state.latest_legacy_timestamp = Some(t);
            }
        }
    }

    Ok(state)
}

fn parse_step_ts(s: &str) -> Option<NaiveDateTime> {
    NaiveDateTime::parse_from_str(s, "%Y-%m-%dT%H:%M:%S").ok()
}

fn parse_commit_ts(s: &str) -> Option<NaiveDateTime> {
    DateTime::parse_from_rfc3339(s)
        .ok()
        .map(|dt| dt.naive_local())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saga;
    use agentrail_core::{StepRole, StepStatus};
    use std::process::Command;

    fn init_repo(path: &Path) {
        run_git(path, &["init", "-q", "-b", "main"]);
        run_git(path, &["config", "user.email", "t@t"]);
        run_git(path, &["config", "user.name", "t"]);
        run_git(path, &["config", "commit.gpgsign", "false"]);
    }

    fn run_git(path: &Path, args: &[&str]) {
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

    fn commit(path: &Path, file: &str, msg: &str) -> String {
        std::fs::write(path.join(file), msg).unwrap();
        run_git(path, &["add", file]);
        run_git(path, &["commit", "-q", "-m", msg]);
        let out = Command::new("git")
            .arg("-C")
            .arg(path)
            .args(["rev-parse", "HEAD"])
            .output()
            .unwrap();
        String::from_utf8(out.stdout).unwrap().trim().to_string()
    }

    #[test]
    fn no_agentrail_no_git_empty_report() {
        let tmp = tempfile::tempdir().unwrap();
        let report = run(tmp.path(), None).unwrap();
        assert!(report.active_saga_name.is_none());
        assert_eq!(report.archived_saga_count, 0);
        assert!(report.matched.is_empty());
        assert!(report.orphan_commits.is_empty());
        assert!(report.orphan_steps.is_empty());
        assert!(!report.has_gaps());
    }

    #[test]
    fn no_agentrail_all_commits_are_orphans() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        init_repo(p);
        commit(p, "a.txt", "first");
        commit(p, "b.txt", "second");

        let report = run(p, None).unwrap();
        assert!(report.active_saga_name.is_none());
        assert_eq!(report.orphan_commits.len(), 2);
        assert!(report.has_gaps());
    }

    #[test]
    fn step_with_recorded_commit_matches() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        init_repo(p);
        let hash = commit(p, "a.txt", "step one work");

        saga::init_saga(p, "test", "# plan").unwrap();
        let saga_dir = saga::saga_dir(p);
        step::create_step(&step::CreateStepParams {
            saga_dir: &saga_dir,
            number: 1,
            slug: "one",
            prompt: "do the thing",
            description: "do the thing",
            role: StepRole::Production,
            context_files: &[],
            task_type: None,
            job_spec: None,
        })
        .unwrap();

        let step_dir = step::find_step_dir(&saga_dir, 1).unwrap();
        let mut cfg = step::load_step(&step_dir).unwrap();
        cfg.status = StepStatus::Completed;
        cfg.commits.push(hash.clone());
        step::save_step(&step_dir, &cfg).unwrap();

        let report = run(p, None).unwrap();
        assert_eq!(report.matched.len(), 1);
        assert_eq!(report.matched[0].0.hash, hash);
        assert!(report.orphan_commits.is_empty());
        assert!(report.orphan_steps.is_empty());
    }

    #[test]
    fn commit_after_last_step_is_orphan() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        init_repo(p);
        let step_hash = commit(p, "a.txt", "step one");

        saga::init_saga(p, "test", "# plan").unwrap();
        let saga_dir = saga::saga_dir(p);
        step::create_step(&step::CreateStepParams {
            saga_dir: &saga_dir,
            number: 1,
            slug: "one",
            prompt: "",
            description: "step one",
            role: StepRole::Production,
            context_files: &[],
            task_type: None,
            job_spec: None,
        })
        .unwrap();
        let step_dir = step::find_step_dir(&saga_dir, 1).unwrap();
        let mut cfg = step::load_step(&step_dir).unwrap();
        cfg.status = StepStatus::Completed;
        cfg.commits.push(step_hash);
        step::save_step(&step_dir, &cfg).unwrap();

        // A commit after the step that wasn't recorded anywhere.
        commit(p, "b.txt", "orphaned work");

        let report = run(p, None).unwrap();
        assert_eq!(report.matched.len(), 1);
        assert_eq!(report.orphan_commits.len(), 1);
        assert_eq!(report.orphan_commits[0].subject, "orphaned work");
        assert!(report.has_gaps());
    }

    #[test]
    fn archived_saga_claims_its_commits() {
        let tmp = tempfile::tempdir().unwrap();
        let p = tmp.path();
        init_repo(p);
        let archived_hash = commit(p, "a.txt", "archived work");

        // Create a saga, record the commit, archive it.
        saga::init_saga(p, "old", "# plan").unwrap();
        let saga_dir = saga::saga_dir(p);
        step::create_step(&step::CreateStepParams {
            saga_dir: &saga_dir,
            number: 1,
            slug: "one",
            prompt: "",
            description: "",
            role: StepRole::Production,
            context_files: &[],
            task_type: None,
            job_spec: None,
        })
        .unwrap();
        let step_dir = step::find_step_dir(&saga_dir, 1).unwrap();
        let mut cfg = step::load_step(&step_dir).unwrap();
        cfg.commits.push(archived_hash.clone());
        step::save_step(&step_dir, &cfg).unwrap();

        crate::archive::archive_saga(p, None).unwrap();

        // Now a new commit comes in (no new saga yet).
        commit(p, "b.txt", "new work after archive");

        let report = run(p, None).unwrap();
        assert_eq!(report.archived_saga_count, 1);
        // archived_hash is claimed, only the new commit is orphan.
        assert_eq!(report.orphan_commits.len(), 1);
        assert_eq!(report.orphan_commits[0].subject, "new work after archive");
        assert!(
            !report
                .orphan_commits
                .iter()
                .any(|c| c.hash == archived_hash)
        );
    }
}
