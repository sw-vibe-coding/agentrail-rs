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
        commits: Vec::new(),
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
            | (StepStatus::Completed, StepStatus::InProgress)
            | (StepStatus::Blocked, StepStatus::InProgress)
    );

    if !valid {
        return Err(Error::InvalidStepTransition {
            from: config.status.to_string(),
            to: to.to_string(),
        });
    }

    match to {
        StepStatus::Completed => {
            config.completed_at = Some(agentrail_core::timestamp_iso());
        }
        StepStatus::InProgress => {
            // Reopen: clear the completion timestamp but keep `commits` so
            // the original git-history linkage is preserved.
            config.completed_at = None;
        }
        _ => {}
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

/// Shift every step with number >= `from` by `delta` (typically +1 or -1).
///
/// Uses a two-phase rename (each dir -> `.tmp.<n>-<slug>` -> final) so
/// intermediate collisions don't corrupt the layout when shifting up.
/// Refuses if any affected step is `Completed` — completed steps anchor
/// git-tracked history and must not be renumbered.
pub fn shift_tail(saga_dir: &Path, from: u32, delta: i32) -> Result<()> {
    if delta == 0 {
        return Ok(());
    }

    let all = list_steps(saga_dir)?;
    let affected: Vec<_> = all.into_iter().filter(|(_, s)| s.number >= from).collect();
    if affected.is_empty() {
        return Ok(());
    }

    for (_, s) in &affected {
        if s.status == StepStatus::Completed {
            return Err(Error::Other(format!(
                "cannot shift completed step {:03}-{}",
                s.number, s.slug
            )));
        }
        let new_number = s.number as i64 + delta as i64;
        if new_number < 1 {
            return Err(Error::Other(format!(
                "shift would drop step {:03}-{} below 1",
                s.number, s.slug
            )));
        }
    }

    let steps_dir = saga_dir.join("steps");

    // Phase 1: move every affected dir to a temp name keyed by its old number.
    for (path, s) in &affected {
        let tmp = steps_dir.join(format!(".tmp.{:03}-{}", s.number, s.slug));
        std::fs::rename(path, &tmp)?;
    }

    // Phase 2: move each temp to its final number and rewrite step.toml.
    for (_, s) in &affected {
        let tmp = steps_dir.join(format!(".tmp.{:03}-{}", s.number, s.slug));
        let new_number = (s.number as i64 + delta as i64) as u32;
        let final_dir = steps_dir.join(format!("{:03}-{}", new_number, s.slug));
        std::fs::rename(&tmp, &final_dir)?;
        let mut cfg = load_step(&final_dir)?;
        cfg.number = new_number;
        save_step(&final_dir, &cfg)?;
    }

    Ok(())
}

/// Insert a new pending step at position `after + 1`, shifting subsequent
/// pending/in-progress steps up by one. Refuses if any step in the shifted
/// range is `Completed`.
pub fn insert_after(after: u32, params: &CreateStepParams<'_>) -> Result<PathBuf> {
    shift_tail(params.saga_dir, after + 1, 1)?;
    let p = CreateStepParams {
        saga_dir: params.saga_dir,
        number: after + 1,
        slug: params.slug,
        prompt: params.prompt,
        description: params.description,
        role: params.role.clone(),
        context_files: params.context_files,
        task_type: params.task_type,
        job_spec: params.job_spec.clone(),
    };
    create_step(&p)
}

/// Move the step currently at `from` to position `to`, shifting the
/// intervening steps by one in the opposite direction. Refuses if the
/// source or any step in the swept range is `Completed`.
pub fn move_step(saga_dir: &Path, from: u32, to: u32) -> Result<()> {
    if from == to {
        return Ok(());
    }

    let all = list_steps(saga_dir)?;
    let (source_path, source_cfg) = all
        .iter()
        .find(|(_, s)| s.number == from)
        .ok_or_else(|| Error::Other(format!("no step at position {:03}", from)))?
        .clone();

    if source_cfg.status == StepStatus::Completed {
        return Err(Error::Other(format!(
            "cannot move completed step {:03}-{}",
            source_cfg.number, source_cfg.slug
        )));
    }

    let max_num = all.iter().map(|(_, s)| s.number).max().unwrap_or(0);
    if to < 1 || to > max_num {
        return Err(Error::Other(format!(
            "target position {:03} out of range (1..={:03})",
            to, max_num
        )));
    }

    // The intervening steps that will shift by ±1.
    let (lo, hi, delta): (u32, u32, i32) = if from < to {
        (from + 1, to, -1)
    } else {
        (to, from - 1, 1)
    };

    let affected: Vec<_> = all
        .iter()
        .filter(|(_, s)| s.number >= lo && s.number <= hi)
        .cloned()
        .collect();

    for (_, s) in &affected {
        if s.status == StepStatus::Completed {
            return Err(Error::Other(format!(
                "cannot move: intervening step {:03}-{} is completed",
                s.number, s.slug
            )));
        }
    }

    let steps_dir = saga_dir.join("steps");

    // Phase 1: park the source out of the way.
    let source_tmp = steps_dir.join(format!(".tmp.src-{:03}-{}", from, source_cfg.slug));
    std::fs::rename(&source_path, &source_tmp)?;

    // Phase 2: park every intervening step at a temp keyed by old number.
    for (path, s) in &affected {
        let tmp = steps_dir.join(format!(".tmp.mov-{:03}-{}", s.number, s.slug));
        std::fs::rename(path, &tmp)?;
    }

    // Phase 3: drop intervening steps into their new positions.
    for (_, s) in &affected {
        let tmp = steps_dir.join(format!(".tmp.mov-{:03}-{}", s.number, s.slug));
        let new_number = (s.number as i64 + delta as i64) as u32;
        let final_dir = steps_dir.join(format!("{:03}-{}", new_number, s.slug));
        std::fs::rename(&tmp, &final_dir)?;
        let mut cfg = load_step(&final_dir)?;
        cfg.number = new_number;
        save_step(&final_dir, &cfg)?;
    }

    // Phase 4: drop the source into `to`.
    let final_source = steps_dir.join(format!("{:03}-{}", to, source_cfg.slug));
    std::fs::rename(&source_tmp, &final_source)?;
    let mut cfg = load_step(&final_source)?;
    cfg.number = to;
    save_step(&final_source, &cfg)?;

    Ok(())
}
