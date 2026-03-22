use agentrail_core::error::{Error, Result};
use agentrail_core::{SagaStatus, StepRole, StepStatus, Trajectory};
use agentrail_store::{domain, saga, step, trajectory};
use std::path::Path;

/// Run the orchestrator loop: auto-execute deterministic steps,
/// pause at production/meta steps for agent intervention.
/// Returns exit code: 0 = paused at agent step, 1 = saga complete, 2 = no work
pub fn run(saga_path: &Path) -> Result<u8> {
    let mut config = saga::load_saga(saga_path)?;
    let saga_dir = saga::saga_dir(saga_path);

    if config.status == SagaStatus::Completed {
        println!("Saga '{}' is complete.", config.name);
        return Ok(1);
    }

    if config.current_step == 0 {
        println!("No steps to run. Use `agentrail complete` to create the first step.");
        return Ok(2);
    }

    loop {
        let step_dir = match step::find_step_dir(&saga_dir, config.current_step) {
            Ok(d) => d,
            Err(Error::NoCurrentStep) => {
                println!("No more steps. Saga may be complete.");
                return Ok(1);
            }
            Err(e) => return Err(e),
        };

        let mut step_config = step::load_step(&step_dir)?;

        // Skip already completed steps
        if step_config.status == StepStatus::Completed {
            println!(
                "  [skip] {:03}-{} already completed",
                step_config.number, step_config.slug
            );
            // Try next step
            config.current_step += 1;
            saga::save_saga(saga_path, &config)?;
            continue;
        }

        match step_config.role {
            StepRole::Deterministic => {
                println!(
                    "  [exec] {:03}-{} [deterministic]",
                    step_config.number, step_config.slug
                );

                // Transition to in-progress
                if step_config.status == StepStatus::Pending {
                    step::transition_step(&mut step_config, StepStatus::InProgress)?;
                    step::save_step(&step_dir, &step_config)?;
                }

                // Execute the job
                let result = execute_deterministic_step(&saga_dir, &step_config);

                match result {
                    Ok(summary) => {
                        // Record success trajectory
                        if let Some(ref task_type) = step_config.task_type {
                            let t = Trajectory {
                                task_type: task_type.clone(),
                                state: serde_json::json!({"step": step_config.slug}),
                                action: summary.clone(),
                                result: "success".to_string(),
                                reward: 1,
                                timestamp: agentrail_core::timestamp_iso(),
                            };
                            trajectory::save_trajectory(&saga_dir, &t)?;
                        }

                        step::transition_step(&mut step_config, StepStatus::Completed)?;
                        step::save_step(&step_dir, &step_config)?;
                        step::save_summary(&step_dir, &summary)?;

                        println!(
                            "    -> completed: {}",
                            agentrail_core::truncate(&summary, 60)
                        );
                    }
                    Err(e) => {
                        // Record failure trajectory
                        if let Some(ref task_type) = step_config.task_type {
                            let t = Trajectory {
                                task_type: task_type.clone(),
                                state: serde_json::json!({"step": step_config.slug}),
                                action: "executor failed".to_string(),
                                result: e.to_string(),
                                reward: -1,
                                timestamp: agentrail_core::timestamp_iso(),
                            };
                            trajectory::save_trajectory(&saga_dir, &t)?;
                        }

                        step::transition_step(&mut step_config, StepStatus::Blocked)?;
                        step::save_step(&step_dir, &step_config)?;
                        step::save_summary(&step_dir, &format!("BLOCKED: {e}"))?;

                        println!("    -> FAILED: {e}");
                        println!("    Pausing loop. Fix the issue and re-run.");
                        return Ok(0);
                    }
                }

                // Check if there's a next step
                let next_num = config.current_step + 1;
                if step::find_step_dir(&saga_dir, next_num).is_ok() {
                    config.current_step = next_num;
                    saga::save_saga(saga_path, &config)?;
                } else {
                    println!("  No more steps after {:03}.", step_config.number);
                    return Ok(1);
                }
            }

            StepRole::Validation => {
                println!(
                    "  [validate] {:03}-{} [validation]",
                    step_config.number, step_config.slug
                );

                if step_config.status == StepStatus::Pending {
                    step::transition_step(&mut step_config, StepStatus::InProgress)?;
                    step::save_step(&step_dir, &step_config)?;
                }

                let result = run_validation_step(&saga_dir, &step_config);

                match result {
                    Ok(summary) => {
                        step::transition_step(&mut step_config, StepStatus::Completed)?;
                        step::save_step(&step_dir, &step_config)?;
                        step::save_summary(&step_dir, &summary)?;
                        println!("    -> passed: {}", agentrail_core::truncate(&summary, 60));
                    }
                    Err(e) => {
                        step::transition_step(&mut step_config, StepStatus::Blocked)?;
                        step::save_step(&step_dir, &step_config)?;
                        step::save_summary(&step_dir, &format!("VALIDATION FAILED: {e}"))?;
                        println!("    -> FAILED: {e}");
                        return Ok(0);
                    }
                }

                let next_num = config.current_step + 1;
                if step::find_step_dir(&saga_dir, next_num).is_ok() {
                    config.current_step = next_num;
                    saga::save_saga(saga_path, &config)?;
                } else {
                    println!("  No more steps after {:03}.", step_config.number);
                    return Ok(1);
                }
            }

            // Production, Meta, Legacy: pause for agent
            _ => {
                println!(
                    "  [pause] {:03}-{} [{}] -- requires agent",
                    step_config.number, step_config.slug, step_config.role
                );
                println!("  Run `agentrail next` for full context.");
                return Ok(0);
            }
        }
    }
}

fn execute_deterministic_step(
    saga_dir: &Path,
    step_config: &agentrail_core::StepConfig,
) -> Result<String> {
    let job = step_config
        .job_spec
        .as_ref()
        .ok_or_else(|| Error::JobFailed("Deterministic step has no job_spec".to_string()))?;

    // Find domain that provides the executor
    if let Some((domain_dir, _)) =
        domain::find_domain_for_task(saga_dir, step_config.task_type.as_deref().unwrap_or(""))?
    {
        let result = agentrail_exec::run_shell_executor(&domain_dir, job)?;
        if result.success {
            Ok(format!(
                "Executed {} via {}: {}",
                job.kind,
                domain_dir.display(),
                result.outputs
            ))
        } else {
            Err(Error::JobFailed(
                result
                    .error
                    .unwrap_or_else(|| "executor returned failure".to_string()),
            ))
        }
    } else {
        Err(Error::JobFailed(format!(
            "No domain found for task type '{}'",
            step_config.task_type.as_deref().unwrap_or("(none)")
        )))
    }
}

fn run_validation_step(
    saga_dir: &Path,
    step_config: &agentrail_core::StepConfig,
) -> Result<String> {
    let task_type = step_config.task_type.as_deref().unwrap_or("");

    let (domain_dir, tt) = domain::find_domain_for_task(saga_dir, task_type)?.ok_or_else(|| {
        Error::ValidationFailed(format!("No domain found for task type '{task_type}'"))
    })?;

    if tt.validators.is_empty() {
        return Ok("No validators configured".to_string());
    }

    // Build context from job spec params or empty
    let context = step_config
        .job_spec
        .as_ref()
        .map(|j| j.params.clone())
        .unwrap_or(serde_json::json!({}));

    let mut results = Vec::new();
    for check in &tt.validators {
        let vr = agentrail_validate::run_shell_validator(&domain_dir, check, &context)?;
        if !vr.valid {
            return Err(Error::ValidationFailed(format!(
                "Check '{}' failed: {}",
                check,
                vr.error.unwrap_or_else(|| "unknown".to_string())
            )));
        }
        results.push(format!("{check}: ok"));
    }

    Ok(format!("All checks passed: {}", results.join(", ")))
}
