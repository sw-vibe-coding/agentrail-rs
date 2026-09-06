//! Retroactive slug renaming — the mechanism that keeps two agents'
//! parallel sagas distinguishable when their branches are merged.

use agentrail_core::{StepRole, StepStatus};
use agentrail_store::{rename, saga, step};
use std::path::Path;
use tempfile::{TempDir, tempdir};

fn mk_step(saga_dir: &Path, number: u32, slug: &str) {
    let description = format!("step {slug}");
    step::create_step(&step::CreateStepParams {
        saga_dir,
        number,
        slug,
        prompt: "do the thing",
        description: &description,
        role: StepRole::Production,
        context_files: &[],
        task_type: None,
        job_spec: None,
    })
    .unwrap();
}

/// A saga with three steps: two completed, one in-progress.
fn fixture() -> TempDir {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "perf-work", "the plan").unwrap();
    let dir = saga::saga_dir(tmp.path());

    mk_step(&dir, 1, "setup");
    mk_step(&dir, 2, "bench");
    mk_step(&dir, 3, "tune");

    for (n, status) in [
        (1, StepStatus::Completed),
        (2, StepStatus::Completed),
        (3, StepStatus::InProgress),
    ] {
        let sd = step::find_step_dir(&dir, n).unwrap();
        let mut cfg = step::load_step(&sd).unwrap();
        step::transition_step(&mut cfg, StepStatus::InProgress).unwrap();
        if status == StepStatus::Completed {
            step::transition_step(&mut cfg, StepStatus::Completed).unwrap();
            cfg.commits = vec![format!("{:040}", n)];
        }
        step::save_step(&sd, &cfg).unwrap();
    }
    tmp
}

fn dir_names(saga_dir: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(saga_dir.join("steps"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

// ---------------------------------------------------------------------------
// Prefix planning
// ---------------------------------------------------------------------------

#[test]
fn plan_prefix_covers_every_step_including_completed_ones() {
    let tmp = fixture();
    let dir = saga::saga_dir(tmp.path());

    let plan = rename::plan_prefix(&dir, "rtx5060").unwrap();
    assert_eq!(plan.len(), 3);
    assert_eq!(plan[0].number, 1);
    assert_eq!(plan[0].old_slug, "setup");
    assert_eq!(plan[0].new_slug, "rtx5060-setup");
    assert_eq!(plan[2].new_slug, "rtx5060-tune");
}

#[test]
fn plan_prefix_is_idempotent() {
    let tmp = fixture();
    let dir = saga::saga_dir(tmp.path());

    let plan = rename::plan_prefix(&dir, "rtx5060").unwrap();
    rename::apply(&dir, &plan).unwrap();

    // Second pass sees every slug already prefixed and plans nothing.
    let again = rename::plan_prefix(&dir, "rtx5060").unwrap();
    assert!(again.is_empty(), "re-running the prefix must be a no-op");
}

#[test]
fn plan_prefix_only_touches_steps_missing_the_prefix() {
    let tmp = fixture();
    let dir = saga::saga_dir(tmp.path());

    // Hand-prefix one step, then plan: only the other two need work.
    let one = rename::plan_one(&dir, 2, "rtx5060-bench").unwrap().unwrap();
    rename::apply(&dir, &[one]).unwrap();

    let plan = rename::plan_prefix(&dir, "rtx5060").unwrap();
    assert_eq!(plan.len(), 2);
    assert_eq!(
        plan.iter().map(|r| r.number).collect::<Vec<_>>(),
        vec![1, 3]
    );
}

#[test]
fn normalize_prefix_ignores_trailing_separator_and_whitespace() {
    assert_eq!(rename::normalize_prefix("rtx5060").unwrap(), "rtx5060");
    assert_eq!(rename::normalize_prefix("rtx5060-").unwrap(), "rtx5060");
    assert_eq!(
        rename::normalize_prefix("  rtx5060--  ").unwrap(),
        "rtx5060"
    );
    assert!(rename::normalize_prefix("").is_err());
    assert!(rename::normalize_prefix("---").is_err());
    assert!(rename::normalize_prefix("rtx/5060").is_err());
}

// ---------------------------------------------------------------------------
// Applying renames
// ---------------------------------------------------------------------------

#[test]
fn apply_renames_directories_and_slug_fields() {
    let tmp = fixture();
    let dir = saga::saga_dir(tmp.path());

    let plan = rename::plan_prefix(&dir, "rtx5060").unwrap();
    rename::apply(&dir, &plan).unwrap();

    assert_eq!(
        dir_names(&dir),
        vec![
            "001-rtx5060-setup".to_string(),
            "002-rtx5060-bench".to_string(),
            "003-rtx5060-tune".to_string(),
        ]
    );

    for (n, slug) in [
        (1, "rtx5060-setup"),
        (2, "rtx5060-bench"),
        (3, "rtx5060-tune"),
    ] {
        let sd = step::find_step_dir(&dir, n).unwrap();
        assert_eq!(step::load_step(&sd).unwrap().slug, slug);
    }
}

#[test]
fn apply_preserves_number_status_commits_and_artifacts() {
    let tmp = fixture();
    let dir = saga::saga_dir(tmp.path());

    let plan = rename::plan_prefix(&dir, "rtx5060").unwrap();
    rename::apply(&dir, &plan).unwrap();

    let sd = step::find_step_dir(&dir, 1).unwrap();
    let cfg = step::load_step(&sd).unwrap();
    assert_eq!(cfg.number, 1, "renaming must never renumber");
    assert_eq!(cfg.status, StepStatus::Completed);
    assert_eq!(cfg.commits, vec![format!("{:040}", 1)]);
    assert!(cfg.completed_at.is_some());
    assert_eq!(
        std::fs::read_to_string(sd.join("prompt.md")).unwrap(),
        "do the thing",
        "step artifacts move with the directory"
    );
}

#[test]
fn apply_handles_a_slug_swap_between_two_steps() {
    let tmp = fixture();
    let dir = saga::saga_dir(tmp.path());

    // 001-setup -> 001-bench and 002-bench -> 002-setup. Distinct dirs
    // (the number disambiguates), but a naive one-at-a-time rename would
    // still be fine here; the two-phase path must handle it regardless.
    let plan = vec![
        rename::StepRename {
            number: 1,
            old_slug: "setup".into(),
            new_slug: "bench".into(),
        },
        rename::StepRename {
            number: 2,
            old_slug: "bench".into(),
            new_slug: "setup".into(),
        },
    ];
    rename::apply(&dir, &plan).unwrap();

    assert_eq!(
        step::find_step_dir(&dir, 1).unwrap().file_name().unwrap(),
        "001-bench"
    );
    assert_eq!(
        step::find_step_dir(&dir, 2).unwrap().file_name().unwrap(),
        "002-setup"
    );
}

#[test]
fn apply_refuses_when_target_directory_already_exists() {
    let tmp = fixture();
    let dir = saga::saga_dir(tmp.path());

    std::fs::create_dir_all(dir.join("steps").join("001-taken")).unwrap();
    let plan = vec![rename::StepRename {
        number: 1,
        old_slug: "setup".into(),
        new_slug: "taken".into(),
    }];

    let err = rename::apply(&dir, &plan).unwrap_err().to_string();
    assert!(
        err.contains("001-taken"),
        "error should name the collision: {err}"
    );
    // Nothing moved.
    assert!(dir.join("steps").join("001-setup").is_dir());
}

#[test]
fn apply_is_atomic_when_a_later_rename_would_collide() {
    let tmp = fixture();
    let dir = saga::saga_dir(tmp.path());

    std::fs::create_dir_all(dir.join("steps").join("003-taken")).unwrap();
    let plan = vec![
        rename::StepRename {
            number: 1,
            old_slug: "setup".into(),
            new_slug: "ok".into(),
        },
        rename::StepRename {
            number: 3,
            old_slug: "tune".into(),
            new_slug: "taken".into(),
        },
    ];

    assert!(rename::apply(&dir, &plan).is_err());
    assert!(
        dir.join("steps").join("001-setup").is_dir(),
        "the valid rename must not land when a later one is rejected"
    );
}

// ---------------------------------------------------------------------------
// Single-step rename
// ---------------------------------------------------------------------------

#[test]
fn plan_one_returns_none_when_the_slug_already_matches() {
    let tmp = fixture();
    let dir = saga::saga_dir(tmp.path());
    assert!(rename::plan_one(&dir, 1, "setup").unwrap().is_none());
}

#[test]
fn plan_one_errors_for_an_unknown_step() {
    let tmp = fixture();
    let dir = saga::saga_dir(tmp.path());
    assert!(rename::plan_one(&dir, 99, "whatever").is_err());
}

// ---------------------------------------------------------------------------
// Slug validation
// ---------------------------------------------------------------------------

#[test]
fn validate_slug_rejects_path_escapes_and_hidden_names() {
    for bad in ["", ".", "..", "../evil", "a/b", "a\\b", ".hidden", "  "] {
        assert!(
            rename::validate_slug(bad).is_err(),
            "expected {bad:?} to be rejected"
        );
    }
    for good in ["setup", "rtx5060-tune-kernel", "step_2", "v1.2-bench"] {
        assert!(
            rename::validate_slug(good).is_ok(),
            "expected {good:?} to be accepted"
        );
    }
}

// ---------------------------------------------------------------------------
// Saga name
// ---------------------------------------------------------------------------

#[test]
fn prefixed_saga_name_applies_once() {
    assert_eq!(
        rename::prefixed_saga_name("perf-work", "rtx5060"),
        Some("rtx5060-perf-work".to_string())
    );
    assert_eq!(
        rename::prefixed_saga_name("rtx5060-perf-work", "rtx5060"),
        None
    );
}
