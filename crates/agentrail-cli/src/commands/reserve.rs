use agentrail_core::StepStatus;
use agentrail_core::error::{Error, Result};
use agentrail_store::{saga, step};
use std::path::Path;

/// Reserve a step for someone else (another lane, branch or
/// repository): the cursor skips it, `complete` never lands on it, and
/// `status` shows who has it. Close it with `agentrail close` when the
/// owner reports it done, or `unreserve` it to take it back.
pub fn reserve(saga_path: &Path, number: u32, owner: &str) -> Result<()> {
    if owner.trim().is_empty() {
        return Err(Error::Other(
            "say who the step is reserved for (--for)".into(),
        ));
    }
    let mut config = saga::load_saga(saga_path)?;
    let saga_dir = saga::saga_dir(saga_path);
    let step_dir = step::find_step_dir(&saga_dir, number)?;
    let mut step_cfg = step::load_step(&step_dir)?;
    if step_cfg.status == StepStatus::Completed {
        return Err(Error::Other(format!(
            "step {:03}-{} is already completed",
            step_cfg.number, step_cfg.slug
        )));
    }
    step_cfg.reserved_for = Some(owner.to_string());
    step::save_step(&step_dir, &step_cfg)?;
    println!(
        "Reserved step {:03}-{} for {owner}.",
        step_cfg.number, step_cfg.slug
    );
    if number == config.current_step
        && let Some(next) = step::next_open_after(&saga_dir, number)?
    {
        config.current_step = next;
        saga::save_saga(saga_path, &config)?;
        println!("Cursor moved to the next open step, {next:03}.");
    }
    Ok(())
}

/// Take a reserved step back. The cursor does not move; a step behind it
/// is then reported as stranded by `status` until it is reordered
/// forward or closed.
pub fn unreserve(saga_path: &Path, number: u32) -> Result<()> {
    let saga_dir = saga::saga_dir(saga_path);
    let step_dir = step::find_step_dir(&saga_dir, number)?;
    let mut step_cfg = step::load_step(&step_dir)?;
    match step_cfg.reserved_for.take() {
        Some(owner) => {
            step::save_step(&step_dir, &step_cfg)?;
            println!(
                "Step {:03}-{} is no longer reserved (it was for {owner}).",
                step_cfg.number, step_cfg.slug
            );
            Ok(())
        }
        None => Err(Error::Other(format!(
            "step {:03}-{} is not reserved",
            step_cfg.number, step_cfg.slug
        ))),
    }
}
