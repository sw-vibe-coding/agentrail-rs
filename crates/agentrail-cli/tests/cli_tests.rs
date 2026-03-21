use agentrail_cli::commands::{abort, begin, complete, history, init, next, plan, status};
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
    };
    complete::run(tmp.path(), &args).unwrap();

    // Next should show both skill and trajectory
    let code = next::run(tmp.path()).unwrap();
    assert_eq!(code, 0);
}
