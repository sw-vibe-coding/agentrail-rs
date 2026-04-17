use agentrail_core::StepRole;
use agentrail_core::error::Result;
use agentrail_store::{saga, step};
use std::path::Path;

/// Insert a new pending step at position `after + 1`, shifting any
/// pending/in-progress steps after that point up by one.
pub fn run(
    saga_path: &Path,
    after: u32,
    slug: &str,
    prompt_raw: &str,
    role: &str,
    task_type: Option<&str>,
) -> Result<()> {
    let mut config = saga::load_saga(saga_path)?;
    let saga_dir = saga::saga_dir(saga_path);

    let prompt = agentrail_core::read_input(prompt_raw)?;

    let role = match role {
        "meta" => StepRole::Meta,
        "deterministic" => StepRole::Deterministic,
        "validation" => StepRole::Validation,
        _ => StepRole::Production,
    };

    let description = agentrail_core::truncate(&prompt, 80);

    let params = step::CreateStepParams {
        saga_dir: &saga_dir,
        number: 0, // overwritten by insert_after
        slug,
        prompt: &prompt,
        description: &description,
        role,
        context_files: &[],
        task_type,
        job_spec: None,
    };

    let new_dir = step::insert_after(after, &params)?;

    let new_number = after + 1;
    let old_current = config.current_step;
    let new_current = saga::cursor_after_insert(old_current, new_number);
    let preempted = new_current == new_number && old_current != 0 && old_current != new_number;
    if new_current != old_current {
        config.current_step = new_current;
    }
    if config.status == agentrail_core::SagaStatus::Completed {
        config.status = agentrail_core::SagaStatus::Active;
    }
    saga::save_saga(saga_path, &config)?;

    println!(
        "Inserted step {:03}-{} (shifted later steps up).",
        new_number, slug
    );
    if preempted {
        println!(
            "Focus moved to step {:03}-{} (preempted previous cursor at step {:03}).",
            new_number, slug, old_current
        );
    }
    let _ = new_dir;
    Ok(())
}
