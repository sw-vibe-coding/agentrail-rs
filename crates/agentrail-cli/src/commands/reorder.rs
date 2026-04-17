use agentrail_core::error::Result;
use agentrail_store::{saga, step};
use std::path::Path;

/// Move an existing pending/in-progress step to a new position.
pub fn run(saga_path: &Path, from: u32, to: u32) -> Result<()> {
    let mut config = saga::load_saga(saga_path)?;
    let saga_dir = saga::saga_dir(saga_path);

    step::move_step(&saga_dir, from, to)?;

    let new_current = saga::cursor_after_move(config.current_step, from, to);
    if new_current != config.current_step {
        config.current_step = new_current;
        saga::save_saga(saga_path, &config)?;
    }

    println!("Moved step {:03} -> {:03}.", from, to);
    Ok(())
}
