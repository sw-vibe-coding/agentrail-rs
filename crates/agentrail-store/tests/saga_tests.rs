use agentrail_core::{SagaStatus, StepRole, StepStatus};
use agentrail_store::{saga, step};
use tempfile::tempdir;

#[test]
fn init_creates_saga_directory_and_files() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "test-saga", "The plan").unwrap();

    assert!(saga::saga_exists(tmp.path()));
    let dir = saga::saga_dir(tmp.path());
    assert!(dir.join("saga.toml").is_file());
    assert!(dir.join("steps").is_dir());
    assert!(dir.join("trajectories").is_dir());
    assert!(dir.join("sessions").is_dir());

    let plan = std::fs::read_to_string(tmp.path().join(".agentrail/plan.md")).unwrap();
    assert_eq!(plan, "The plan");
}

#[test]
fn init_sets_correct_defaults() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "my-saga", "plan").unwrap();

    let config = saga::load_saga(tmp.path()).unwrap();
    assert_eq!(config.name, "my-saga");
    assert_eq!(config.status, SagaStatus::Active);
    assert_eq!(config.current_step, 0);
    assert!(config.plan_file.ends_with("plan.md"));
}

#[test]
fn init_fails_if_saga_already_exists() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let err = saga::init_saga(tmp.path(), "s", "p").unwrap_err();
    assert!(matches!(
        err,
        agentrail_core::error::Error::SagaAlreadyExists { .. }
    ));
}

#[test]
fn load_saga_fails_on_empty_dir() {
    let tmp = tempdir().unwrap();
    let err = saga::load_saga(tmp.path()).unwrap_err();
    assert!(matches!(
        err,
        agentrail_core::error::Error::SagaNotFound { .. }
    ));
}

#[test]
fn save_and_load_saga_roundtrips() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "rt", "plan").unwrap();

    let mut config = saga::load_saga(tmp.path()).unwrap();
    config.current_step = 5;
    config.status = SagaStatus::Completed;
    saga::save_saga(tmp.path(), &config).unwrap();

    let reloaded = saga::load_saga(tmp.path()).unwrap();
    assert_eq!(reloaded.current_step, 5);
    assert_eq!(reloaded.status, SagaStatus::Completed);
}

#[test]
fn create_step_with_role() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    let dir = step::create_step(
        &saga_dir,
        1,
        "my-step",
        "prompt text",
        "description",
        StepRole::Production,
        &[],
    )
    .unwrap();

    assert!(dir.exists());
    let config = step::load_step(&dir).unwrap();
    assert_eq!(config.role, StepRole::Production);
    assert_eq!(config.status, StepStatus::Pending);
    assert_eq!(config.number, 1);
    assert_eq!(config.slug, "my-step");

    let prompt = std::fs::read_to_string(dir.join("prompt.md")).unwrap();
    assert_eq!(prompt, "prompt text");
}

#[test]
fn transition_step_valid_transitions() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    // Pending -> InProgress -> Completed
    let dir = step::create_step(&saga_dir, 1, "a", "", "", StepRole::Legacy, &[]).unwrap();
    let mut config = step::load_step(&dir).unwrap();
    step::transition_step(&mut config, StepStatus::InProgress).unwrap();
    assert_eq!(config.status, StepStatus::InProgress);
    step::transition_step(&mut config, StepStatus::Completed).unwrap();
    assert_eq!(config.status, StepStatus::Completed);
    assert!(config.completed_at.is_some());

    // Pending -> InProgress -> Blocked
    let dir2 = step::create_step(&saga_dir, 2, "b", "", "", StepRole::Legacy, &[]).unwrap();
    let mut config2 = step::load_step(&dir2).unwrap();
    step::transition_step(&mut config2, StepStatus::InProgress).unwrap();
    step::transition_step(&mut config2, StepStatus::Blocked).unwrap();
    assert_eq!(config2.status, StepStatus::Blocked);
}

#[test]
fn transition_step_invalid_transitions() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    let dir = step::create_step(&saga_dir, 1, "a", "", "", StepRole::Legacy, &[]).unwrap();
    let mut config = step::load_step(&dir).unwrap();

    // Pending -> Completed (invalid)
    assert!(step::transition_step(&mut config, StepStatus::Completed).is_err());
    // Pending -> Blocked (invalid)
    assert!(step::transition_step(&mut config, StepStatus::Blocked).is_err());

    // Get to Completed, then try Completed -> InProgress (invalid)
    step::transition_step(&mut config, StepStatus::InProgress).unwrap();
    step::transition_step(&mut config, StepStatus::Completed).unwrap();
    assert!(step::transition_step(&mut config, StepStatus::InProgress).is_err());
}

#[test]
fn list_steps_sorted_by_number() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    // Create out of order
    step::create_step(&saga_dir, 3, "c", "", "", StepRole::Validation, &[]).unwrap();
    step::create_step(&saga_dir, 1, "a", "", "", StepRole::Meta, &[]).unwrap();
    step::create_step(&saga_dir, 2, "b", "", "", StepRole::Production, &[]).unwrap();

    let steps = step::list_steps(&saga_dir).unwrap();
    assert_eq!(steps.len(), 3);
    let numbers: Vec<u32> = steps.iter().map(|(_, c)| c.number).collect();
    assert_eq!(numbers, vec![1, 2, 3]);
}
