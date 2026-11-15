use agentrail_core::error::Result;
use agentrail_store::session;
use std::path::Path;

pub enum Action {
    /// Print the session store directory for this repository.
    Path,
    /// Move in-repo session logs to the store and untrack them.
    Migrate,
}

pub fn run(saga_path: &Path, action: Action) -> Result<()> {
    let store = session::session_store_dir(saga_path)?;
    match action {
        Action::Path => {
            println!("{}", store.display());
            Ok(())
        }
        Action::Migrate => migrate(saga_path, &store),
    }
}

fn migrate(saga_path: &Path, store: &Path) -> Result<()> {
    let report = session::migrate_in_repo_logs(saga_path, store)?;
    if report.moved.is_empty() && report.superseded.is_empty() && !report.untracked_from_git {
        println!("No session logs inside the repository. Nothing to do.");
        println!("Session store: {}", store.display());
        return Ok(());
    }

    for p in &report.moved {
        println!("moved      {}", p.display());
    }
    for p in &report.superseded {
        println!(
            "dropped    {} (the store already has a copy at least as long)",
            p.display()
        );
    }
    println!();
    println!("Session store: {}", store.display());

    if report.untracked_from_git {
        println!();
        println!("The logs were tracked by git; their removal is staged. Commit it:");
        println!("  git commit -m \"saga: move session logs out of the repository\"");
        println!();
        println!("Earlier commits still contain them. If the repository is public, or the");
        println!("logs made it large, removing them from history takes a rewrite");
        println!("(e.g. `git filter-repo --invert-paths --path .agentrail/sessions/`)");
        println!("and a force-push — a decision for the repository's owner, not a routine step.");
    }
    Ok(())
}
