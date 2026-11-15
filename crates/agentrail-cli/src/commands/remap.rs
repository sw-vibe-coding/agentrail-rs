use agentrail_core::error::{Error, Result};
use agentrail_store::{git_history, remap};
use std::path::Path;

pub struct RemapArgs<'a> {
    /// Path to a git filter-repo `commit-map` file.
    pub commit_map: Option<&'a str>,
    /// Hand-given old hashes (may be abbreviated), paired with `to`.
    pub from: &'a [String],
    /// Hand-given new commit references, resolved in this repository.
    pub to: &'a [String],
    pub dry_run: bool,
}

/// Returns exit code: 0 = every recorded commit exists (or not a git
/// repository), 1 = some recorded commits are still dangling.
pub fn run(saga_path: &Path, args: &RemapArgs<'_>) -> Result<u8> {
    if args.from.len() != args.to.len() {
        return Err(Error::Other(format!(
            "--from and --to come in pairs: got {} --from and {} --to",
            args.from.len(),
            args.to.len()
        )));
    }

    let mut mappings = Vec::new();
    if let Some(path) = args.commit_map {
        let text = std::fs::read_to_string(path)
            .map_err(|e| Error::Other(format!("cannot read commit map {path}: {e}")))?;
        mappings = remap::parse_commit_map(&text)?;
    }
    for (old, new) in args.from.iter().zip(args.to) {
        let resolved = git_history::resolve_commit(saga_path, new)?;
        mappings.push(remap::manual_mapping(old, &resolved)?);
    }

    let check_only = mappings.is_empty();
    let report = remap::remap(saga_path, &mappings, args.dry_run || check_only)?;

    let verb = if args.dry_run {
        "would rewrite"
    } else {
        "rewrote"
    };
    for r in &report.rewritten {
        println!("{verb}  {}: {} -> {}", r.step, short(&r.old), short(&r.new));
    }
    for d in &report.dropped {
        println!(
            "dropped  {}: {} was removed by the rewrite; left as recorded",
            d.step,
            short(&d.hash)
        );
    }
    for d in &report.dangling {
        println!("dangling {}: {} is not in this repository", d.step, d.hash);
    }

    println!();
    if check_only {
        println!(
            "Checked {} step(s): {} recorded commit(s) dangling.",
            report.steps_seen,
            report.dangling.len()
        );
        if !report.dangling.is_empty() {
            println!("  After git filter-repo:  agentrail remap .git/filter-repo/commit-map");
            println!("  After a squash/rebase:  agentrail remap --from <old-hash> --to <new-ref>");
        }
    } else {
        println!(
            "{} {} commit hash(es) across {} step(s) examined; {} dropped, {} still dangling.",
            if args.dry_run {
                "Would rewrite"
            } else {
                "Rewrote"
            },
            report.rewritten.len(),
            report.steps_seen,
            report.dropped.len(),
            report.dangling.len()
        );
        if !args.dry_run && !report.rewritten.is_empty() {
            println!("Commit the change:  git add -A .agentrail/ .agentrail-archive/");
        }
    }
    Ok(if report.dangling.is_empty() { 0 } else { 1 })
}

fn short(hash: &str) -> &str {
    &hash[..hash.len().min(12)]
}
