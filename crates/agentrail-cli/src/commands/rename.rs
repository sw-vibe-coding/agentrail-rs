use agentrail_core::error::{Error, Result};
use agentrail_store::{rename, saga, tracking};
use std::path::Path;

pub enum Action {
    /// Apply a lane prefix to the saga name and every step slug.
    Prefix {
        prefix: String,
        dry_run: bool,
        skip_saga: bool,
    },
    /// Rename a single step's slug.
    Step {
        number: u32,
        new_slug: String,
        dry_run: bool,
    },
    /// Rename the saga itself.
    Saga { name: String },
}

pub fn run(saga_path: &Path, action: Action) -> Result<()> {
    match action {
        Action::Prefix {
            prefix,
            dry_run,
            skip_saga,
        } => prefix_lane(saga_path, &prefix, dry_run, skip_saga),
        Action::Step {
            number,
            new_slug,
            dry_run,
        } => rename_step(saga_path, number, &new_slug, dry_run),
        Action::Saga { name } => rename_saga(saga_path, &name),
    }
}

fn prefix_lane(saga_path: &Path, raw_prefix: &str, dry_run: bool, skip_saga: bool) -> Result<()> {
    let prefix = rename::normalize_prefix(raw_prefix)?;
    let mut config = saga::load_saga(saga_path)?;
    let saga_dir = saga::saga_dir(saga_path);

    let plan = rename::plan_prefix(&saga_dir, &prefix)?;
    let new_name = if skip_saga {
        None
    } else {
        rename::prefixed_saga_name(&config.name, &prefix)
    };

    if plan.is_empty() && new_name.is_none() {
        println!("Already prefixed with \"{prefix}\" — nothing to do.");
        return Ok(());
    }

    print_plan(&config.name, new_name.as_deref(), &plan, dry_run);
    if dry_run {
        println!("\nDry run — nothing was changed. Re-run without --dry-run to apply.");
        return Ok(());
    }

    rename::apply(&saga_dir, &plan)?;
    if let Some(name) = new_name {
        config.name = name;
        saga::save_saga(saga_path, &config)?;
    }

    println!(
        "\nRenamed {} step(s). Step numbers, statuses, and recorded commits are unchanged.",
        plan.len()
    );
    print_git_reminder(saga_path);
    Ok(())
}

fn rename_step(saga_path: &Path, number: u32, new_slug: &str, dry_run: bool) -> Result<()> {
    let saga_dir = saga::saga_dir(saga_path);
    if !saga::saga_exists(saga_path) {
        return Err(Error::SagaNotFound {
            path: saga_path.to_path_buf(),
        });
    }

    let Some(r) = rename::plan_one(&saga_dir, number, new_slug)? else {
        println!("Step {number:03} is already named \"{new_slug}\" — nothing to do.");
        return Ok(());
    };

    println!("  {} -> {}", r.old_dir_name(), r.new_dir_name());
    if dry_run {
        println!("\nDry run — nothing was changed.");
        return Ok(());
    }

    rename::apply(&saga_dir, std::slice::from_ref(&r))?;
    println!("\nRenamed step {number:03}.");
    print_git_reminder(saga_path);
    Ok(())
}

fn rename_saga(saga_path: &Path, name: &str) -> Result<()> {
    let mut config = saga::load_saga(saga_path)?;
    if config.name == name {
        println!("Saga is already named \"{name}\" — nothing to do.");
        return Ok(());
    }
    println!("  saga: \"{}\" -> \"{}\"", config.name, name);
    config.name = name.to_string();
    saga::save_saga(saga_path, &config)?;
    print_git_reminder(saga_path);
    Ok(())
}

fn print_plan(old_name: &str, new_name: Option<&str>, plan: &[rename::StepRename], dry_run: bool) {
    println!(
        "{}",
        if dry_run {
            "Would rename:"
        } else {
            "Renaming:"
        }
    );
    if let Some(new) = new_name {
        println!("  saga: \"{old_name}\" -> \"{new}\"");
    }
    for r in plan {
        println!("  {} -> {}", r.old_dir_name(), r.new_dir_name());
    }
}

/// Directory renames leave the old paths staged as deletions and the new
/// ones untracked, so `git add -A` (not a plain `git add`) is what
/// records the move.
fn print_git_reminder(saga_path: &Path) {
    if tracking::check(saga_path).is_some() {
        println!(
            "\nCommit the rename so the other agent's merge sees it:\n  \
             git add -A .agentrail/\n  \
             git commit -m \"saga: rename steps into lane\""
        );
    }
}
