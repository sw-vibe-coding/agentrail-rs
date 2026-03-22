use agentrail_core::error::{Error, Result};
use agentrail_core::{StepConfig, StepRole, StepStatus};
use std::path::{Path, PathBuf};

/// Format step directory name: NNN-slug
fn step_dir_name(number: u32, slug: &str) -> String {
    format!("{:03}-{}", number, slug)
}

/// Get the step directory path within the saga dir.
pub fn step_dir(saga_dir: &Path, number: u32, slug: &str) -> PathBuf {
    saga_dir.join("steps").join(step_dir_name(number, slug))
}

/// Find a step directory by number (slug may vary).
pub fn find_step_dir(saga_dir: &Path, number: u32) -> Result<PathBuf> {
    let steps_dir = saga_dir.join("steps");
    if !steps_dir.is_dir() {
        return Err(Error::NoSteps);
    }
    let prefix = format!("{:03}-", number);
    for entry in std::fs::read_dir(&steps_dir)? {
        let entry = entry?;
        if let Some(name) = entry.file_name().to_str()
            && name.starts_with(&prefix)
        {
            return Ok(entry.path());
        }
    }
    Err(Error::NoCurrentStep)
}

pub fn load_step(step_dir: &Path) -> Result<StepConfig> {
    let file = step_dir.join("step.toml");
    let content = std::fs::read_to_string(&file)?;
    Ok(toml::from_str(&content)?)
}

pub fn save_step(step_dir: &Path, config: &StepConfig) -> Result<()> {
    let file = step_dir.join("step.toml");
    let content = toml::to_string_pretty(config)?;
    std::fs::write(&file, content)?;
    Ok(())
}

pub struct CreateStepParams<'a> {
    pub saga_dir: &'a Path,
    pub number: u32,
    pub slug: &'a str,
    pub prompt: &'a str,
    pub description: &'a str,
    pub role: StepRole,
    pub context_files: &'a [String],
    pub task_type: Option<&'a str>,
    pub job_spec: Option<agentrail_core::JobSpec>,
}

pub fn create_step(p: &CreateStepParams<'_>) -> Result<PathBuf> {
    let saga_dir = p.saga_dir;
    let number = p.number;
    let slug = p.slug;
    let prompt = p.prompt;
    let description = p.description;
    let role = p.role.clone();
    let context_files = p.context_files;
    let task_type = p.task_type;
    let dir = step_dir(saga_dir, number, slug);
    std::fs::create_dir_all(&dir)?;

    let config = StepConfig {
        number,
        slug: slug.to_string(),
        status: StepStatus::Pending,
        description: description.to_string(),
        role,
        context_files: context_files.to_vec(),
        created_at: agentrail_core::timestamp_iso(),
        completed_at: None,
        transcript_file: None,
        job_spec: p.job_spec.clone(),
        packet_file: None,
        task_type: task_type.map(|s| s.to_string()),
    };

    save_step(&dir, &config)?;
    std::fs::write(dir.join("prompt.md"), prompt)?;

    Ok(dir)
}

pub fn transition_step(config: &mut StepConfig, to: StepStatus) -> Result<()> {
    let valid = matches!(
        (&config.status, &to),
        (StepStatus::Pending, StepStatus::InProgress)
            | (StepStatus::InProgress, StepStatus::Completed)
            | (StepStatus::InProgress, StepStatus::Blocked)
    );

    if !valid {
        return Err(Error::InvalidStepTransition {
            from: config.status.to_string(),
            to: to.to_string(),
        });
    }

    if to == StepStatus::Completed {
        config.completed_at = Some(agentrail_core::timestamp_iso());
    }

    config.status = to;
    Ok(())
}

/// List all steps sorted by number.
pub fn list_steps(saga_dir: &Path) -> Result<Vec<(PathBuf, StepConfig)>> {
    let steps_dir = saga_dir.join("steps");
    if !steps_dir.is_dir() {
        return Ok(vec![]);
    }

    let mut steps = Vec::new();
    for entry in std::fs::read_dir(&steps_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let step_toml = entry.path().join("step.toml");
            if step_toml.is_file() {
                let config = load_step(&entry.path())?;
                steps.push((entry.path(), config));
            }
        }
    }

    steps.sort_by_key(|(_, c)| c.number);
    Ok(steps)
}

/// Save summary to a step directory.
pub fn save_summary(step_dir: &Path, content: &str) -> Result<()> {
    std::fs::write(step_dir.join("summary.md"), content)?;
    Ok(())
}
