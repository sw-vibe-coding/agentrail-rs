use agentrail_core::StepStatus;
use agentrail_core::error::{Error, Result};
use agentrail_store::{git_history, saga, step};
use std::path::Path;

pub struct CloseArgs<'a> {
    pub number: u32,
    /// Summary: text, file path, or "-" for stdin.
    pub summary: &'a str,
    /// Commits in this repository that did the work.
    pub commits: &'a [String],
    /// Where the work was done when it is not a commit here
    /// (`other-repo@abc1234`, `lane readable`, ...).
    pub elsewhere: Option<&'a str>,
}

/// Close a step out of order: mark a pending, in-progress or blocked
/// step completed without moving the cursor through it, recording where
/// the work actually is. For a step done in another lane, branch or
/// repository, or done before the saga noticed. Unlike `reopen` +
/// `complete`, it never touches another step and never records HEAD:
/// the caller says which commits (here) or which place (elsewhere).
pub fn run(saga_path: &Path, args: &CloseArgs<'_>) -> Result<()> {
    if args.commits.is_empty() && args.elsewhere.is_none() {
        return Err(Error::Other(
            "say where the work is: --commit <ref> (in this repository) or --elsewhere <where>"
                .into(),
        ));
    }
    let summary = agentrail_core::read_input(args.summary)?;
    if summary.trim().is_empty() {
        return Err(Error::Other("a closed step needs a summary".into()));
    }
    let mut config = saga::load_saga(saga_path)?;
    let saga_dir = saga::saga_dir(saga_path);
    let step_dir = step::find_step_dir(&saga_dir, args.number)?;
    let mut step_cfg = step::load_step(&step_dir)?;
    if step_cfg.status == StepStatus::Completed {
        return Err(Error::Other(format!(
            "step {:03}-{} is already completed (reopen it to change it)",
            step_cfg.number, step_cfg.slug
        )));
    }
    let mut resolved = Vec::with_capacity(args.commits.len());
    for reference in args.commits {
        resolved.push(git_history::resolve_commit(saga_path, reference)?);
    }

    if step_cfg.status != StepStatus::InProgress {
        step::transition_step(&mut step_cfg, StepStatus::InProgress)?;
    }
    step::transition_step(&mut step_cfg, StepStatus::Completed)?;
    for sha in resolved {
        if !step_cfg.commits.contains(&sha) {
            step_cfg.commits.push(sha);
        }
    }
    step_cfg.done_elsewhere = args.elsewhere.map(str::to_string);
    step_cfg.reserved_for = None;
    step::save_step(&step_dir, &step_cfg)?;
    step::save_summary(&step_dir, &summary)?;
    println!("Closed step {:03}-{}.", step_cfg.number, step_cfg.slug);

    if args.number == config.current_step {
        if let Some(next) = step::next_open_after(&saga_dir, args.number)? {
            config.current_step = next;
            saga::save_saga(saga_path, &config)?;
            println!("Cursor moved to the next open step, {next:03}.");
        }
    } else {
        println!("Cursor stays at {:03}.", config.current_step);
    }
    println!("Commit .agentrail/ so the next session sees it.");
    Ok(())
}
