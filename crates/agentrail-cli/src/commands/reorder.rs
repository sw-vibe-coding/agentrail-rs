use agentrail_core::error::Result;
use agentrail_store::{saga, step};
use std::path::Path;

/// Move an existing pending/in-progress step to a new position.
pub fn run(saga_path: &Path, from: u32, to: u32) -> Result<()> {
    let mut config = saga::load_saga(saga_path)?;
    let saga_dir = saga::saga_dir(saga_path);

    step::move_step(&saga_dir, from, to)?;

    let old_current = config.current_step;
    let new_current = saga::cursor_after_move(old_current, from, to);
    // Preemption: a different step crossed over the cursor from behind and
    // now sits at `to`. `cursor_after_move` returns `to` both for identity
    // (old_current == from) and preemption (old_current != from); the latter
    // is the case worth announcing.
    let preempted =
        new_current == to && old_current != 0 && old_current != from && old_current != to;
    if new_current != old_current {
        config.current_step = new_current;
        saga::save_saga(saga_path, &config)?;
    }

    println!("Moved step {:03} -> {:03}.", from, to);
    if preempted {
        println!(
            "Focus moved to step {:03} (preempted previous cursor at step {:03}).",
            to, old_current
        );
    }
    Ok(())
}
