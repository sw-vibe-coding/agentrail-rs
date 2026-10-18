//! Correcting a saga whose record disagrees with what was done:
//! `close` (done elsewhere), `edit` (a stale prompt), `reserve` (someone
//! else's step), and the cursor never landing on finished work.

use agentrail_cli::commands::{close, complete, edit, init, reserve};
use agentrail_core::StepStatus;
use agentrail_store::{saga, step};
use std::path::Path;
use tempfile::tempdir;

/// A saga whose steps 1..=n exist (1 is current, the rest pending).
fn saga_with(dir: &Path, n: usize) {
    init::run(dir, "s", "plan", false).unwrap();
    let planned: Vec<String> = (2..=n).map(|i| format!("s{i}: step {i}")).collect();
    complete::run(dir, &complete_args(Some("s1"), planned)).unwrap();
}

fn complete_args(next_slug: Option<&str>, planned: Vec<String>) -> complete::CompleteArgs<'_> {
    complete::CompleteArgs {
        summary: Some("done"),
        next_slug,
        next_prompt: next_slug.map(|_| "first"),
        next_context: vec![],
        next_role: "production",
        next_task_type: None,
        planned,
        done: false,
        reward: None,
        actions: None,
        failure_mode: None,
    }
}

fn load(dir: &Path, n: u32) -> agentrail_core::StepConfig {
    let saga_dir = saga::saga_dir(dir);
    step::load_step(&step::find_step_dir(&saga_dir, n).unwrap()).unwrap()
}

fn set_cursor(dir: &Path, n: u32) {
    let mut config = saga::load_saga(dir).unwrap();
    config.current_step = n;
    saga::save_saga(dir, &config).unwrap();
}

fn close_args<'a>(
    number: u32,
    commits: &'a [String],
    elsewhere: Option<&'a str>,
) -> close::CloseArgs<'a> {
    close::CloseArgs {
        number,
        summary: "done in the libraries",
        commits,
        elsewhere,
    }
}

#[test]
fn close_a_stranded_step_records_where_and_keeps_the_cursor() {
    let tmp = tempdir().unwrap();
    saga_with(tmp.path(), 4);
    set_cursor(tmp.path(), 3); // step 2 is pending behind the cursor
    let saga_dir = saga::saga_dir(tmp.path());
    assert_eq!(step::stranded(&saga_dir, 3).unwrap().len(), 2); // 1 and 2

    close::run(tmp.path(), &close_args(2, &[], Some("libs@0f82f93"))).unwrap();
    let s2 = load(tmp.path(), 2);
    assert_eq!(s2.status, StepStatus::Completed);
    assert_eq!(s2.done_elsewhere.as_deref(), Some("libs@0f82f93"));
    assert!(s2.commits.is_empty(), "HEAD is never recorded on its own");
    assert_eq!(saga::load_saga(tmp.path()).unwrap().current_step, 3);
    let summary = std::fs::read_to_string(
        step::find_step_dir(&saga_dir, 2)
            .unwrap()
            .join("summary.md"),
    )
    .unwrap();
    assert_eq!(summary, "done in the libraries");
    let left: Vec<u32> = step::stranded(&saga_dir, 3)
        .unwrap()
        .iter()
        .map(|s| s.number)
        .collect();
    assert_eq!(left, [1]);
}

#[test]
fn close_needs_to_know_where_the_work_is() {
    let tmp = tempdir().unwrap();
    saga_with(tmp.path(), 3);
    assert!(close::run(tmp.path(), &close_args(2, &[], None)).is_err());
    assert_eq!(load(tmp.path(), 2).status, StepStatus::Pending);
}

#[test]
fn close_refuses_a_completed_step() {
    let tmp = tempdir().unwrap();
    saga_with(tmp.path(), 3);
    close::run(tmp.path(), &close_args(2, &[], Some("x"))).unwrap();
    assert!(close::run(tmp.path(), &close_args(2, &[], Some("x"))).is_err());
}

#[test]
fn close_resolves_commits_in_this_repository() {
    let tmp = tempdir().unwrap();
    let git = |args: &[&str]| {
        let ok = std::process::Command::new("git")
            .args(args)
            .current_dir(tmp.path())
            .output()
            .unwrap();
        assert!(ok.status.success(), "{args:?}");
        String::from_utf8(ok.stdout).unwrap()
    };
    git(&["init", "-q"]);
    git(&[
        "-c",
        "user.email=t@t",
        "-c",
        "user.name=t",
        "commit",
        "-q",
        "--allow-empty",
        "-m",
        "work",
    ]);
    let head = git(&["rev-parse", "HEAD"]).trim().to_string();
    saga_with(tmp.path(), 3);
    let refs = ["HEAD".to_string()];
    close::run(tmp.path(), &close_args(3, &refs, None)).unwrap();
    assert_eq!(load(tmp.path(), 3).commits, [head]);
    let bad = ["no-such-ref".to_string()];
    assert!(close::run(tmp.path(), &close_args(2, &bad, None)).is_err());
}

#[test]
fn closing_the_current_step_moves_the_cursor_to_the_next_open_one() {
    let tmp = tempdir().unwrap();
    saga_with(tmp.path(), 4);
    set_cursor(tmp.path(), 2);
    close::run(tmp.path(), &close_args(3, &[], Some("lane b"))).unwrap();
    close::run(tmp.path(), &close_args(2, &[], Some("lane b"))).unwrap();
    assert_eq!(saga::load_saga(tmp.path()).unwrap().current_step, 4);
}

#[test]
fn complete_skips_completed_and_reserved_steps() {
    let tmp = tempdir().unwrap();
    saga_with(tmp.path(), 5);
    close::run(tmp.path(), &close_args(2, &[], Some("elsewhere"))).unwrap();
    reserve::reserve(tmp.path(), 3, "lane readable").unwrap();
    complete::run(tmp.path(), &complete_args(None, vec![])).unwrap();
    assert_eq!(saga::load_saga(tmp.path()).unwrap().current_step, 4);
}

#[test]
fn complete_creates_a_new_step_past_finished_ones() {
    let tmp = tempdir().unwrap();
    saga_with(tmp.path(), 3);
    close::run(tmp.path(), &close_args(2, &[], Some("x"))).unwrap();
    close::run(tmp.path(), &close_args(3, &[], Some("x"))).unwrap();
    complete::run(tmp.path(), &complete_args(Some("next"), vec![])).unwrap();
    assert_eq!(saga::load_saga(tmp.path()).unwrap().current_step, 4);
    assert_eq!(load(tmp.path(), 4).slug, "next");
}

#[test]
fn reserving_the_current_step_moves_the_cursor_and_unreserve_takes_it_back() {
    let tmp = tempdir().unwrap();
    saga_with(tmp.path(), 3);
    set_cursor(tmp.path(), 2);
    reserve::reserve(tmp.path(), 2, "cloud agent").unwrap();
    assert_eq!(
        load(tmp.path(), 2).reserved_for.as_deref(),
        Some("cloud agent")
    );
    assert_eq!(saga::load_saga(tmp.path()).unwrap().current_step, 3);
    let saga_dir = saga::saga_dir(tmp.path());
    assert!(
        step::stranded(&saga_dir, 3)
            .unwrap()
            .iter()
            .all(|s| s.number != 2)
    );
    reserve::unreserve(tmp.path(), 2).unwrap();
    assert!(load(tmp.path(), 2).reserved_for.is_none());
    assert!(
        step::stranded(&saga_dir, 3)
            .unwrap()
            .iter()
            .any(|s| s.number == 2)
    );
    assert!(reserve::unreserve(tmp.path(), 2).is_err());
}

#[test]
fn closing_a_reserved_step_ends_the_reservation() {
    let tmp = tempdir().unwrap();
    saga_with(tmp.path(), 3);
    reserve::reserve(tmp.path(), 3, "lane").unwrap();
    close::run(tmp.path(), &close_args(3, &[], Some("lane's PR"))).unwrap();
    let s3 = load(tmp.path(), 3);
    assert_eq!(s3.status, StepStatus::Completed);
    assert!(s3.reserved_for.is_none());
    assert!(
        reserve::reserve(tmp.path(), 3, "x").is_err(),
        "a completed step cannot be reserved"
    );
}

#[test]
fn edit_replaces_a_pending_prompt_but_not_a_completed_one() {
    let tmp = tempdir().unwrap();
    saga_with(tmp.path(), 3);
    edit::run(tmp.path(), 2, "the corrected prompt").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());
    let dir = step::find_step_dir(&saga_dir, 2).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("prompt.md")).unwrap(),
        "the corrected prompt"
    );
    assert_eq!(load(tmp.path(), 2).description, "the corrected prompt");
    assert!(edit::run(tmp.path(), 2, "  ").is_err());
    close::run(tmp.path(), &close_args(2, &[], Some("x"))).unwrap();
    assert!(edit::run(tmp.path(), 2, "rewrite history").is_err());
}

#[test]
fn step_files_without_the_new_fields_still_load_and_stay_unchanged() {
    let tmp = tempdir().unwrap();
    saga_with(tmp.path(), 2);
    let saga_dir = saga::saga_dir(tmp.path());
    let toml =
        std::fs::read_to_string(step::find_step_dir(&saga_dir, 2).unwrap().join("step.toml"))
            .unwrap();
    assert!(
        !toml.contains("reserved_for") && !toml.contains("done_elsewhere"),
        "{toml}"
    );
    let s2 = load(tmp.path(), 2);
    assert!(s2.reserved_for.is_none() && s2.done_elsewhere.is_none());
}
