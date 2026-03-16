use agentrail_core::StepStatus;
use agentrail_core::error::{Error, Result};
use agentrail_store::{saga, step};
use std::path::Path;

pub fn run(saga_path: &Path, reason: Option<&str>) -> Result<()> {
    let config = saga::load_saga(saga_path)?;
    let saga_dir = saga::saga_dir(saga_path);

    if config.current_step == 0 {
        return Err(Error::NoCurrentStep);
    }

    let step_dir = step::find_step_dir(&saga_dir, config.current_step)?;
    let mut step_config = step::load_step(&step_dir)?;

    // Auto-begin if still pending
    if step_config.status == StepStatus::Pending {
        step::transition_step(&mut step_config, StepStatus::InProgress)?;
    }

    step::transition_step(&mut step_config, StepStatus::Blocked)?;
    step::save_step(&step_dir, &step_config)?;

    if let Some(reason) = reason {
        let reason_text = agentrail_core::read_input(reason)?;
        step::save_summary(&step_dir, &format!("BLOCKED: {}", reason_text))?;
    }

    println!(
        "Step {:03}-{} marked as blocked.",
        step_config.number, step_config.slug
    );
    Ok(())
}
