use agentrail_core::error::Result;
use agentrail_core::{FailureMode, OutputContract, Procedure, Skill};
use agentrail_store::{saga, skill, trajectory};
use std::collections::HashMap;
use std::path::Path;

/// Distill trajectories for a task type into a skill document.
/// Implements the XSkill accumulation phase.
pub fn run(saga_path: &Path, task_type: &str) -> Result<()> {
    let saga_dir = saga::saga_dir(saga_path);
    let task_dir = saga_dir.join("trajectories").join(task_type);

    if !task_dir.is_dir() {
        println!("No trajectories found for task type '{task_type}'.");
        return Ok(());
    }

    let all = trajectory::load_all_trajectories(&task_dir)?;
    if all.is_empty() {
        println!("No trajectories found for task type '{task_type}'.");
        return Ok(());
    }

    let successes: Vec<_> = all.iter().filter(|t| t.reward > 0).collect();
    let failures: Vec<_> = all.iter().filter(|t| t.reward < 0).collect();

    println!(
        "Distilling '{}': {} trajectories ({} successes, {} failures)",
        task_type,
        all.len(),
        successes.len(),
        failures.len()
    );

    // Extract common action patterns from successes
    let mut action_counts: HashMap<&str, u32> = HashMap::new();
    for t in &successes {
        *action_counts.entry(t.action.as_str()).or_default() += 1;
    }
    let mut sorted_actions: Vec<_> = action_counts.into_iter().collect();
    sorted_actions.sort_by(|a, b| b.1.cmp(&a.1));

    // Build procedure from most common successful actions
    let procedure_steps: Vec<String> = sorted_actions
        .iter()
        .take(5)
        .map(|(action, count)| format!("{} (seen in {} successful runs)", action, count))
        .collect();

    // Extract success patterns (unique successful actions)
    let success_patterns: Vec<String> = sorted_actions
        .iter()
        .filter(|(_, count)| *count > 1)
        .map(|(action, _)| action.to_string())
        .collect();

    // Extract failure modes
    let mut failure_counts: HashMap<&str, u32> = HashMap::new();
    for t in &failures {
        let mode = t
            .result
            .strip_prefix("failure: ")
            .unwrap_or(t.result.as_str());
        *failure_counts.entry(mode).or_default() += 1;
    }
    let common_failures: Vec<FailureMode> = failure_counts
        .into_iter()
        .map(|(mode, freq)| FailureMode {
            mode: mode.to_string(),
            description: format!("Observed in {} failed runs", freq),
            frequency: freq,
        })
        .collect();

    // Load existing skill or create new
    let mut s = skill::load_skill(&saga_dir, task_type)?.unwrap_or_else(|| Skill {
        task_type: task_type.to_string(),
        version: 0,
        updated_at: String::new(),
        distilled_from: 0,
        procedure: Procedure::default(),
        success_patterns: vec![],
        common_failures: vec![],
        output_contract: OutputContract::default(),
    });

    // Update skill
    s.version += 1;
    s.updated_at = agentrail_core::timestamp_iso();
    s.distilled_from = all.len() as u32;

    if !procedure_steps.is_empty() {
        s.procedure.steps = procedure_steps;
    }
    if s.procedure.summary.is_empty() {
        s.procedure.summary = format!("Distilled from {} trajectories", all.len());
    }
    if !success_patterns.is_empty() {
        s.success_patterns = success_patterns;
    }
    if !common_failures.is_empty() {
        s.common_failures = common_failures;
    }

    let path = skill::save_skill(&saga_dir, &s)?;
    println!(
        "Saved skill v{} to {} ({} procedure steps, {} failure modes)",
        s.version,
        path.display(),
        s.procedure.steps.len(),
        s.common_failures.len()
    );

    Ok(())
}
