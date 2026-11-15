//! Rewrite the commit hashes steps record, after git history was rewritten.
//!
//! A step's `commits` are full hashes. Anything that rewrites history —
//! `git filter-repo`, a rebase before merge, a squash merge — gives the
//! same work new hashes and leaves the recorded ones dangling, so
//! `agentrail audit` can only fall back to matching by time. `remap`
//! applies an old→new mapping (git filter-repo's `commit-map`, or pairs
//! given by hand) to every step, active and archived.

use agentrail_core::error::{Error, Result};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::digest::TRANSCRIPT_FILE;
use crate::step;

const ARCHIVE_DIR: &str = ".agentrail-archive";

/// One old→new pair. `old` may be a prefix (7+ hex digits) of a recorded
/// hash; `new` is a full hash, or all zeros when the rewrite dropped the
/// commit (filter-repo's convention).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Mapping {
    pub old: String,
    pub new: String,
}

impl Mapping {
    fn dropped(&self) -> bool {
        self.new.bytes().all(|b| b == b'0')
    }
}

fn is_hex(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_hexdigit())
}

/// Parse git filter-repo's `commit-map`: an `old new` header line, then
/// one `<old-hash> <new-hash>` pair per line.
pub fn parse_commit_map(text: &str) -> Result<Vec<Mapping>> {
    let mut out = Vec::new();
    for (i, line) in text.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let mut parts = line.split_whitespace();
        let (Some(old), Some(new), None) = (parts.next(), parts.next(), parts.next()) else {
            return Err(Error::Other(format!(
                "commit map line {}: expected `<old-hash> <new-hash>`, got `{line}`",
                i + 1
            )));
        };
        if old == "old" && new == "new" {
            continue;
        }
        if !is_hex(old) || !is_hex(new) || old.len() < 40 || new.len() != old.len() {
            return Err(Error::Other(format!(
                "commit map line {}: `{line}` is not a pair of full commit hashes",
                i + 1
            )));
        }
        out.push(Mapping {
            old: old.to_ascii_lowercase(),
            new: new.to_ascii_lowercase(),
        });
    }
    Ok(out)
}

/// A hand-given pair (`--from OLD --to NEW`). `old` may be abbreviated,
/// since the commit it names may no longer exist to resolve; `new` must
/// already be a full hash (resolve it first).
pub fn manual_mapping(old: &str, new: &str) -> Result<Mapping> {
    if !is_hex(old) || old.len() < 7 {
        return Err(Error::Other(format!(
            "--from `{old}`: give at least 7 hex digits of the old commit hash"
        )));
    }
    Ok(Mapping {
        old: old.to_ascii_lowercase(),
        new: new.to_ascii_lowercase(),
    })
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rewrite {
    /// Step label, e.g. `003-tune` or `old-saga-2026.../003-tune`.
    pub step: String,
    pub old: String,
    pub new: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StepCommit {
    pub step: String,
    pub hash: String,
}

#[derive(Debug, Default)]
pub struct RemapReport {
    /// Hashes replaced (or, in a dry run, that would be).
    pub rewritten: Vec<Rewrite>,
    /// Recorded commits the rewrite dropped; left as they were.
    pub dropped: Vec<StepCommit>,
    /// Recorded commits that do not exist in this repository after the
    /// remap. Empty outside a git repository.
    pub dangling: Vec<StepCommit>,
    /// Steps examined, active and archived.
    pub steps_seen: usize,
}

/// Every step directory, active then archived, with a display label.
fn all_steps(repo_path: &Path) -> Result<Vec<(String, PathBuf, agentrail_core::StepConfig)>> {
    let mut out = Vec::new();
    let active = crate::saga::saga_dir(repo_path);
    if active.is_dir() {
        for (dir, cfg) in step::list_steps(&active)? {
            out.push((format!("{:03}-{}", cfg.number, cfg.slug), dir, cfg));
        }
    }
    let archive_root = repo_path.join(ARCHIVE_DIR);
    if archive_root.is_dir() {
        let mut archives: Vec<PathBuf> = std::fs::read_dir(&archive_root)?
            .filter_map(|e| e.ok())
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        archives.sort();
        for archive in archives {
            let name = archive
                .file_name()
                .unwrap_or_default()
                .to_string_lossy()
                .to_string();
            for (dir, cfg) in step::list_steps(&archive).unwrap_or_default() {
                out.push((format!("{name}/{:03}-{}", cfg.number, cfg.slug), dir, cfg));
            }
        }
    }
    Ok(out)
}

/// Apply `mappings` to every step's recorded commits, in `.agentrail/`
/// and `.agentrail-archive/`. Nothing is written when `dry_run`.
///
/// A commit the rewrite dropped keeps its old hash (there is nothing to
/// point it at) and is reported. Several old commits mapping to one new
/// commit (a squash) collapse to a single entry. The `- commits:` line
/// of a step's transcript.md is rewritten to match.
pub fn remap(repo_path: &Path, mappings: &[Mapping], dry_run: bool) -> Result<RemapReport> {
    let exact: HashMap<&str, &Mapping> = mappings
        .iter()
        .filter(|m| m.old.len() >= 40)
        .map(|m| (m.old.as_str(), m))
        .collect();
    let prefixes: Vec<&Mapping> = mappings.iter().filter(|m| m.old.len() < 40).collect();
    let lookup = |hash: &str| -> Result<Option<&Mapping>> {
        let hash_lc = hash.to_ascii_lowercase();
        if let Some(m) = exact.get(hash_lc.as_str()) {
            return Ok(Some(*m));
        }
        let mut hits = prefixes.iter().filter(|m| hash_lc.starts_with(&m.old));
        let first = hits.next().copied();
        if let (Some(a), Some(b)) = (first, hits.next())
            && a.new != b.new
        {
            return Err(Error::Other(format!(
                "recorded commit {hash} matches both --from {} and --from {}",
                a.old, b.old
            )));
        }
        Ok(first)
    };

    let mut report = RemapReport::default();
    let in_git = is_git_repo(repo_path);

    for (label, dir, mut cfg) in all_steps(repo_path)? {
        report.steps_seen += 1;
        let mut new_commits: Vec<String> = Vec::with_capacity(cfg.commits.len());
        let mut step_rewrites: Vec<Rewrite> = Vec::new();
        for hash in &cfg.commits {
            let replacement = match lookup(hash)? {
                Some(m) if m.dropped() => {
                    report.dropped.push(StepCommit {
                        step: label.clone(),
                        hash: hash.clone(),
                    });
                    hash.clone()
                }
                Some(m) if m.new != hash.to_ascii_lowercase() => {
                    step_rewrites.push(Rewrite {
                        step: label.clone(),
                        old: hash.clone(),
                        new: m.new.clone(),
                    });
                    m.new.clone()
                }
                _ => hash.clone(),
            };
            if !new_commits.contains(&replacement) {
                new_commits.push(replacement);
            }
        }

        if in_git {
            for hash in &new_commits {
                if !commit_exists(repo_path, hash) {
                    report.dangling.push(StepCommit {
                        step: label.clone(),
                        hash: hash.clone(),
                    });
                }
            }
        }

        if step_rewrites.is_empty() {
            continue;
        }
        if !dry_run {
            let transcript = dir.join(TRANSCRIPT_FILE);
            if transcript.is_file() {
                let body = std::fs::read_to_string(&transcript)?;
                let updated = rewrite_transcript_commits(&body, &new_commits);
                if updated != body {
                    std::fs::write(&transcript, updated)?;
                }
            }
            cfg.commits = new_commits;
            step::save_step(&dir, &cfg)?;
        }
        report.rewritten.extend(step_rewrites);
    }
    Ok(report)
}

/// Replace the header's `- commits: ...` line (the first one, before the
/// first `---` rule) with the step's current commits.
fn rewrite_transcript_commits(body: &str, commits: &[String]) -> String {
    let mut out = String::with_capacity(body.len());
    let mut in_header = true;
    for line in body.split_inclusive('\n') {
        if in_header && line.trim_end() == "---" {
            in_header = false;
        }
        if in_header && line.starts_with("- commits: ") {
            out.push_str(&format!("- commits: {}\n", commits.join(", ")));
            in_header = false;
        } else {
            out.push_str(line);
        }
    }
    out
}

fn is_git_repo(path: &Path) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "--git-dir"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn commit_exists(path: &Path, hash: &str) -> bool {
    Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["cat-file", "-e", &format!("{hash}^{{commit}}")])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use agentrail_core::StepRole;
    use tempfile::tempdir;

    const A: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    const B: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";
    const C: &str = "cccccccccccccccccccccccccccccccccccccccc";
    const N1: &str = "1111111111111111111111111111111111111111";
    const N2: &str = "2222222222222222222222222222222222222222";
    const ZERO: &str = "0000000000000000000000000000000000000000";

    fn step_with(root: &Path, number: u32, slug: &str, commits: &[&str]) -> PathBuf {
        let saga_dir = crate::saga::saga_dir(root);
        let dir = step::create_step(&step::CreateStepParams {
            saga_dir: &saga_dir,
            number,
            slug,
            prompt: "p",
            description: "d",
            role: StepRole::Production,
            context_files: &[],
            task_type: None,
            job_spec: None,
        })
        .unwrap();
        let mut cfg = step::load_step(&dir).unwrap();
        cfg.commits = commits.iter().map(|s| s.to_string()).collect();
        step::save_step(&dir, &cfg).unwrap();
        dir
    }

    fn map(pairs: &[(&str, &str)]) -> Vec<Mapping> {
        pairs
            .iter()
            .map(|(o, n)| Mapping {
                old: o.to_string(),
                new: n.to_string(),
            })
            .collect()
    }

    #[test]
    fn parses_a_filter_repo_commit_map() {
        let text =
            format!("old                                      new\n{A} {N1}\n\n{B} {ZERO}\n");
        let m = parse_commit_map(&text).unwrap();
        assert_eq!(m, map(&[(A, N1), (B, ZERO)]));
        assert!(m[1].dropped());

        assert!(parse_commit_map("abc def\n").is_err());
        assert!(parse_commit_map(&format!("{A}\n")).is_err());
        assert!(parse_commit_map(&format!("{A} {N1} extra\n")).is_err());
    }

    #[test]
    fn rewrites_active_and_archived_steps_and_leaves_the_rest() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        crate::saga::init_saga(root, "old", "plan").unwrap();
        step_with(root, 1, "archived", &[A]);
        crate::archive::archive_saga(root, None).unwrap();
        crate::saga::init_saga(root, "new", "plan").unwrap();
        let live = step_with(root, 1, "live", &[B, C]);
        let untouched = step_with(root, 2, "untouched", &[C]);
        let before = std::fs::read_to_string(untouched.join("step.toml")).unwrap();

        let report = remap(root, &map(&[(A, N1), (B, N2)]), false).unwrap();
        assert_eq!(report.steps_seen, 3);
        assert_eq!(report.rewritten.len(), 2);
        assert!(report.dropped.is_empty());

        assert_eq!(step::load_step(&live).unwrap().commits, vec![N2, C]);
        let archive = std::fs::read_dir(root.join(ARCHIVE_DIR))
            .unwrap()
            .next()
            .unwrap()
            .unwrap()
            .path();
        let archived = step::list_steps(&archive).unwrap();
        assert_eq!(archived[0].1.commits, vec![N1]);
        // A step with nothing to remap is not rewritten at all.
        assert_eq!(
            std::fs::read_to_string(untouched.join("step.toml")).unwrap(),
            before
        );

        // Idempotent: the old hashes are gone, so nothing matches.
        let again = remap(root, &map(&[(A, N1), (B, N2)]), false).unwrap();
        assert!(again.rewritten.is_empty());
    }

    #[test]
    fn dry_run_reports_and_writes_nothing() {
        let tmp = tempdir().unwrap();
        crate::saga::init_saga(tmp.path(), "s", "plan").unwrap();
        let dir = step_with(tmp.path(), 1, "x", &[A]);
        let report = remap(tmp.path(), &map(&[(A, N1)]), true).unwrap();
        assert_eq!(report.rewritten.len(), 1);
        assert_eq!(step::load_step(&dir).unwrap().commits, vec![A]);
    }

    #[test]
    fn dropped_commits_keep_their_hash_and_squashes_collapse() {
        let tmp = tempdir().unwrap();
        crate::saga::init_saga(tmp.path(), "s", "plan").unwrap();
        let dir = step_with(tmp.path(), 1, "x", &[A, B, C]);
        let report = remap(tmp.path(), &map(&[(A, N1), (B, N1), (C, ZERO)]), false).unwrap();
        assert_eq!(step::load_step(&dir).unwrap().commits, vec![N1, C]);
        assert_eq!(report.rewritten.len(), 2);
        assert_eq!(
            report.dropped,
            vec![StepCommit {
                step: "001-x".into(),
                hash: C.into()
            }]
        );
    }

    #[test]
    fn manual_pairs_match_by_prefix() {
        let tmp = tempdir().unwrap();
        crate::saga::init_saga(tmp.path(), "s", "plan").unwrap();
        let dir = step_with(tmp.path(), 1, "x", &[A, B]);
        let m = vec![manual_mapping("aaaaaaa", N1).unwrap()];
        remap(tmp.path(), &m, false).unwrap();
        assert_eq!(step::load_step(&dir).unwrap().commits, vec![N1, B]);
        assert!(manual_mapping("abc", N1).is_err());
        assert!(manual_mapping("not-hex", N1).is_err());
    }

    #[test]
    fn transcript_header_follows_the_remap() {
        let tmp = tempdir().unwrap();
        crate::saga::init_saga(tmp.path(), "s", "plan").unwrap();
        let dir = step_with(tmp.path(), 1, "x", &[A]);
        let body = format!(
            "# Transcript: step 001-x\n\n- session: `s`\n- commits: {A}\n- tokens: 1 input\n\n---\n\n**User**\n\n- commits: {A} is what I typed\n"
        );
        std::fs::write(dir.join(TRANSCRIPT_FILE), &body).unwrap();
        remap(tmp.path(), &map(&[(A, N1)]), false).unwrap();
        let after = std::fs::read_to_string(dir.join(TRANSCRIPT_FILE)).unwrap();
        assert!(after.contains(&format!("- commits: {N1}\n- tokens")));
        // The conversation itself is not edited.
        assert!(after.contains(&format!("- commits: {A} is what I typed")));
    }

    #[test]
    fn dangling_commits_are_reported_in_a_git_repo() {
        let tmp = tempdir().unwrap();
        let root = tmp.path();
        let git = |args: &[&str]| {
            let out = Command::new("git")
                .arg("-C")
                .arg(root)
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success(), "git {args:?}");
            String::from_utf8_lossy(&out.stdout).trim().to_string()
        };
        git(&["init", "-q"]);
        git(&["config", "user.email", "t@t"]);
        git(&["config", "user.name", "t"]);
        std::fs::write(root.join("f"), "x").unwrap();
        git(&["add", "f"]);
        git(&["commit", "-q", "-m", "c"]);
        let head = git(&["rev-parse", "HEAD"]);

        crate::saga::init_saga(root, "s", "plan").unwrap();
        step_with(root, 1, "x", &[A, &head]);

        // No mappings: a pure check.
        let report = remap(root, &[], true).unwrap();
        assert_eq!(
            report.dangling,
            vec![StepCommit {
                step: "001-x".into(),
                hash: A.into()
            }]
        );
        // Mapping the dangling one onto a real commit clears it.
        let report = remap(root, &map(&[(A, &head)]), false).unwrap();
        assert!(report.dangling.is_empty());
    }
}
