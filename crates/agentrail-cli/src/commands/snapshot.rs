use agentrail_core::error::Result;
use agentrail_store::snapshot;
use std::path::Path;

pub struct SnapshotArgs {
    pub list: bool,
}

pub fn run(saga_path: &Path, args: &SnapshotArgs) -> Result<()> {
    if args.list {
        return list(saga_path);
    }
    take(saga_path)
}

fn take(saga_path: &Path) -> Result<()> {
    let info = snapshot::take(saga_path)?;
    println!("Snapshot created: {}", info.ref_name);
    println!("  Commit:         {}", info.commit);
    println!("  Paths captured: {}", info.captured_paths.join(", "));
    println!();
    println!("The snapshot is a proper git commit under a dedicated ref namespace,");
    println!("so it survives `git gc` and will not be garbage-collected.");
    println!();
    println!("Inspect it with:");
    println!("  git log {}", info.ref_name);
    println!("  git ls-tree -r {}", info.ref_name);
    println!();
    println!("Restore (to working tree) with:");
    println!(
        "  git restore --source={} -- .agentrail .agentrail-archive",
        info.ref_name
    );
    println!();
    println!("List all snapshots with: agentrail snapshot --list");
    Ok(())
}

fn list(saga_path: &Path) -> Result<()> {
    let entries = snapshot::list(saga_path)?;
    if entries.is_empty() {
        println!("No agentrail snapshots found.");
        println!("Create one with: agentrail snapshot");
        return Ok(());
    }
    println!("Agentrail snapshots (newest first):");
    println!();
    println!("| Timestamp | Ref | Commit |");
    println!("|---|---|---|");
    for e in &entries {
        println!("| {} | {} | {} |", e.timestamp, e.ref_name, &e.commit[..12]);
    }
    println!();
    println!("Restore an individual snapshot with:");
    println!("  git restore --source=<ref> -- .agentrail .agentrail-archive");
    Ok(())
}
