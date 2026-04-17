use agentrail_core::StepStatus;
use agentrail_core::error::{Error, Result};
use agentrail_store::{saga, step};
use std::path::Path;

/// Reopen a completed or blocked step: transitions it back to InProgress,
/// clears `completed_at`, and resets the saga cursor to that step.
/// Recorded `commits` are preserved so the git-history linkage stays intact.
pub fn run(saga_path: &Path, number: u32) -> Result<()> {
    let mut config = saga::load_saga(saga_path)?;
    let saga_dir = saga::saga_dir(saga_path);

    let step_dir = step::find_step_dir(&saga_dir, number)?;
    let mut step_cfg = step::load_step(&step_dir)?;

    match step_cfg.status {
        StepStatus::Completed | StepStatus::Blocked => {}
        _ => {
            return Err(Error::Other(format!(
                "step {:03}-{} is {} — only completed or blocked steps can be reopened",
                step_cfg.number, step_cfg.slug, step_cfg.status
            )));
        }
    }

    step::transition_step(&mut step_cfg, StepStatus::InProgress)?;
    step::save_step(&step_dir, &step_cfg)?;

    config.current_step = number;
    if config.status == agentrail_core::SagaStatus::Completed {
        config.status = agentrail_core::SagaStatus::Active;
    }
    saga::save_saga(saga_path, &config)?;

    println!(
        "Reopened step {:03}-{} (in-progress).",
        number, step_cfg.slug
    );
    Ok(())
}
