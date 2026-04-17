use agentrail_core::error::{Error, Result};
use agentrail_core::{SagaConfig, SagaStatus};
use std::path::{Path, PathBuf};

/// Adjust the saga cursor after a tail-shift starting at `from`.
///
/// If the cursor points at a step that got renumbered, it follows that step
/// to its new number. Used by `agentrail insert`.
pub fn cursor_after_shift(current: u32, from: u32, delta: i32) -> u32 {
    if current == 0 || current < from {
        return current;
    }
    let shifted = current as i64 + delta as i64;
    shifted.max(0) as u32
}

/// Adjust the saga cursor after a move_step operation. The cursor follows
/// the step by identity: if it was pointing at `from`, it now points at
/// `to`. Otherwise, it shifts with the intervening range.
pub fn cursor_after_move(current: u32, from: u32, to: u32) -> u32 {
    if current == 0 || from == to {
        return current;
    }
    if current == from {
        return to;
    }
    if from < to {
        if current > from && current <= to {
            current - 1
        } else {
            current
        }
    } else if current >= to && current < from {
        current + 1
    } else {
        current
    }
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
