use agentrail_core::error::{Error, Result};
use agentrail_store::{saga, step};
use std::path::Path;

pub fn run(saga_path: &Path) -> Result<()> {
    let config = saga::load_saga(saga_path)?;
    let saga_dir = saga::saga_dir(saga_path);

    if config.current_step == 0 {
        return Err(Error::NoCurrentStep);
    }

    let step_dir = step::find_step_dir(&saga_dir, config.current_step)?;
    let mut step_config = step::load_step(&step_dir)?;

    step::transition_step(&mut step_config, agentrail_core::StepStatus::InProgress)?;
    step::save_step(&step_dir, &step_config)?;

    println!(
        "Step {:03}-{} is now in-progress.",
        step_config.number, step_config.slug
    );
    Ok(())
}
