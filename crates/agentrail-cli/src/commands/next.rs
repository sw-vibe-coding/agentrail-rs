use agentrail_core::error::{Error, Result};
use agentrail_core::{SagaStatus, StepStatus};
use agentrail_store::{saga, skill, step, trajectory};
use std::path::Path;

/// Returns exit code: 0 = active step found, 1 = saga complete, 2 = no saga
pub fn run(saga_path: &Path) -> Result<u8> {
    if !saga::saga_exists(saga_path) {
        eprintln!("No saga found. Run `agentrail init` to create one.");
        return Ok(2);
    }

    let config = saga::load_saga(saga_path)?;
    if config.status == SagaStatus::Completed {
        println!("Saga '{}' is complete. No more steps.", config.name);
        return Ok(1);
    }

    let saga_dir = saga::saga_dir(saga_path);

    // Print plan
    let plan_path = saga_path.join(&config.plan_file);
    if plan_path.is_file() {
        let plan = std::fs::read_to_string(&plan_path)?;
        println!("=== PLAN ===");
        println!("{}", plan.trim());
        println!();
    }

    // Print step list
    let steps = step::list_steps(&saga_dir)?;
    if !steps.is_empty() {
        println!("=== STEPS ===");
        for (_, s) in &steps {
            let marker = match s.status {
                StepStatus::Completed => "x",
                StepStatus::InProgress => ">",
                StepStatus::Blocked => "!",
                StepStatus::Pending => " ",
            };
            let here = if s.number == config.current_step {
                " <-- YOU ARE HERE"
            } else {
                ""
            };
            println!(
                "  [{}] {:03}-{} [{}]: {}{}",
                marker, s.number, s.slug, s.role, s.description, here
            );
        }
        println!();
    }

    // Print current step details
    if config.current_step > 0 {
        match step::find_step_dir(&saga_dir, config.current_step) {
            Ok(step_dir) => {
                let step_config = step::load_step(&step_dir)?;
                println!(
                    "=== CURRENT STEP: {:03}-{} [{}] ===",
                    step_config.number, step_config.slug, step_config.role
                );
                println!("Status: {}", step_config.status);
                println!("Description: {}", step_config.description);

                if let Some(ref task_type) = step_config.task_type {
                    println!("Task type: {}", task_type);
                }

                if !step_config.context_files.is_empty() {
                    println!("\nContext files:");
                    for f in &step_config.context_files {
                        println!("  - {}", f);
                    }
                }

                let prompt_path = step_dir.join("prompt.md");
                if prompt_path.is_file() {
                    let prompt = std::fs::read_to_string(&prompt_path)?;
                    if !prompt.trim().is_empty() {
                        println!("\n=== PROMPT ===");
                        println!("{}", prompt.trim());
                    }
                }

                // XSkill dual memory: inject skill doc + past experiences
                if let Some(ref task_type) = step_config.task_type {
                    // Skill (strategic workflow)
                    if let Some(s) = skill::load_skill(&saga_dir, task_type)? {
                        println!("\n=== SKILL: {} (v{}) ===", task_type, s.version);
                        if !s.procedure.summary.is_empty() {
                            println!("{}", s.procedure.summary);
                        }
                        if !s.procedure.steps.is_empty() {
                            println!("\nProcedure:");
                            for (i, step_text) in s.procedure.steps.iter().enumerate() {
                                println!("  {}. {}", i + 1, step_text);
                            }
                        }
                        if !s.success_patterns.is_empty() {
                            println!("\nSuccess patterns:");
                            for p in &s.success_patterns {
                                println!("  - {}", p);
                            }
                        }
                        if !s.common_failures.is_empty() {
                            println!("\nKnown failure modes:");
                            for f in &s.common_failures {
                                println!("  - {} ({}x): {}", f.mode, f.frequency, f.description);
                            }
                        }
                    }

                    // Experiences (tactical per-run records)
                    let successes = trajectory::retrieve_successes(&saga_dir, task_type, 3)?;
                    if !successes.is_empty() {
                        println!("\n=== PAST SUCCESSES ({}) ===", task_type);
                        for t in &successes {
                            println!(
                                "  [{}] action={}, result={}, reward={:+}",
                                t.timestamp, t.action, t.result, t.reward
                            );
                        }
                    }
                }

                println!();
                println!("=== WHEN DONE ===");
                println!("Run: agentrail begin    (if not yet started)");
                println!(
                    "Run: agentrail complete --summary \"what you did\" --next-slug <slug> --next-prompt \"next instructions\""
                );
                println!(
                    "  Or: agentrail complete --summary \"what you did\" --done   (if this is the last step)"
                );
            }
            Err(Error::NoCurrentStep) => {
                println!(
                    "No step {} found. Use `agentrail complete` to create the next step.",
                    config.current_step
                );
            }
            Err(e) => return Err(e),
        }
    } else {
        println!("Saga initialized but no steps created yet.");
        println!(
            "Run: agentrail complete --summary \"initial setup\" --next-slug <slug> --next-prompt \"instructions\""
        );
    }

    Ok(0)
}
