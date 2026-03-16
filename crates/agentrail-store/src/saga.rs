use agentrail_core::error::{Error, Result};
use agentrail_core::{SagaConfig, SagaStatus};
use std::path::{Path, PathBuf};

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
    };

    save_saga(path, &config)?;

    let plan_path = path.join(&plan_file);
    std::fs::write(&plan_path, plan_content)?;

    Ok(())
}
