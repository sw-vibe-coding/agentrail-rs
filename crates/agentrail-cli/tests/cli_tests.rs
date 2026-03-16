use agentrail_cli::commands::{abort, begin, complete, history, init, next, plan, status};
use agentrail_core::SagaStatus;
use agentrail_store::{saga, step};
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
