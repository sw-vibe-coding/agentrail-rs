use agentrail_core::StepStatus;
use agentrail_core::error::{Error, Result};
use agentrail_store::{saga, step};
use std::path::Path;

/// Correct a step that is not done yet: replace its prompt (and the
/// description shown by `status`, the prompt's first 80 characters).
/// A completed step is history and is refused; reopen it first.
pub fn run(saga_path: &Path, number: u32, prompt_raw: &str) -> Result<()> {
    let prompt = agentrail_core::read_input(prompt_raw)?;
    if prompt.trim().is_empty() {
        return Err(Error::Other("the new prompt is empty".into()));
    }
    let saga_dir = saga::saga_dir(saga_path);
    let step_dir = step::find_step_dir(&saga_dir, number)?;
    let mut step_cfg = step::load_step(&step_dir)?;
    if step_cfg.status == StepStatus::Completed {
        return Err(Error::Other(format!(
            "step {:03}-{} is completed: its prompt is history (reopen it to change it)",
            step_cfg.number, step_cfg.slug
        )));
    }
    std::fs::write(step_dir.join("prompt.md"), &prompt)?;
    step_cfg.description = agentrail_core::truncate(&prompt, 80);
    step::save_step(&step_dir, &step_cfg)?;
    println!(
        "Edited the prompt of step {:03}-{}.",
        step_cfg.number, step_cfg.slug
    );
    Ok(())
}
