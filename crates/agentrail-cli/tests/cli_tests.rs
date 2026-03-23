use agentrail_cli::commands::{abort, begin, complete, distill, history, init, next, plan, status};
use agentrail_core::{FailureMode, OutputContract, Procedure, SagaStatus, Skill, Trajectory};
use agentrail_store::{saga, skill, step, trajectory};
use tempfile::tempdir;

#[test]
fn init_creates_saga() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "my-feature", "Build a thing").unwrap();
    assert!(saga::saga_exists(tmp.path()));

    let config = saga::load_saga(tmp.path()).unwrap();
    assert_eq!(config.name, "my-feature");
}

#[test]
fn init_fails_when_saga_exists() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "s", "p").unwrap();
    assert!(init::run(tmp.path(), "s", "p").is_err());
}

#[test]
fn next_returns_2_when_no_saga() {
    let tmp = tempdir().unwrap();
    let code = next::run(tmp.path()).unwrap();
    assert_eq!(code, 2);
}

#[test]
fn next_returns_0_after_init() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "s", "plan").unwrap();
    let code = next::run(tmp.path()).unwrap();
    assert_eq!(code, 0);
}

#[test]
fn next_returns_1_when_complete() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "s", "plan").unwrap();

    let mut config = saga::load_saga(tmp.path()).unwrap();
    config.status = SagaStatus::Completed;
    saga::save_saga(tmp.path(), &config).unwrap();

    let code = next::run(tmp.path()).unwrap();
    assert_eq!(code, 1);
}

#[test]
fn status_runs_after_init() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "my-saga", "plan").unwrap();
    status::run(tmp.path()).unwrap();
}

#[test]
fn full_workflow() {
    let tmp = tempdir().unwrap();

    // Init
    init::run(tmp.path(), "test-saga", "The master plan").unwrap();

    // Complete step 0, create step 1
    let args = complete::CompleteArgs {
        summary: Some("Initial setup done"),
        next_slug: Some("implement"),
        next_prompt: Some("Write the code"),
        next_context: vec![],
        next_role: "production",
        next_task_type: None,
        planned: vec![],
        done: false,
        reward: None,
        actions: None,
        failure_mode: None,
    };
    complete::run(tmp.path(), &args).unwrap();

    let config = saga::load_saga(tmp.path()).unwrap();
    assert_eq!(config.current_step, 1);

    // Next should show step 1
    let code = next::run(tmp.path()).unwrap();
    assert_eq!(code, 0);

    // Begin step 1
    begin::run(tmp.path()).unwrap();
    let saga_dir = saga::saga_dir(tmp.path());
    let step_dir = step::find_step_dir(&saga_dir, 1).unwrap();
    let step_config = step::load_step(&step_dir).unwrap();
    assert_eq!(step_config.status, agentrail_core::StepStatus::InProgress);
    assert_eq!(step_config.role, agentrail_core::StepRole::Production);

    // Complete step 1, mark done
    let args2 = complete::CompleteArgs {
        summary: Some("Code written"),
        next_slug: None,
        next_prompt: None,
        next_context: vec![],
        next_role: "legacy",
        next_task_type: None,
        planned: vec![],
        done: true,
        reward: None,
        actions: None,
        failure_mode: None,
    };
    complete::run(tmp.path(), &args2).unwrap();

    let config = saga::load_saga(tmp.path()).unwrap();
    assert_eq!(config.status, SagaStatus::Completed);

    // Next should return 1 (complete)
    let code = next::run(tmp.path()).unwrap();
    assert_eq!(code, 1);
}

#[test]
fn plan_view_and_update() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "s", "Original plan").unwrap();

    // View
    plan::run(tmp.path(), None).unwrap();

    // Update
    plan::run(tmp.path(), Some("New plan")).unwrap();

    let content = std::fs::read_to_string(tmp.path().join(".agentrail/plan.md")).unwrap();
    assert_eq!(content, "New plan");
}

#[test]
fn history_shows_steps() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "s", "p").unwrap();

    let args = complete::CompleteArgs {
        summary: Some("setup"),
        next_slug: Some("step1"),
        next_prompt: Some("do it"),
        next_context: vec![],
        next_role: "meta",
        next_task_type: None,
        planned: vec![],
        done: false,
        reward: None,
        actions: None,
        failure_mode: None,
    };
    complete::run(tmp.path(), &args).unwrap();

    history::run(tmp.path()).unwrap();
}

#[test]
fn abort_blocks_step() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "s", "p").unwrap();

    let args = complete::CompleteArgs {
        summary: Some("setup"),
        next_slug: Some("step1"),
        next_prompt: Some("do it"),
        next_context: vec![],
        next_role: "production",
        next_task_type: None,
        planned: vec![],
        done: false,
        reward: None,
        actions: None,
        failure_mode: None,
    };
    complete::run(tmp.path(), &args).unwrap();

    abort::run(tmp.path(), Some("stuck")).unwrap();

    let saga_dir = saga::saga_dir(tmp.path());
    let step_dir = step::find_step_dir(&saga_dir, 1).unwrap();
    let step_config = step::load_step(&step_dir).unwrap();
    assert_eq!(step_config.status, agentrail_core::StepStatus::Blocked);
}

#[test]
fn complete_with_planned_steps() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "s", "p").unwrap();

    let args = complete::CompleteArgs {
        summary: Some("setup"),
        next_slug: Some("step1"),
        next_prompt: Some("first"),
        next_context: vec![],
        next_role: "meta",
        next_task_type: None,
        planned: vec![
            "step2: do second thing".to_string(),
            "step3: do third thing".to_string(),
        ],
        done: false,
        reward: None,
        actions: None,
        failure_mode: None,
    };
    complete::run(tmp.path(), &args).unwrap();

    let saga_dir = saga::saga_dir(tmp.path());
    let steps = step::list_steps(&saga_dir).unwrap();
    assert_eq!(steps.len(), 3);
    assert_eq!(steps[0].1.slug, "step1");
    assert_eq!(steps[1].1.slug, "step2");
    assert_eq!(steps[2].1.slug, "step3");
}

#[test]
fn complete_with_task_type_and_next_shows_trajectories() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    // Pre-populate trajectories for the "tts" task type
    let t = Trajectory {
        task_type: "tts".to_string(),
        state: serde_json::json!({"script": "hello"}),
        action: "gradio_client".to_string(),
        result: "ok".to_string(),
        reward: 1,
        timestamp: "2026-03-20T10:00:00".to_string(),
    };
    trajectory::save_trajectory(&saga_dir, &t).unwrap();

    // Create step 1 with task_type "tts"
    let args = complete::CompleteArgs {
        summary: Some("setup"),
        next_slug: Some("gen-audio"),
        next_prompt: Some("Generate TTS audio"),
        next_context: vec![],
        next_role: "deterministic",
        next_task_type: Some("tts"),
        planned: vec![],
        done: false,
        reward: None,
        actions: None,
        failure_mode: None,
    };
    complete::run(tmp.path(), &args).unwrap();

    // Verify task_type was set
    let step_dir = step::find_step_dir(&saga_dir, 1).unwrap();
    let step_config = step::load_step(&step_dir).unwrap();
    assert_eq!(step_config.task_type, Some("tts".to_string()));

    // Next should succeed and include trajectory data (we just verify it runs)
    let code = next::run(tmp.path()).unwrap();
    assert_eq!(code, 0);
}

#[test]
fn next_shows_skill_and_trajectories_together() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    // Save a skill doc for "tts"
    let s = Skill {
        task_type: "tts".to_string(),
        version: 2,
        updated_at: "2026-03-21T10:00:00".to_string(),
        distilled_from: 5,
        procedure: Procedure {
            summary: "Generate TTS audio".to_string(),
            steps: vec!["Read script".to_string(), "Call API".to_string()],
        },
        success_patterns: vec!["Use Gradio client".to_string()],
        common_failures: vec![FailureMode {
            mode: "wrong_api".to_string(),
            description: "Used HTTP instead of Gradio".to_string(),
            frequency: 3,
        }],
        output_contract: OutputContract {
            required_files: vec!["output.wav".to_string()],
            acceptance_checks: vec!["file-exists".to_string()],
        },
    };
    skill::save_skill(&saga_dir, &s).unwrap();

    // Save a trajectory
    let t = Trajectory {
        task_type: "tts".to_string(),
        state: serde_json::json!({}),
        action: "gradio_client".to_string(),
        result: "ok".to_string(),
        reward: 1,
        timestamp: "2026-03-21T09:00:00".to_string(),
    };
    trajectory::save_trajectory(&saga_dir, &t).unwrap();

    // Create step with task_type
    let args = complete::CompleteArgs {
        summary: Some("setup"),
        next_slug: Some("gen-audio"),
        next_prompt: Some("Generate TTS"),
        next_context: vec![],
        next_role: "deterministic",
        next_task_type: Some("tts"),
        planned: vec![],
        done: false,
        reward: None,
        actions: None,
        failure_mode: None,
    };
    complete::run(tmp.path(), &args).unwrap();

    // Next should show both skill and trajectory
    let code = next::run(tmp.path()).unwrap();
    assert_eq!(code, 0);
}

#[test]
fn complete_records_trajectory_with_reward() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    // Create step 1 with task_type
    let args = complete::CompleteArgs {
        summary: Some("setup"),
        next_slug: Some("gen-audio"),
        next_prompt: Some("Generate TTS"),
        next_context: vec![],
        next_role: "deterministic",
        next_task_type: Some("tts"),
        planned: vec![],
        done: false,
        reward: None,
        actions: None,
        failure_mode: None,
    };
    complete::run(tmp.path(), &args).unwrap();

    // Begin and complete step 1 with reward
    begin::run(tmp.path()).unwrap();
    let args2 = complete::CompleteArgs {
        summary: Some("Generated TTS audio successfully"),
        next_slug: None,
        next_prompt: None,
        next_context: vec![],
        next_role: "legacy",
        next_task_type: None,
        planned: vec![],
        done: true,
        reward: Some(1),
        actions: Some("gradio_client /tts with reference voice"),
        failure_mode: None,
    };
    complete::run(tmp.path(), &args2).unwrap();

    // Verify trajectory was recorded
    let trajectories =
        trajectory::load_all_trajectories(&saga_dir.join("trajectories/tts")).unwrap();
    assert_eq!(trajectories.len(), 1);
    assert_eq!(trajectories[0].task_type, "tts");
    assert_eq!(trajectories[0].reward, 1);
    assert_eq!(
        trajectories[0].action,
        "gradio_client /tts with reference voice"
    );
    assert_eq!(trajectories[0].result, "success");
}

#[test]
fn complete_records_failure_trajectory() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    // Create step 1 with task_type
    let args = complete::CompleteArgs {
        summary: Some("setup"),
        next_slug: Some("gen-audio"),
        next_prompt: Some("Generate TTS"),
        next_context: vec![],
        next_role: "deterministic",
        next_task_type: Some("tts"),
        planned: vec![],
        done: false,
        reward: None,
        actions: None,
        failure_mode: None,
    };
    complete::run(tmp.path(), &args).unwrap();

    // Complete with failure
    let args2 = complete::CompleteArgs {
        summary: Some("TTS failed - used wrong API"),
        next_slug: None,
        next_prompt: None,
        next_context: vec![],
        next_role: "legacy",
        next_task_type: None,
        planned: vec![],
        done: true,
        reward: Some(-1),
        actions: Some("curl http://localhost:7860/tts"),
        failure_mode: Some("wrong_api"),
    };
    complete::run(tmp.path(), &args2).unwrap();

    let trajectories =
        trajectory::load_all_trajectories(&saga_dir.join("trajectories/tts")).unwrap();
    assert_eq!(trajectories.len(), 1);
    assert_eq!(trajectories[0].reward, -1);
    assert_eq!(trajectories[0].result, "failure: wrong_api");
}

#[test]
fn distill_generates_skill_from_trajectories() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    // Pre-populate trajectories
    for (action, reward, result) in [
        ("gradio_client /tts", 1i8, "success"),
        ("gradio_client /tts", 1, "success"),
        ("gradio_client /tts", 1, "success"),
        ("curl http://localhost/tts", -1, "failure: wrong_api"),
        ("curl http://localhost/tts", -1, "failure: wrong_api"),
    ] {
        let t = Trajectory {
            task_type: "tts".to_string(),
            state: serde_json::json!({}),
            action: action.to_string(),
            result: result.to_string(),
            reward,
            timestamp: "2026-03-22T10:00:00".to_string(),
        };
        trajectory::save_trajectory(&saga_dir, &t).unwrap();
    }

    // No skill exists yet
    assert!(skill::load_skill(&saga_dir, "tts").unwrap().is_none());

    // Distill
    distill::run(tmp.path(), "tts").unwrap();

    // Skill should now exist
    let s = skill::load_skill(&saga_dir, "tts").unwrap().unwrap();
    assert_eq!(s.task_type, "tts");
    assert_eq!(s.version, 1);
    assert_eq!(s.distilled_from, 5);
    assert!(!s.procedure.steps.is_empty());
    assert!(!s.common_failures.is_empty());
    assert_eq!(s.common_failures[0].mode, "wrong_api");
    assert_eq!(s.common_failures[0].frequency, 2);

    // Distill again increments version
    distill::run(tmp.path(), "tts").unwrap();
    let s2 = skill::load_skill(&saga_dir, "tts").unwrap().unwrap();
    assert_eq!(s2.version, 2);
}

#[test]
fn full_loop_complete_with_trajectory_then_distill_then_next() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    // Step 0 -> Step 1 (tts)
    let args = complete::CompleteArgs {
        summary: Some("setup"),
        next_slug: Some("gen-audio"),
        next_prompt: Some("Generate TTS"),
        next_context: vec![],
        next_role: "production",
        next_task_type: Some("tts"),
        planned: vec![],
        done: false,
        reward: None,
        actions: None,
        failure_mode: None,
    };
    complete::run(tmp.path(), &args).unwrap();

    // Complete step 1 with trajectory
    let args2 = complete::CompleteArgs {
        summary: Some("Generated audio"),
        next_slug: Some("gen-audio-2"),
        next_prompt: Some("Generate TTS for segment 2"),
        next_context: vec![],
        next_role: "production",
        next_task_type: Some("tts"),
        planned: vec![],
        done: false,
        reward: Some(1),
        actions: Some("gradio_client /tts"),
        failure_mode: None,
    };
    complete::run(tmp.path(), &args2).unwrap();

    // Distill creates skill from the trajectory
    distill::run(tmp.path(), "tts").unwrap();

    // Next for step 2 should show both the distilled skill and the trajectory
    let code = next::run(tmp.path()).unwrap();
    assert_eq!(code, 0);

    // Verify skill exists
    let s = skill::load_skill(&saga_dir, "tts").unwrap().unwrap();
    assert_eq!(s.distilled_from, 1);
}
