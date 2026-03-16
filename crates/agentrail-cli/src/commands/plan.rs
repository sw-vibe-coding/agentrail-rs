use agentrail_core::error::Result;
use agentrail_store::saga;
use std::path::Path;

pub fn run(saga_path: &Path, update_raw: Option<&str>) -> Result<()> {
    let config = saga::load_saga(saga_path)?;
    let plan_path = saga_path.join(&config.plan_file);

    if let Some(raw) = update_raw {
        let content = agentrail_core::read_input(raw)?;
        std::fs::write(&plan_path, &content)?;
        println!("Plan updated.");
    } else {
        let content = std::fs::read_to_string(&plan_path)?;
        println!("{}", content.trim());
    }

    Ok(())
}
