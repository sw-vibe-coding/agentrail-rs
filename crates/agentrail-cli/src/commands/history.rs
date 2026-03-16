use agentrail_core::error::Result;
use agentrail_store::{saga, step};
use std::path::Path;

pub fn run(saga_path: &Path) -> Result<()> {
    let saga_dir = saga::saga_dir(saga_path);
    let steps = step::list_steps(&saga_dir)?;

    if steps.is_empty() {
        println!("No steps yet.");
        return Ok(());
    }

    for (dir, s) in &steps {
        println!("{:03}-{} [{}] ({})", s.number, s.slug, s.role, s.status);
        println!("  {}", s.description);

        let summary_path = dir.join("summary.md");
        if summary_path.is_file() {
            let summary = std::fs::read_to_string(&summary_path)?;
            let trimmed = summary.trim();
            if !trimmed.is_empty() {
                println!("  Summary: {}", agentrail_core::truncate(trimmed, 120));
            }
        }
        println!();
    }

    Ok(())
}
