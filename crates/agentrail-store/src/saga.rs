use agentrail_core::error::{Error, Result};
use agentrail_core::{SagaConfig, SagaStatus};
use std::path::{Path, PathBuf};

/// Adjust the saga cursor after `agentrail insert` places a new step at
/// `new_step`.
///
/// Preemption semantic: if the inserted step lands at or before the current
/// cursor, focus follows the new arrival (the cursor "sees" the blocker
/// first). Otherwise the cursor's original step is unaffected — insert only
/// shifts steps at positions ≥ `new_step`, so a cursor earlier than
/// `new_step` keeps its number without needing to track identity.
pub fn cursor_after_insert(current: u32, new_step: u32) -> u32 {
    if current == 0 {
        return 0;
    }
    if new_step <= current {
        new_step
    } else {
        current
    }
}

/// Adjust the saga cursor after a move_step operation.
///
/// Cases (in order):
/// 1. Cursor was on the moved step → follows it to `to` (identity).
/// 2. Preemption: the moved step was behind the cursor (`from > current`)
///    and now lands at or ahead of the cursor's identity-shifted slot.
///    Focus jumps to the new arrival at `to`. Only backward moves can
///    trigger preemption — a forward move from `from > current` leaves
///    `to > from > current`, so the moved step stays behind.
/// 3. Otherwise, the cursor tracks the intervening shift by identity.
pub fn cursor_after_move(current: u32, from: u32, to: u32) -> u32 {
    if current == 0 || from == to {
        return current;
    }
    if current == from {
        return to;
    }
    let identity = if from < to {
        if current > from && current <= to {
            current - 1
        } else {
            current
        }
    } else if current >= to && current < from {
        current + 1
    } else {
        current
    };
    if from > current && to <= identity {
        return to;
    }
    identity
}

const DIR_NAME: &str = ".agentrail";

pub fn saga_dir(path: &Path) -> PathBuf {
    path.join(DIR_NAME)
}

pub fn saga_exists(path: &Path) -> bool {
    saga_dir(path).join("saga.toml").is_file()
}

pub fn load_saga(path: &Path) -> Result<SagaConfig> {
    let file = saga_dir(path).join("saga.toml");
    if !file.is_file() {
        return Err(Error::SagaNotFound {
            path: path.to_path_buf(),
        });
    }
    let content = std::fs::read_to_string(&file)?;
    Ok(toml::from_str(&content)?)
}

pub fn save_saga(path: &Path, config: &SagaConfig) -> Result<()> {
    let file = saga_dir(path).join("saga.toml");
    let content = toml::to_string_pretty(config)?;
    std::fs::write(&file, content)?;
    Ok(())
}

pub fn init_saga(path: &Path, name: &str, plan_content: &str) -> Result<()> {
    let dir = saga_dir(path);
    if dir.join("saga.toml").is_file() {
        return Err(Error::SagaAlreadyExists {
            path: path.to_path_buf(),
        });
    }

    std::fs::create_dir_all(dir.join("steps"))?;
    std::fs::create_dir_all(dir.join("trajectories"))?;
    std::fs::create_dir_all(dir.join("sessions"))?;

    let plan_file = format!("{DIR_NAME}/plan.md");
    let config = SagaConfig {
        name: name.to_string(),
        status: SagaStatus::Active,
        current_step: 0,
        created_at: agentrail_core::timestamp_iso(),
        plan_file: plan_file.clone(),
        retroactive: false,
    };

    save_saga(path, &config)?;

    let plan_path = path.join(&plan_file);
    std::fs::write(&plan_path, plan_content)?;

    Ok(())
}
