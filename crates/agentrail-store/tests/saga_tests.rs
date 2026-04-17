use agentrail_core::{SagaStatus, StepRole, StepStatus};
use agentrail_store::{saga, step};
use step::CreateStepParams;
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

    let dir = step::create_step(&CreateStepParams {
        saga_dir: &saga_dir,
        number: 1,
        slug: "my-step",
        prompt: "prompt text",
        description: "description",
        role: StepRole::Production,
        context_files: &[],
        task_type: None,
        job_spec: None,
    })
    .unwrap();

    assert!(dir.exists());
    let config = step::load_step(&dir).unwrap();
    assert_eq!(config.role, StepRole::Production);
    assert_eq!(config.status, StepStatus::Pending);
    assert_eq!(config.number, 1);
    assert_eq!(config.slug, "my-step");
    assert!(config.task_type.is_none());

    let prompt = std::fs::read_to_string(dir.join("prompt.md")).unwrap();
    assert_eq!(prompt, "prompt text");
}

#[test]
fn create_step_with_task_type() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    let dir = step::create_step(&CreateStepParams {
        saga_dir: &saga_dir,
        number: 1,
        slug: "gen-audio",
        prompt: "Generate TTS",
        description: "TTS generation",
        role: StepRole::Deterministic,
        context_files: &[],
        task_type: Some("tts"),
        job_spec: None,
    })
    .unwrap();

    let config = step::load_step(&dir).unwrap();
    assert_eq!(config.task_type, Some("tts".to_string()));
    assert_eq!(config.role, StepRole::Deterministic);
}

#[test]
fn transition_step_valid_transitions() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    // Pending -> InProgress -> Completed
    let dir = step::create_step(&CreateStepParams {
        saga_dir: &saga_dir,
        number: 1,
        slug: "a",
        prompt: "",
        description: "",
        role: StepRole::Legacy,
        context_files: &[],
        task_type: None,
        job_spec: None,
    })
    .unwrap();
    let mut config = step::load_step(&dir).unwrap();
    step::transition_step(&mut config, StepStatus::InProgress).unwrap();
    assert_eq!(config.status, StepStatus::InProgress);
    step::transition_step(&mut config, StepStatus::Completed).unwrap();
    assert_eq!(config.status, StepStatus::Completed);
    assert!(config.completed_at.is_some());

    // Pending -> InProgress -> Blocked
    let dir2 = step::create_step(&CreateStepParams {
        saga_dir: &saga_dir,
        number: 2,
        slug: "b",
        prompt: "",
        description: "",
        role: StepRole::Legacy,
        context_files: &[],
        task_type: None,
        job_spec: None,
    })
    .unwrap();
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

    let dir = step::create_step(&CreateStepParams {
        saga_dir: &saga_dir,
        number: 1,
        slug: "a",
        prompt: "",
        description: "",
        role: StepRole::Legacy,
        context_files: &[],
        task_type: None,
        job_spec: None,
    })
    .unwrap();
    let mut config = step::load_step(&dir).unwrap();

    // Pending -> Completed (invalid)
    assert!(step::transition_step(&mut config, StepStatus::Completed).is_err());
    // Pending -> Blocked (invalid)
    assert!(step::transition_step(&mut config, StepStatus::Blocked).is_err());

    // Get to Completed, then try Completed -> Blocked (invalid)
    step::transition_step(&mut config, StepStatus::InProgress).unwrap();
    step::transition_step(&mut config, StepStatus::Completed).unwrap();
    assert!(step::transition_step(&mut config, StepStatus::Blocked).is_err());
}

#[test]
fn transition_step_reopens_completed() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    let dir = step::create_step(&CreateStepParams {
        saga_dir: &saga_dir,
        number: 1,
        slug: "a",
        prompt: "",
        description: "",
        role: StepRole::Legacy,
        context_files: &[],
        task_type: None,
        job_spec: None,
    })
    .unwrap();
    let mut config = step::load_step(&dir).unwrap();
    config.commits.push("abc123".into());
    step::transition_step(&mut config, StepStatus::InProgress).unwrap();
    step::transition_step(&mut config, StepStatus::Completed).unwrap();
    assert!(config.completed_at.is_some());

    // Reopen: Completed -> InProgress clears completed_at but preserves commits.
    step::transition_step(&mut config, StepStatus::InProgress).unwrap();
    assert_eq!(config.status, StepStatus::InProgress);
    assert!(config.completed_at.is_none());
    assert_eq!(config.commits, vec!["abc123".to_string()]);

    // Can re-complete after reopen.
    step::transition_step(&mut config, StepStatus::Completed).unwrap();
    assert!(config.completed_at.is_some());
}

#[test]
fn transition_step_unblocks_to_in_progress() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    let dir = step::create_step(&CreateStepParams {
        saga_dir: &saga_dir,
        number: 1,
        slug: "a",
        prompt: "",
        description: "",
        role: StepRole::Legacy,
        context_files: &[],
        task_type: None,
        job_spec: None,
    })
    .unwrap();
    let mut config = step::load_step(&dir).unwrap();
    step::transition_step(&mut config, StepStatus::InProgress).unwrap();
    step::transition_step(&mut config, StepStatus::Blocked).unwrap();
    step::transition_step(&mut config, StepStatus::InProgress).unwrap();
    assert_eq!(config.status, StepStatus::InProgress);
}

fn create_pending(saga_dir: &std::path::Path, number: u32, slug: &str) -> std::path::PathBuf {
    step::create_step(&CreateStepParams {
        saga_dir,
        number,
        slug,
        prompt: "",
        description: "",
        role: StepRole::Legacy,
        context_files: &[],
        task_type: None,
        job_spec: None,
    })
    .unwrap()
}

#[test]
fn shift_tail_moves_numbers_up() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    create_pending(&saga_dir, 1, "a");
    create_pending(&saga_dir, 2, "b");
    create_pending(&saga_dir, 3, "c");

    step::shift_tail(&saga_dir, 2, 1).unwrap();

    let steps = step::list_steps(&saga_dir).unwrap();
    let numbers: Vec<u32> = steps.iter().map(|(_, s)| s.number).collect();
    assert_eq!(numbers, vec![1, 3, 4]);

    // Slugs stay with their dirs.
    let by_number: std::collections::HashMap<u32, String> = steps
        .iter()
        .map(|(_, s)| (s.number, s.slug.clone()))
        .collect();
    assert_eq!(by_number[&1], "a");
    assert_eq!(by_number[&3], "b");
    assert_eq!(by_number[&4], "c");

    // Dirs on disk agree with step.toml numbers.
    assert!(saga_dir.join("steps/001-a").is_dir());
    assert!(saga_dir.join("steps/003-b").is_dir());
    assert!(saga_dir.join("steps/004-c").is_dir());
    assert!(!saga_dir.join("steps/002-b").exists());
}

#[test]
fn shift_tail_rejects_completed_in_range() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    let dir1 = create_pending(&saga_dir, 1, "a");
    let dir2 = create_pending(&saga_dir, 2, "b");
    // Complete step 2.
    let mut cfg2 = step::load_step(&dir2).unwrap();
    step::transition_step(&mut cfg2, StepStatus::InProgress).unwrap();
    step::transition_step(&mut cfg2, StepStatus::Completed).unwrap();
    step::save_step(&dir2, &cfg2).unwrap();
    // Shifting from 2 hits the completed step.
    let err = step::shift_tail(&saga_dir, 2, 1).unwrap_err();
    assert!(err.to_string().contains("completed step"), "{err}");

    // But shifting from 3 is fine (no steps in range).
    step::shift_tail(&saga_dir, 3, 1).unwrap();

    // Sanity: step 1 untouched.
    let cfg1 = step::load_step(&dir1).unwrap();
    assert_eq!(cfg1.number, 1);
}

#[test]
fn shift_tail_no_op_when_range_empty() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());
    create_pending(&saga_dir, 1, "a");
    step::shift_tail(&saga_dir, 99, 1).unwrap();
    let steps = step::list_steps(&saga_dir).unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].1.number, 1);
}

#[test]
fn insert_after_pushes_tail() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    create_pending(&saga_dir, 1, "a");
    create_pending(&saga_dir, 2, "b");
    create_pending(&saga_dir, 3, "c");

    let params = CreateStepParams {
        saga_dir: &saga_dir,
        number: 0, // ignored; insert_after sets it
        slug: "fix",
        prompt: "fix the bug",
        description: "fix the bug",
        role: StepRole::Production,
        context_files: &[],
        task_type: None,
        job_spec: None,
    };
    let new_dir = step::insert_after(1, &params).unwrap();
    assert!(new_dir.ends_with("002-fix"));

    let steps = step::list_steps(&saga_dir).unwrap();
    let by_number: std::collections::HashMap<u32, String> = steps
        .iter()
        .map(|(_, s)| (s.number, s.slug.clone()))
        .collect();
    assert_eq!(by_number[&1], "a");
    assert_eq!(by_number[&2], "fix");
    assert_eq!(by_number[&3], "b");
    assert_eq!(by_number[&4], "c");
}

#[test]
fn insert_after_past_completed_boundary_ok() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    let dir1 = create_pending(&saga_dir, 1, "a");
    let dir2 = create_pending(&saga_dir, 2, "b");
    create_pending(&saga_dir, 3, "c");
    // Complete steps 1 and 2.
    for dir in [&dir1, &dir2] {
        let mut cfg = step::load_step(dir).unwrap();
        step::transition_step(&mut cfg, StepStatus::InProgress).unwrap();
        step::transition_step(&mut cfg, StepStatus::Completed).unwrap();
        step::save_step(dir, &cfg).unwrap();
    }

    // Insert after 2: shifts step 3 up, step 1 and 2 untouched.
    let params = CreateStepParams {
        saga_dir: &saga_dir,
        number: 0,
        slug: "hotfix",
        prompt: "",
        description: "",
        role: StepRole::Production,
        context_files: &[],
        task_type: None,
        job_spec: None,
    };
    step::insert_after(2, &params).unwrap();

    let steps = step::list_steps(&saga_dir).unwrap();
    let numbers: Vec<(u32, String)> = steps
        .iter()
        .map(|(_, s)| (s.number, s.slug.clone()))
        .collect();
    assert_eq!(
        numbers,
        vec![
            (1, "a".into()),
            (2, "b".into()),
            (3, "hotfix".into()),
            (4, "c".into()),
        ]
    );
}

#[test]
fn insert_after_rejects_when_would_shift_completed() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    let dir1 = create_pending(&saga_dir, 1, "a");
    let dir2 = create_pending(&saga_dir, 2, "b");
    // Complete step 2.
    let mut cfg = step::load_step(&dir2).unwrap();
    step::transition_step(&mut cfg, StepStatus::InProgress).unwrap();
    step::transition_step(&mut cfg, StepStatus::Completed).unwrap();
    step::save_step(&dir2, &cfg).unwrap();
    let _ = dir1;

    // Insert after 1: would shift completed step 2. Reject.
    let params = CreateStepParams {
        saga_dir: &saga_dir,
        number: 0,
        slug: "bad",
        prompt: "",
        description: "",
        role: StepRole::Production,
        context_files: &[],
        task_type: None,
        job_spec: None,
    };
    let err = step::insert_after(1, &params).unwrap_err();
    assert!(err.to_string().contains("completed step"), "{err}");
}

#[test]
fn move_step_forward_shifts_intermediates_back() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    create_pending(&saga_dir, 1, "a");
    create_pending(&saga_dir, 2, "b");
    create_pending(&saga_dir, 3, "c");
    create_pending(&saga_dir, 4, "d");

    // Move step 2 (b) to position 4.
    step::move_step(&saga_dir, 2, 4).unwrap();

    let steps = step::list_steps(&saga_dir).unwrap();
    let numbers: Vec<(u32, String)> = steps
        .iter()
        .map(|(_, s)| (s.number, s.slug.clone()))
        .collect();
    assert_eq!(
        numbers,
        vec![
            (1, "a".into()),
            (2, "c".into()),
            (3, "d".into()),
            (4, "b".into()),
        ]
    );
}

#[test]
fn move_step_backward_shifts_intermediates_forward() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    create_pending(&saga_dir, 1, "a");
    create_pending(&saga_dir, 2, "b");
    create_pending(&saga_dir, 3, "c");
    create_pending(&saga_dir, 4, "d");

    // Move step 4 (d) to position 2.
    step::move_step(&saga_dir, 4, 2).unwrap();

    let steps = step::list_steps(&saga_dir).unwrap();
    let numbers: Vec<(u32, String)> = steps
        .iter()
        .map(|(_, s)| (s.number, s.slug.clone()))
        .collect();
    assert_eq!(
        numbers,
        vec![
            (1, "a".into()),
            (2, "d".into()),
            (3, "b".into()),
            (4, "c".into()),
        ]
    );
}

#[test]
fn move_step_rejects_completed_source_or_range() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    create_pending(&saga_dir, 1, "a");
    let dir2 = create_pending(&saga_dir, 2, "b");
    create_pending(&saga_dir, 3, "c");
    create_pending(&saga_dir, 4, "d");
    let mut cfg2 = step::load_step(&dir2).unwrap();
    step::transition_step(&mut cfg2, StepStatus::InProgress).unwrap();
    step::transition_step(&mut cfg2, StepStatus::Completed).unwrap();
    step::save_step(&dir2, &cfg2).unwrap();

    // Moving the completed source is rejected.
    let err = step::move_step(&saga_dir, 2, 4).unwrap_err();
    assert!(err.to_string().contains("completed step"), "{err}");

    // Moving through the completed step 2 is rejected.
    let err = step::move_step(&saga_dir, 4, 1).unwrap_err();
    assert!(err.to_string().contains("completed"), "{err}");

    // Moving in the pending tail (3 <-> 4) is fine.
    step::move_step(&saga_dir, 4, 3).unwrap();
}

#[test]
fn move_step_no_op_when_from_equals_to() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());
    create_pending(&saga_dir, 1, "a");
    step::move_step(&saga_dir, 1, 1).unwrap();
    let steps = step::list_steps(&saga_dir).unwrap();
    assert_eq!(steps.len(), 1);
    assert_eq!(steps[0].1.number, 1);
}

#[test]
fn cursor_after_shift_moves_when_in_range() {
    // Insert at position 2 (shift_tail from=2, delta=+1): cursor 3 -> 4.
    assert_eq!(saga::cursor_after_shift(3, 2, 1), 4);
    // Cursor before shift range is untouched.
    assert_eq!(saga::cursor_after_shift(1, 2, 1), 1);
    // No cursor set.
    assert_eq!(saga::cursor_after_shift(0, 2, 1), 0);
    // Cursor exactly at `from` moves.
    assert_eq!(saga::cursor_after_shift(2, 2, 1), 3);
}

#[test]
fn cursor_after_move_tracks_identity() {
    // Cursor pointing at the moved step follows it.
    assert_eq!(saga::cursor_after_move(2, 2, 4), 4);
    // Cursor in the forward-moved range shifts -1.
    assert_eq!(saga::cursor_after_move(3, 2, 4), 2);
    // Cursor in the backward-moved range shifts +1.
    assert_eq!(saga::cursor_after_move(2, 4, 1), 3);
    // Cursor outside the range is untouched.
    assert_eq!(saga::cursor_after_move(5, 2, 4), 5);
    // No-op.
    assert_eq!(saga::cursor_after_move(3, 3, 3), 3);
    // Unset cursor.
    assert_eq!(saga::cursor_after_move(0, 2, 4), 0);
}

#[test]
fn list_steps_sorted_by_number() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    // Create out of order
    for (num, slug, role) in [
        (3u32, "c", StepRole::Validation),
        (1, "a", StepRole::Meta),
        (2, "b", StepRole::Production),
    ] {
        step::create_step(&CreateStepParams {
            saga_dir: &saga_dir,
            number: num,
            slug,
            prompt: "",
            description: "",
            role,
            context_files: &[],
            task_type: None,
            job_spec: None,
        })
        .unwrap();
    }

    let steps = step::list_steps(&saga_dir).unwrap();
    assert_eq!(steps.len(), 3);
    let numbers: Vec<u32> = steps.iter().map(|(_, c)| c.number).collect();
    assert_eq!(numbers, vec![1, 2, 3]);
}
