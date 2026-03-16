use agentrail_core::error::Result;
use agentrail_store::{saga, step};
use std::path::Path;

pub fn run(saga_path: &Path) -> Result<()> {
    let config = saga::load_saga(saga_path)?;
    let saga_dir = saga::saga_dir(saga_path);

    println!("Saga: {}", config.name);
    println!("Status: {}", config.status);
    println!("Current step: {}", config.current_step);
    println!("Created: {}", config.created_at);

    let steps = step::list_steps(&saga_dir)?;
    if steps.is_empty() {
        println!("\nNo steps defined yet.");
    } else {
        println!("\nSteps:");
        for (_, s) in &steps {
            let marker = match s.status {
                agentrail_core::StepStatus::Completed => "x",
                agentrail_core::StepStatus::InProgress => ">",
                agentrail_core::StepStatus::Blocked => "!",
                agentrail_core::StepStatus::Pending => " ",
            };
            println!(
                "  [{}] {:03}-{} [{}]: {}",
                marker, s.number, s.slug, s.role, s.description
            );
        }
    }

    Ok(())
}
