use agentrail_core::error::Result;
use agentrail_core::{SagaStatus, StepRole, StepStatus};
use agentrail_store::{saga, session, step};
use std::path::Path;

pub struct CompleteArgs<'a> {
    pub summary: Option<&'a str>,
    pub next_slug: Option<&'a str>,
    pub next_prompt: Option<&'a str>,
    pub next_context: Vec<String>,
    pub next_role: &'a str,
    pub next_task_type: Option<&'a str>,
    pub planned: Vec<String>,
    pub done: bool,
}

pub fn run(saga_path: &Path, args: &CompleteArgs<'_>) -> Result<()> {
    let mut config = saga::load_saga(saga_path)?;
    let saga_dir = saga::saga_dir(saga_path);

    // Try to snapshot session (non-fatal, silent when no session exists)
    if let Ok((path, lines)) = session::snapshot_session(&saga_dir, saga_path)
        && lines > 0
    {
        eprintln!("Snapshotted {} new lines to {}", lines, path.display());
    }

    // Read summary
    let summary_text = match args.summary {
        Some(raw) => agentrail_core::read_input(raw)?,
        None => String::new(),
    };

    // Complete current step (or handle step 0)
    if config.current_step == 0 {
        // Step 0: just save the summary at the saga level
        if !summary_text.is_empty() {
            let summary_path = saga_dir.join("step0-summary.md");
            std::fs::write(&summary_path, &summary_text)?;
            println!("Saved step 0 summary.");
        }
    } else {
        // Find and complete the current step
        let step_dir = step::find_step_dir(&saga_dir, config.current_step)?;
        let mut step_config = step::load_step(&step_dir)?;

        // Auto-begin if still pending
        if step_config.status == StepStatus::Pending {
            step::transition_step(&mut step_config, StepStatus::InProgress)?;
        }

        step::transition_step(&mut step_config, StepStatus::Completed)?;
        step::save_step(&step_dir, &step_config)?;

        if !summary_text.is_empty() {
            step::save_summary(&step_dir, &summary_text)?;
        }

        println!(
            "Completed step {:03}-{}.",
            step_config.number, step_config.slug
        );
    }

    if args.done {
        config.status = SagaStatus::Completed;
        saga::save_saga(saga_path, &config)?;
        println!("Saga '{}' marked as complete.", config.name);
        return Ok(());
    }

    // Create next step if specified
    if let (Some(slug), Some(prompt_raw)) = (args.next_slug, args.next_prompt) {
        let prompt = agentrail_core::read_input(prompt_raw)?;
        let next_number = config.current_step + 1;
        let role = parse_role(args.next_role);

        let description = agentrail_core::truncate(&prompt, 80);
        step::create_step(&step::CreateStepParams {
            saga_dir: &saga_dir,
            number: next_number,
            slug,
            prompt: &prompt,
            description: &description,
            role,
            context_files: &args.next_context,
            task_type: args.next_task_type,
        })?;

        config.current_step = next_number;
        saga::save_saga(saga_path, &config)?;

        println!("Created step {:03}-{}.", next_number, slug);
    }

    // Create planned steps as pending placeholders
    let base = config.current_step;
    for (i, planned) in args.planned.iter().enumerate() {
        let number = base + 1 + i as u32;
        let (slug, desc) = match planned.split_once(": ") {
            Some((s, d)) => (s.trim(), d.trim()),
            None => (planned.trim(), planned.trim()),
        };
        step::create_step(&step::CreateStepParams {
            saga_dir: &saga_dir,
            number,
            slug,
            prompt: "",
            description: desc,
            role: StepRole::Legacy,
            context_files: &[],
            task_type: None,
        })?;
        println!("Planned step {:03}-{}.", number, slug);
    }

    Ok(())
}

fn parse_role(s: &str) -> StepRole {
    match s {
        "meta" => StepRole::Meta,
        "production" => StepRole::Production,
        "deterministic" => StepRole::Deterministic,
        "validation" => StepRole::Validation,
        _ => StepRole::Legacy,
    }
}
