use agentrail_core::error::{Error, Result};
use agentrail_store::{session, shadow};
use std::path::{Path, PathBuf};

pub enum Action {
    /// Remember a shadow target for this repository.
    Set(String),
    /// Forget it.
    Unset,
    Status,
    Push {
        no_git_push: bool,
    },
    Pull {
        no_git_pull: bool,
    },
}

pub fn run(saga_path: &Path, action: Action) -> Result<u8> {
    let store = session::session_store_dir(saga_path)?;
    match action {
        Action::Set(path) => {
            let abs = shadow::set_target(&store, Path::new(&path))?;
            println!("Shadow store for this repository: {}", abs.display());
            if !abs.join(".git").exists() {
                println!("(a plain directory: `shadow push` will copy but not commit or push)");
            }
            println!("Push the session logs there with:  agentrail shadow push");
            Ok(0)
        }
        Action::Unset => {
            if shadow::clear_target(&store)? {
                println!("Shadow store forgotten. Nothing in it was changed.");
            } else {
                println!("No shadow store was set.");
            }
            Ok(0)
        }
        Action::Status => status(&store),
        Action::Push { no_git_push } => push(&store, !no_git_push),
        Action::Pull { no_git_pull } => pull(&store, !no_git_pull),
    }
}

fn target(store: &Path) -> Result<PathBuf> {
    shadow::configured_target(store).ok_or_else(|| {
        Error::Other(format!(
            "no shadow store is set for this repository.\n  \
             agentrail shadow set <dir>     (a directory, or a clone of a private git repository)\n  \
             or set {}=<dir>",
            shadow::SHADOW_ENV
        ))
    })
}

fn status(store: &Path) -> Result<u8> {
    println!("Session store: {}", store.display());
    let Some(target) = shadow::configured_target(store) else {
        println!("Shadow store:  not set  (agentrail shadow set <dir>)");
        return Ok(1);
    };
    println!("Shadow store:  {}", target.display());
    let entries = shadow::status(store, &target)?;
    if entries.is_empty() {
        println!("\nNo session logs in either.");
        return Ok(0);
    }
    println!();
    let mut behind = false;
    for e in &entries {
        let label = match e.state {
            shadow::State::InSync => "in sync    ",
            shadow::State::Differs => "to push    ",
            shadow::State::LocalOnly => "to push    ",
            shadow::State::ShadowOnly => "to pull    ",
        };
        behind |= matches!(e.state, shadow::State::Differs | shadow::State::LocalOnly);
        println!("  {label} {}", e.name);
    }
    Ok(if behind { 1 } else { 0 })
}

fn push(store: &Path, git_push: bool) -> Result<u8> {
    let target = target(store)?;
    let report = shadow::push(store, &target, git_push)?;
    for name in &report.written {
        println!("compressed {name}");
    }
    println!(
        "{} log(s) written, {} already current, in {}",
        report.written.len(),
        report.unchanged.len(),
        target.join("sessions").display()
    );
    if let Some(commit) = &report.committed {
        println!("Committed {commit} in the shadow repository.");
    }
    if report.pushed {
        println!("Pushed.");
    }
    if let Some(err) = &report.push_error {
        eprintln!("⚠ The shadow repository could not be pushed; the logs are committed locally.");
        eprintln!("  {err}");
        return Ok(1);
    }
    Ok(0)
}

fn pull(store: &Path, git_pull: bool) -> Result<u8> {
    let target = target(store)?;
    let report = shadow::pull(store, &target, git_pull)?;
    if let Some(err) = &report.pull_error {
        eprintln!("⚠ Could not update the shadow repository; restoring from what it already has.");
        eprintln!("  {err}");
    }
    for name in &report.restored {
        println!("restored   {name}");
    }
    for (name, why) in &report.failed {
        eprintln!("✖ {name}: {why}");
    }
    println!(
        "{} log(s) restored to {}, {} left as they were (local copy at least as long), {} failed.",
        report.restored.len(),
        store.display(),
        report.kept_local.len(),
        report.failed.len()
    );
    Ok(if report.failed.is_empty() { 0 } else { 1 })
}
