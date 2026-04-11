use agentrail_core::error::Result;
use agentrail_store::audit::{self, AuditReport};
use agentrail_store::git_history::Commit;
use std::path::Path;

pub struct AuditArgs<'a> {
    pub since: Option<&'a str>,
    pub emit_commands: bool,
}

pub fn run(saga_path: &Path, args: &AuditArgs<'_>) -> Result<()> {
    let report = audit::run(saga_path, args.since)?;

    if args.emit_commands {
        print_shell_script(&report);
    } else {
        print_markdown_report(&report);
    }

    Ok(())
}

fn print_markdown_report(r: &AuditReport) {
    println!("# Agentrail Audit Report");
    println!();
    println!("- **Repository**: `{}`", r.repo_path.display());
    println!("- **Branch scope**: current branch, first-parent only");
    println!(
        "- **Range**: {}",
        r.since
            .as_deref()
            .map(|s| format!("{s}..HEAD"))
            .unwrap_or_else(|| "HEAD (all history)".to_string())
    );
    println!(
        "- **Active saga**: {}",
        r.active_saga_name.as_deref().unwrap_or("(none)")
    );
    println!("- **Archived sagas**: {}", r.archived_saga_count);
    println!();

    println!("## Summary");
    println!();
    println!("| Section | Count |");
    println!("|---|---|");
    println!("| Matched (commit ↔ step) | {} |", r.matched.len());
    println!("| Orphan commits (no step) | {} |", r.orphan_commits.len());
    println!("| Orphan steps (no commit) | {} |", r.orphan_steps.len());
    println!(
        "| Uncommitted working-tree entries | {} |",
        r.uncommitted.len()
    );
    println!();

    if !r.matched.is_empty() {
        println!("## Matched");
        println!();
        println!("| Commit | Date | Step | Subject |");
        println!("|---|---|---|---|");
        for (c, s) in &r.matched {
            println!(
                "| `{}` | {} | {:03}-{} | {} |",
                short_hash(&c.hash),
                c.timestamp,
                s.number,
                s.slug,
                escape_md(&c.subject),
            );
        }
        println!();
    }

    if !r.orphan_commits.is_empty() {
        println!("## Orphan commits (missing steps)");
        println!();
        println!(
            "These commits have no matching saga step. Use `--emit-commands` to get `agentrail add` scaffolding."
        );
        println!();
        println!("| Commit | Date | Subject |");
        println!("|---|---|---|");
        for c in &r.orphan_commits {
            println!(
                "| `{}` | {} | {} |",
                short_hash(&c.hash),
                c.timestamp,
                escape_md(&c.subject),
            );
        }
        println!();
    }

    if !r.orphan_steps.is_empty() {
        println!("## Orphan steps (no matching commit)");
        println!();
        println!(
            "These steps exist in the saga but no commit could be matched. The commit may have been rebased away, squashed, or the step may have recorded a hash no longer reachable."
        );
        println!();
        println!("| Step | Description |");
        println!("|---|---|");
        for s in &r.orphan_steps {
            println!(
                "| {:03}-{} | {} |",
                s.number,
                s.slug,
                escape_md(&s.description)
            );
        }
        println!();
    }

    if !r.uncommitted.is_empty() {
        println!("## Working tree (uncommitted)");
        println!();
        println!("Not turned into commands — reported for awareness only.");
        println!();
        println!("```");
        for line in &r.uncommitted {
            println!("{line}");
        }
        println!("```");
        println!();
    }

    if r.has_gaps() {
        println!(
            "**Gaps detected.** Re-run with `--emit-commands` to get a shell script of suggested `agentrail add` / `agentrail init` lines that a human or agent can review and edit before executing."
        );
    } else {
        println!("**No gaps detected.**");
    }
}

fn print_shell_script(r: &AuditReport) {
    println!("#!/bin/sh");
    println!("# agentrail audit --emit-commands");
    println!("# Repo: {}", r.repo_path.display());
    println!("# Generated: {}", agentrail_core::timestamp_iso());
    println!("#");
    println!("# REVIEW BEFORE RUNNING.");
    println!("# Each `agentrail add` line below is a draft derived from a commit");
    println!("# that has no matching saga step. The slug and prompt are seeded");
    println!("# from the commit subject; reword them to match how you want the");
    println!("# retroactive step described before running.");
    println!("#");
    println!("# Matched: {}", r.matched.len());
    println!("# Orphan commits: {}", r.orphan_commits.len());
    println!("# Orphan steps: {}", r.orphan_steps.len());
    println!("# Uncommitted entries: {}", r.uncommitted.len());
    println!();

    if r.orphan_commits.is_empty() && r.orphan_steps.is_empty() {
        println!("# No gaps detected — nothing to do.");
        return;
    }

    if r.active_saga_name.is_none() && !r.orphan_commits.is_empty() {
        println!("# No active saga exists. Seed a retroactive saga covering prior");
        println!("# history, then add each orphan commit as a step.");
        println!("agentrail init \\");
        println!("  --name 'development' \\");
        println!("  --plan 'Retroactive history reconstructed from git.' \\");
        println!("  --retroactive");
        println!();
    }

    // Orphan commits are in reverse-chronological order from git log; reverse
    // so the shell script adds them in the order they were committed.
    let mut ordered: Vec<&Commit> = r.orphan_commits.iter().collect();
    ordered.reverse();

    for c in ordered {
        let slug = slugify(&c.subject, &c.hash);
        let prompt = shell_single_quote(&c.subject);
        let hash = &c.hash;
        println!("# {} — {}", &c.hash[..c.hash.len().min(12)], c.subject);
        println!("agentrail add \\");
        println!("  --slug {slug} \\");
        println!("  --prompt {prompt} \\");
        println!("  --commit {hash}");
        println!();
    }

    if !r.orphan_steps.is_empty() {
        println!("# ---");
        println!("# Orphan steps (no matching commit in current history):");
        for s in &r.orphan_steps {
            println!("#   {:03}-{}  {}", s.number, s.slug, s.description);
        }
        println!("# No command emitted — investigate whether the commit was rebased");
        println!("# away, squashed into another commit, or never made.");
        println!();
    }

    if !r.uncommitted.is_empty() {
        println!("# ---");
        println!("# Working tree has uncommitted changes (not added as steps):");
        for line in &r.uncommitted {
            println!("#   {line}");
        }
        println!();
    }
}

fn short_hash(h: &str) -> &str {
    &h[..h.len().min(12)]
}

fn escape_md(s: &str) -> String {
    s.replace('|', "\\|").replace('\n', " ")
}

/// Turn a commit subject into a kebab-case slug. Strips common
/// conventional-commit prefixes. Falls back to short hash if empty.
fn slugify(subject: &str, hash: &str) -> String {
    let cleaned = strip_conventional_prefix(subject);
    let mut out = String::new();
    let mut prev_dash = true;
    for ch in cleaned.chars() {
        if ch.is_ascii_alphanumeric() {
            out.push(ch.to_ascii_lowercase());
            prev_dash = false;
        } else if !prev_dash {
            out.push('-');
            prev_dash = true;
        }
    }
    while out.ends_with('-') {
        out.pop();
    }
    const MAX: usize = 40;
    if out.len() > MAX {
        out.truncate(MAX);
        while out.ends_with('-') {
            out.pop();
        }
    }
    if out.is_empty() {
        format!("commit-{}", &hash[..hash.len().min(7)])
    } else {
        out
    }
}

fn strip_conventional_prefix(s: &str) -> &str {
    if let Some(idx) = s.find(':')
        && idx < 20
    {
        let prefix = &s[..idx];
        if prefix
            .chars()
            .all(|c| c.is_ascii_alphabetic() || c == '(' || c == ')' || c == '!')
        {
            return s[idx + 1..].trim_start();
        }
    }
    s
}

fn shell_single_quote(s: &str) -> String {
    // Wrap in single quotes; escape embedded single quotes via '\''
    let escaped = s.replace('\'', "'\\''");
    format!("'{escaped}'")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slugify_strips_conventional_prefix() {
        assert_eq!(slugify("feat: add login flow", "abc123"), "add-login-flow");
        assert_eq!(
            slugify("fix(auth): stop dropping session", "abc123"),
            "stop-dropping-session"
        );
    }

    #[test]
    fn slugify_handles_plain_subject() {
        assert_eq!(
            slugify("Refactor the widgets", "abc"),
            "refactor-the-widgets"
        );
    }

    #[test]
    fn slugify_truncates_long_subjects() {
        let long = "a".repeat(100);
        assert!(slugify(&long, "h").len() <= 40);
    }

    #[test]
    fn slugify_falls_back_to_hash() {
        assert_eq!(slugify("!!!", "abc1234"), "commit-abc1234");
    }

    #[test]
    fn shell_quote_escapes_single_quotes() {
        assert_eq!(shell_single_quote("it's fine"), "'it'\\''s fine'");
    }
}
