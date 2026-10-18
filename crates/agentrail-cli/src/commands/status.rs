use agentrail_core::error::Result;
use agentrail_store::{instructions, saga, step};
use std::path::Path;

pub fn run(saga_path: &Path) -> Result<()> {
    let config = saga::load_saga(saga_path)?;
    let saga_dir = saga::saga_dir(saga_path);

    println!("Saga: {}", config.name);
    println!("Status: {}", config.status);
    println!("Current step: {}", config.current_step);
    println!("Created: {}", config.created_at);

    let steps = step::list_steps(&saga_dir)?;
    if steps.is_empty() {
        println!("\nNo steps defined yet.");
    } else {
        println!("\nSteps:");
        for (_, s) in &steps {
            let marker = match s.status {
                agentrail_core::StepStatus::Completed => "x",
                agentrail_core::StepStatus::InProgress => ">",
                agentrail_core::StepStatus::Blocked => "!",
                agentrail_core::StepStatus::Pending => " ",
            };
            let marker =
                if s.reserved_for.is_some() && s.status != agentrail_core::StepStatus::Completed {
                    "r"
                } else {
                    marker
                };
            let owner = s
                .reserved_for
                .as_deref()
                .map(|o| format!(" (reserved for {o})"))
                .unwrap_or_default();
            println!(
                "  [{}] {:03}-{} [{}]: {}{}",
                marker, s.number, s.slug, s.role, s.description, owner
            );
        }
    }

    print_stranded(&saga_dir, config.current_step)?;

    // Read-only freshness warning — never auto-applies from `status`.
    if let Ok(Some(msg)) = instructions::freshness_warning(saga_path) {
        println!();
        println!("{msg}");
    }

    Ok(())
}

/// Warn about pending steps the cursor has passed: nothing will bring
/// the cursor back to them, so they stay pending forever unless closed,
/// reserved or reordered forward.
pub fn print_stranded(saga_dir: &Path, current: u32) -> Result<()> {
    let stranded = step::stranded(saga_dir, current)?;
    if stranded.is_empty() {
        return Ok(());
    }
    println!();
    println!(
        "Stranded: {} pending step(s) behind the cursor ({current:03}):",
        stranded.len()
    );
    for s in &stranded {
        println!("  {:03}-{}", s.number, s.slug);
    }
    println!("  Done elsewhere?  agentrail close N --summary ... --commit REF | --elsewhere WHERE");
    println!("  Someone else's?  agentrail reserve N --for OWNER");
    println!("  Still to do?     agentrail reorder N --to M (if no completed step lies between)");
    Ok(())
}
