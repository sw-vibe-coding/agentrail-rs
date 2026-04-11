use agentrail_core::error::Result;
use agentrail_store::saga;
use std::path::Path;

pub fn run(saga_path: &Path, name: &str, plan_raw: &str, retroactive: bool) -> Result<()> {
    let plan = agentrail_core::read_input(plan_raw)?;
    saga::init_saga(saga_path, name, &plan)?;

    if retroactive {
        let mut cfg = saga::load_saga(saga_path)?;
        cfg.retroactive = true;
        saga::save_saga(saga_path, &cfg)?;
    }

    let dir = saga::saga_dir(saga_path);
    println!(
        "Initialized{} saga '{}' at {}",
        if retroactive { " retroactive" } else { "" },
        name,
        dir.display()
    );
    println!();
    println!("Created:");
    println!("  {}/saga.toml", dir.display());
    println!("  {}/plan.md", dir.display());
    println!("  {}/steps/", dir.display());
    println!("  {}/trajectories/", dir.display());
    println!("  {}/sessions/", dir.display());
    println!();
    println!("The .agentrail/ directory is an append-only store.");
    println!("Do not manually edit files inside it.");
    Ok(())
}
