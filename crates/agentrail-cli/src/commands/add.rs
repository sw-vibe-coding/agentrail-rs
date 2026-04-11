use agentrail_core::StepRole;
use agentrail_core::error::Result;
use agentrail_store::{git_history, saga, step};
use std::path::Path;

/// Add a new step to the saga without completing the current one.
/// Used for maintenance mode and ad-hoc task addition.
pub fn run(
    saga_path: &Path,
    slug: &str,
    prompt_raw: &str,
    role: &str,
    task_type: Option<&str>,
    commits: &[String],
) -> Result<()> {
    let config = saga::load_saga(saga_path)?;
    let saga_dir = saga::saga_dir(saga_path);

    let prompt = agentrail_core::read_input(prompt_raw)?;

    // Find the highest existing step number
    let steps = step::list_steps(&saga_dir)?;
    let max_number = steps.iter().map(|(_, s)| s.number).max().unwrap_or(0);
    let new_number = max_number.max(config.current_step) + 1;

    let role = match role {
        "meta" => StepRole::Meta,
        "production" => StepRole::Production,
        "deterministic" => StepRole::Deterministic,
        "validation" => StepRole::Validation,
        _ => StepRole::Production,
    };

    let description = agentrail_core::truncate(&prompt, 80);
    let step_dir = step::create_step(&step::CreateStepParams {
        saga_dir: &saga_dir,
        number: new_number,
        slug,
        prompt: &prompt,
        description: &description,
        role,
        context_files: &[],
        task_type,
        job_spec: None,
    })?;

    if !commits.is_empty() {
        // Resolve every passed reference to a full 40-char SHA so that
        // `agentrail audit` can match steps against git history exactly.
        // Short hashes, tags, and `HEAD~N` all work; bad refs fail loudly
        // here instead of silently producing orphan commits later.
        let mut resolved = Vec::with_capacity(commits.len());
        for reference in commits {
            let sha = git_history::resolve_commit(saga_path, reference)?;
            if reference != &sha {
                println!("Resolved '{reference}' -> {sha}");
            }
            resolved.push(sha);
        }

        let mut step_cfg = step::load_step(&step_dir)?;
        for sha in resolved {
            if !step_cfg.commits.contains(&sha) {
                step_cfg.commits.push(sha);
            }
        }
        step::save_step(&step_dir, &step_cfg)?;
    }

    // If saga has no current step or current step is complete, advance
    if config.current_step == 0 || config.current_step < new_number {
        if let Ok(current_dir) = step::find_step_dir(&saga_dir, config.current_step) {
            let current = step::load_step(&current_dir)?;
            if current.status == agentrail_core::StepStatus::Completed
                || current.status == agentrail_core::StepStatus::Blocked
            {
                let mut config = config;
                config.current_step = new_number;
                config.status = agentrail_core::SagaStatus::Active;
                saga::save_saga(saga_path, &config)?;
                println!("Advanced to step {:03}-{}.", new_number, slug);
                return Ok(());
            }
        } else if config.current_step == 0 {
            let mut config = config;
            config.current_step = new_number;
            saga::save_saga(saga_path, &config)?;
            println!("Advanced to step {:03}-{}.", new_number, slug);
            return Ok(());
        }
    }

    println!("Added step {:03}-{}.", new_number, slug);
    Ok(())
}
