use agentrail_cli::commands::{
    abort, begin, complete, distill, history, init, insert, next, plan, reopen, reorder, status,
};
use agentrail_core::{FailureMode, OutputContract, Procedure, SagaStatus, Skill, Trajectory};
use agentrail_store::{saga, skill, step, trajectory};
use tempfile::tempdir;

#[test]
fn init_creates_saga() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "my-feature", "Build a thing", false).unwrap();
    assert!(saga::saga_exists(tmp.path()));

    let config = saga::load_saga(tmp.path()).unwrap();
    assert_eq!(config.name, "my-feature");
}

#[test]
fn init_fails_when_saga_exists() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "s", "p", false).unwrap();
    assert!(init::run(tmp.path(), "s", "p", false).is_err());
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
    init::run(tmp.path(), "s", "plan", false).unwrap();
    let code = next::run(tmp.path()).unwrap();
    assert_eq!(code, 0);
}

#[test]
fn next_returns_1_when_complete() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "s", "plan", false).unwrap();

    let mut config = saga::load_saga(tmp.path()).unwrap();
    config.status = SagaStatus::Completed;
    saga::save_saga(tmp.path(), &config).unwrap();

    let code = next::run(tmp.path()).unwrap();
    assert_eq!(code, 1);
}

#[test]
fn status_runs_after_init() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "my-saga", "plan", false).unwrap();
    status::run(tmp.path()).unwrap();
}

#[test]
fn full_workflow() {
    let tmp = tempdir().unwrap();

    // Init
    init::run(tmp.path(), "test-saga", "The master plan", false).unwrap();

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
    init::run(tmp.path(), "s", "Original plan", false).unwrap();

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
    init::run(tmp.path(), "s", "p", false).unwrap();

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
    init::run(tmp.path(), "s", "p", false).unwrap();

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
    init::run(tmp.path(), "s", "p", false).unwrap();

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
    init::run(tmp.path(), "s", "p", false).unwrap();
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
    init::run(tmp.path(), "s", "p", false).unwrap();
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
    init::run(tmp.path(), "s", "p", false).unwrap();
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
    init::run(tmp.path(), "s", "p", false).unwrap();
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
    init::run(tmp.path(), "s", "p", false).unwrap();
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
    init::run(tmp.path(), "s", "p", false).unwrap();
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

#[test]
fn complete_advances_to_existing_planned_step() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "s", "p", false).unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    // Create step 1 with planned steps 2 and 3
    let args = complete::CompleteArgs {
        summary: Some("setup"),
        next_slug: Some("step1"),
        next_prompt: Some("first"),
        next_context: vec![],
        next_role: "production",
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

    // Steps 1, 2, 3 exist. Current is 1.
    let steps = step::list_steps(&saga_dir).unwrap();
    assert_eq!(steps.len(), 3);

    // Complete step 1 -- should advance to existing step 2, not create a duplicate
    let args2 = complete::CompleteArgs {
        summary: Some("did step 1"),
        next_slug: Some("step2-dup"),
        next_prompt: Some("this should be ignored"),
        next_context: vec![],
        next_role: "production",
        next_task_type: None,
        planned: vec![],
        done: false,
        reward: None,
        actions: None,
        failure_mode: None,
    };
    complete::run(tmp.path(), &args2).unwrap();

    // Should still have 3 steps (no duplicate created)
    let steps = step::list_steps(&saga_dir).unwrap();
    assert_eq!(steps.len(), 3);

    // Current step should be 2
    let config = saga::load_saga(tmp.path()).unwrap();
    assert_eq!(config.current_step, 2);

    // Step 2 should be the original planned step, not the duplicate
    let step2_dir = step::find_step_dir(&saga_dir, 2).unwrap();
    let step2 = step::load_step(&step2_dir).unwrap();
    assert_eq!(step2.slug, "step2");
}

#[test]
fn insert_slots_bugfix_between_pending_steps() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "s", "p", false).unwrap();

    // Seed three pending steps via complete+planned.
    let args = complete::CompleteArgs {
        summary: Some("setup"),
        next_slug: Some("feat-a"),
        next_prompt: Some("first"),
        next_context: vec![],
        next_role: "production",
        next_task_type: None,
        planned: vec!["feat-b: second".to_string(), "feat-c: third".to_string()],
        done: false,
        reward: None,
        actions: None,
        failure_mode: None,
    };
    complete::run(tmp.path(), &args).unwrap();

    // Insert a bugfix after step 1 (before feat-b).
    insert::run(
        tmp.path(),
        1,
        "hotfix-crash",
        "Reproduce and fix the crash from issue #42",
        "production",
        None,
    )
    .unwrap();

    let saga_dir = saga::saga_dir(tmp.path());
    let steps = step::list_steps(&saga_dir).unwrap();
    let numbered: Vec<(u32, String)> = steps
        .iter()
        .map(|(_, s)| (s.number, s.slug.clone()))
        .collect();
    assert_eq!(
        numbered,
        vec![
            (1, "feat-a".into()),
            (2, "hotfix-crash".into()),
            (3, "feat-b".into()),
            (4, "feat-c".into()),
        ]
    );

    // Cursor was at 1 (pending feat-a) -- no shift, still 1.
    let cfg = saga::load_saga(tmp.path()).unwrap();
    assert_eq!(cfg.current_step, 1);
}

#[test]
fn insert_adjusts_cursor_when_it_falls_in_shift_range() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "s", "p", false).unwrap();

    let args = complete::CompleteArgs {
        summary: Some("setup"),
        next_slug: Some("feat-a"),
        next_prompt: Some("first"),
        next_context: vec![],
        next_role: "production",
        next_task_type: None,
        planned: vec!["feat-b: second".to_string()],
        done: false,
        reward: None,
        actions: None,
        failure_mode: None,
    };
    complete::run(tmp.path(), &args).unwrap();

    // Complete step 1 so the cursor advances to 2.
    begin::run(tmp.path()).unwrap();
    let args2 = complete::CompleteArgs {
        summary: Some("done a"),
        next_slug: None,
        next_prompt: None,
        next_context: vec![],
        next_role: "legacy",
        next_task_type: None,
        planned: vec![],
        done: false,
        reward: None,
        actions: None,
        failure_mode: None,
    };
    complete::run(tmp.path(), &args2).unwrap();
    assert_eq!(saga::load_saga(tmp.path()).unwrap().current_step, 2);

    // Insert after 1 -- feat-b shifts from 2 to 3, cursor should follow.
    insert::run(tmp.path(), 1, "hotfix", "fix bug", "production", None).unwrap();

    let cfg = saga::load_saga(tmp.path()).unwrap();
    // Cursor follows feat-b (which was at 2, now at 3).
    assert_eq!(cfg.current_step, 3);
}

#[test]
fn reorder_moves_pending_step_forward() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "s", "p", false).unwrap();

    let args = complete::CompleteArgs {
        summary: Some("setup"),
        next_slug: Some("a"),
        next_prompt: Some("first"),
        next_context: vec![],
        next_role: "production",
        next_task_type: None,
        planned: vec![
            "b: second".to_string(),
            "c: third".to_string(),
            "d: fourth".to_string(),
        ],
        done: false,
        reward: None,
        actions: None,
        failure_mode: None,
    };
    complete::run(tmp.path(), &args).unwrap();

    // Move step 2 (b) to position 4 -- c,d shift down, b ends at 4.
    reorder::run(tmp.path(), 2, 4).unwrap();

    let saga_dir = saga::saga_dir(tmp.path());
    let steps = step::list_steps(&saga_dir).unwrap();
    let numbered: Vec<(u32, String)> = steps
        .iter()
        .map(|(_, s)| (s.number, s.slug.clone()))
        .collect();
    assert_eq!(
        numbered,
        vec![
            (1, "a".into()),
            (2, "c".into()),
            (3, "d".into()),
            (4, "b".into()),
        ]
    );
}

#[test]
fn reopen_completed_step_restores_cursor_and_preserves_commits() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "s", "p", false).unwrap();

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
    begin::run(tmp.path()).unwrap();

    // Hand-record a commit on step 1 as if `complete` had captured HEAD.
    let saga_dir = saga::saga_dir(tmp.path());
    let step_dir = step::find_step_dir(&saga_dir, 1).unwrap();
    let mut step_cfg = step::load_step(&step_dir).unwrap();
    step_cfg
        .commits
        .push("deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".into());
    step::save_step(&step_dir, &step_cfg).unwrap();

    // Complete step 1 with --done so the saga is Completed.
    let args2 = complete::CompleteArgs {
        summary: Some("shipped"),
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
    assert_eq!(
        saga::load_saga(tmp.path()).unwrap().status,
        SagaStatus::Completed
    );

    // Bug reported -- reopen step 1.
    reopen::run(tmp.path(), 1).unwrap();

    let saga_cfg = saga::load_saga(tmp.path()).unwrap();
    assert_eq!(saga_cfg.status, SagaStatus::Active);
    assert_eq!(saga_cfg.current_step, 1);

    let step_cfg = step::load_step(&step_dir).unwrap();
    assert_eq!(step_cfg.status, agentrail_core::StepStatus::InProgress);
    assert!(step_cfg.completed_at.is_none());
    // Commits from the original completion are still there.
    assert_eq!(
        step_cfg.commits,
        vec!["deadbeefdeadbeefdeadbeefdeadbeefdeadbeef".to_string()]
    );
}

#[test]
fn reopen_refuses_pending_step() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "s", "p", false).unwrap();

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

    // Step 1 is Pending -- reopen makes no sense here.
    let err = reopen::run(tmp.path(), 1).unwrap_err();
    assert!(
        err.to_string().contains("only completed or blocked"),
        "{err}"
    );
}
