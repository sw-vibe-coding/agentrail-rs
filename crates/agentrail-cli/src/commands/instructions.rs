use agentrail_core::error::Result;
use agentrail_store::instructions::{
    self, ApplyOutcome, EMBEDDED_FRAGMENTS, EMBEDDED_PROFILES, StatusReport, TargetStatus,
};
use std::path::Path;

pub enum Action {
    Status,
    Apply,
    Diff,
    Show,
    List,
}

pub fn run(saga_path: &Path, action: Action) -> Result<u8> {
    match action {
        Action::Status => status(saga_path).map(|stale| if stale { 1 } else { 0 }),
        Action::Apply => apply(saga_path).map(|_| 0),
        Action::Diff => diff(saga_path).map(|differs| if differs { 1 } else { 0 }),
        Action::Show => show(saga_path).map(|_| 0),
        Action::List => list().map(|_| 0),
    }
}

fn status(saga_path: &Path) -> Result<bool> {
    let report = instructions::status(saga_path)?;
    print_status(&report);
    let any_stale = report.targets.iter().any(|t| {
        matches!(
            t.status,
            TargetStatus::Stale | TargetStatus::NoBlock | TargetStatus::Missing
        )
    });
    Ok(any_stale)
}

fn print_status(r: &StatusReport) {
    println!("Profile:        {}", r.profile);
    println!("Embedded hash:  {}", r.embedded_hash);
    match &r.lock {
        Some(lock) => {
            println!("Lock hash:      {}", lock.content_hash);
            println!("Last applied:   {}", lock.updated_at);
        }
        None => println!("Lock:           (none — never applied)"),
    }
    println!();

    if r.targets.is_empty() {
        println!("No targets resolved.");
        println!(
            "Hint: create CLAUDE.md or AGENTS.md, or set targets in \
             .agentrail/instruction-profile.toml."
        );
        return;
    }

    println!("Targets:");
    for t in &r.targets {
        let label = match t.status {
            TargetStatus::UpToDate => "up to date",
            TargetStatus::Stale => "STALE",
            TargetStatus::NoBlock => "no briefing block",
            TargetStatus::Missing => "missing",
        };
        let hash = t
            .current_hash
            .as_deref()
            .map(|h| format!(" [{h}]"))
            .unwrap_or_default();
        println!("  {label:<18} {}{}", t.target.display(), hash);
    }

    if r.targets.iter().any(|t| t.status == TargetStatus::Stale) {
        println!();
        println!("Run `agentrail instructions apply` to refresh stale targets.");
    } else if r
        .targets
        .iter()
        .any(|t| matches!(t.status, TargetStatus::NoBlock | TargetStatus::Missing))
    {
        println!();
        println!("Run `agentrail instructions apply` to install the briefing block.");
    }
}

fn apply(saga_path: &Path) -> Result<()> {
    let (profile, outcomes) = instructions::apply(saga_path)?;
    println!(
        "Applied briefing profile '{profile}' to {} target(s):",
        outcomes.len()
    );
    for o in &outcomes {
        let tag = describe_outcome(o);
        println!("  {tag:<18} {}", o.target.display());
    }
    println!();
    println!("Wrote .agentrail/instruction-lock.toml.");
    println!("Commit the changes (briefing edits are first-class project changes).");
    Ok(())
}

fn describe_outcome(o: &ApplyOutcome) -> &'static str {
    if o.created_file {
        "created"
    } else if o.changed && o.had_block {
        "updated"
    } else if o.changed {
        "added block"
    } else {
        "unchanged"
    }
}

fn diff(saga_path: &Path) -> Result<bool> {
    let (profile, targets) = instructions::resolve_targets(saga_path)?;
    let (_, exclude) = instructions::resolve_render_inputs(saga_path)?;
    let embedded_body = instructions::render_body(&profile, &exclude)?;
    let embedded_hash = instructions::fnv1a_hex(&embedded_body);

    if targets.is_empty() {
        println!("No targets resolved — nothing to diff.");
        return Ok(false);
    }

    let mut any_diff = false;
    for t in &targets {
        let existing = match std::fs::read_to_string(t) {
            Ok(s) => s,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                println!("--- {} (missing)", t.display());
                println!("+++ embedded ({embedded_hash})");
                println!("{embedded_body}");
                any_diff = true;
                continue;
            }
            Err(e) => return Err(e.into()),
        };

        let current_body = instructions::extract_body(&existing);
        match current_body {
            None => {
                println!("--- {} (no briefing block)", t.display());
                println!("+++ embedded ({embedded_hash})");
                println!("{embedded_body}");
                any_diff = true;
            }
            Some(body) => {
                let h = instructions::fnv1a_hex(&body);
                if h == embedded_hash {
                    println!("== {} up to date ({h})", t.display());
                    continue;
                }
                any_diff = true;
                println!("--- {} ({h})", t.display());
                println!("+++ embedded ({embedded_hash})");
                print_line_diff(&body, &embedded_body);
                println!();
            }
        }
    }

    if !any_diff {
        println!("All targets up to date.");
    }
    Ok(any_diff)
}

/// Minimal line-aware diff: walks both bodies in lockstep, prefixing
/// lines that are only in `current` with `-` and lines only in `embedded`
/// with `+`. Not a Myers diff — this is a simple alignment using
/// longest-common-subsequence over hashed lines.
fn print_line_diff(current: &str, embedded: &str) {
    let a: Vec<&str> = current.lines().collect();
    let b: Vec<&str> = embedded.lines().collect();
    let lcs = lcs_table(&a, &b);

    let mut i = a.len();
    let mut j = b.len();
    let mut ops: Vec<String> = Vec::new();
    while i > 0 || j > 0 {
        if i > 0 && j > 0 && a[i - 1] == b[j - 1] {
            ops.push(format!("  {}", a[i - 1]));
            i -= 1;
            j -= 1;
        } else if j > 0 && (i == 0 || lcs[i][j - 1] >= lcs[i - 1][j]) {
            ops.push(format!("+ {}", b[j - 1]));
            j -= 1;
        } else {
            ops.push(format!("- {}", a[i - 1]));
            i -= 1;
        }
    }
    for op in ops.iter().rev() {
        println!("{op}");
    }
}

fn lcs_table(a: &[&str], b: &[&str]) -> Vec<Vec<usize>> {
    let mut t = vec![vec![0usize; b.len() + 1]; a.len() + 1];
    for i in 1..=a.len() {
        for j in 1..=b.len() {
            t[i][j] = if a[i - 1] == b[j - 1] {
                t[i - 1][j - 1] + 1
            } else {
                t[i - 1][j].max(t[i][j - 1])
            };
        }
    }
    t
}

fn show(saga_path: &Path) -> Result<()> {
    // Saga-aware preview: render the body that `apply` would write here,
    // honoring profile + exclude from `.agentrail/instruction-profile.toml`.
    // Without a saga directory, this falls back to the embedded default
    // with no excludes.
    let (profile, exclude) = instructions::resolve_render_inputs(saga_path)?;
    let body = instructions::render_body(&profile, &exclude)?;
    println!("{body}");
    Ok(())
}

fn list() -> Result<()> {
    println!("Embedded profiles:");
    for (name, _) in EMBEDDED_PROFILES {
        println!("  {name}");
    }
    println!();
    println!("Embedded fragments:");
    for (path, content) in EMBEDDED_FRAGMENTS {
        println!("  {path}  ({} bytes)", content.len());
    }
    Ok(())
}
