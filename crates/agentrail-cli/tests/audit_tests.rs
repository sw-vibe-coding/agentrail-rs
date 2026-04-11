use agentrail_cli::commands::{add, audit, init};
use agentrail_store::{audit as store_audit, saga, step};
use std::path::Path;
use std::process::Command;
use tempfile::tempdir;

fn init_repo(path: &Path) {
    run_git(path, &["init", "-q", "-b", "main"]);
    run_git(path, &["config", "user.email", "t@t"]);
    run_git(path, &["config", "user.name", "t"]);
    run_git(path, &["config", "commit.gpgsign", "false"]);
}

fn run_git(path: &Path, args: &[&str]) {
    let ok = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .unwrap()
        .status
        .success();
    assert!(ok, "git {args:?} failed");
}

fn commit(path: &Path, file: &str, msg: &str) -> String {
    std::fs::write(path.join(file), msg).unwrap();
    run_git(path, &["add", file]);
    run_git(path, &["commit", "-q", "-m", msg]);
    let out = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(["rev-parse", "HEAD"])
        .output()
        .unwrap();
    String::from_utf8(out.stdout).unwrap().trim().to_string()
}

#[test]
fn audit_cli_runs_on_clean_repo_without_saga() {
    let tmp = tempdir().unwrap();
    let args = audit::AuditArgs {
        since: None,
        emit_commands: false,
    };
    audit::run(tmp.path(), &args).unwrap();
}

#[test]
fn audit_cli_emits_init_and_add_commands_for_unmanaged_history() {
    let tmp = tempdir().unwrap();
    let p = tmp.path();
    init_repo(p);
    commit(p, "a.txt", "feat: first thing");
    commit(p, "b.txt", "fix(core): second thing");

    let report = store_audit::run(p, None).unwrap();
    assert!(report.active_saga_name.is_none());
    assert_eq!(report.orphan_commits.len(), 2);
    assert!(report.has_gaps());

    // Run the CLI layer — we don't capture stdout here, we just ensure it
    // does not error when emit_commands is set.
    let args = audit::AuditArgs {
        since: None,
        emit_commands: true,
    };
    audit::run(p, &args).unwrap();
}

#[test]
fn init_retroactive_flag_persists() {
    let tmp = tempdir().unwrap();
    init::run(tmp.path(), "retro", "plan", true).unwrap();
    let cfg = saga::load_saga(tmp.path()).unwrap();
    assert!(cfg.retroactive);
    assert_eq!(cfg.name, "retro");
}

#[test]
fn add_command_flag_records_full_commit_sha_from_full_ref() {
    let tmp = tempdir().unwrap();
    let p = tmp.path();
    init_repo(p);
    let full = commit(p, "a.txt", "work");

    init::run(p, "s", "plan", false).unwrap();
    add::run(
        p,
        "recovered",
        "did retroactive work",
        "production",
        None,
        std::slice::from_ref(&full),
    )
    .unwrap();

    let saga_dir = saga::saga_dir(p);
    let steps = step::list_steps(&saga_dir).unwrap();
    assert_eq!(steps.len(), 1);
    let cfg = &steps[0].1;
    assert_eq!(cfg.commits, vec![full]);
    assert_eq!(cfg.slug, "recovered");
}

#[test]
fn add_command_flag_normalizes_short_hash_to_full_sha() {
    let tmp = tempdir().unwrap();
    let p = tmp.path();
    init_repo(p);
    let full = commit(p, "a.txt", "work");
    let short = full[..8].to_string();

    init::run(p, "s", "plan", false).unwrap();
    add::run(
        p,
        "recovered",
        "retroactive",
        "production",
        None,
        std::slice::from_ref(&short),
    )
    .unwrap();

    let saga_dir = saga::saga_dir(p);
    let steps = step::list_steps(&saga_dir).unwrap();
    let cfg = &steps[0].1;
    assert_eq!(
        cfg.commits,
        vec![full],
        "short hash should be normalized to the full 40-char SHA"
    );
}

#[test]
fn add_command_flag_rejects_unresolvable_commit_reference() {
    let tmp = tempdir().unwrap();
    let p = tmp.path();
    init_repo(p);
    commit(p, "a.txt", "work");

    init::run(p, "s", "plan", false).unwrap();
    let err = add::run(
        p,
        "recovered",
        "retroactive",
        "production",
        None,
        &["deadbeefdeadbeef".to_string()],
    )
    .unwrap_err();
    assert!(
        err.to_string()
            .contains("could not resolve commit reference"),
        "got unexpected error: {err}"
    );
}

#[test]
fn audit_matches_active_step_after_add_normalizes_short_hash() {
    let tmp = tempdir().unwrap();
    let p = tmp.path();
    init_repo(p);
    let full = commit(p, "a.txt", "step work");
    let short = full[..7].to_string();

    init::run(p, "dev", "plan", false).unwrap();
    add::run(
        p,
        "one",
        "do step one",
        "production",
        None,
        std::slice::from_ref(&short),
    )
    .unwrap();

    // The regression from issue #1: before the fix, the step stored the
    // short hash verbatim and audit reported the commit as an orphan.
    let report = store_audit::run(p, None).unwrap();
    assert_eq!(report.matched.len(), 1);
    assert_eq!(report.matched[0].0.hash, full);
    assert!(report.orphan_commits.is_empty());
    assert!(!report.has_gaps());
}

#[test]
fn audit_matches_active_step_by_recorded_commit() {
    let tmp = tempdir().unwrap();
    let p = tmp.path();
    init_repo(p);
    let h = commit(p, "a.txt", "step one work");

    init::run(p, "dev", "plan", false).unwrap();
    add::run(
        p,
        "one",
        "do step one",
        "production",
        None,
        std::slice::from_ref(&h),
    )
    .unwrap();

    let report = store_audit::run(p, None).unwrap();
    assert_eq!(report.matched.len(), 1);
    assert_eq!(report.matched[0].0.hash, h);
    assert!(report.orphan_commits.is_empty());
    assert!(report.orphan_steps.is_empty());
    assert!(!report.has_gaps());
}

#[test]
fn audit_detects_orphan_commit_in_active_saga() {
    let tmp = tempdir().unwrap();
    let p = tmp.path();
    init_repo(p);
    let first = commit(p, "a.txt", "step one");

    init::run(p, "dev", "plan", false).unwrap();
    add::run(p, "one", "first", "production", None, &[first]).unwrap();

    // New commit after the step that was never added as a step.
    commit(p, "b.txt", "forgotten work");

    let report = store_audit::run(p, None).unwrap();
    assert_eq!(report.matched.len(), 1);
    assert_eq!(report.orphan_commits.len(), 1);
    assert_eq!(report.orphan_commits[0].subject, "forgotten work");
    assert!(report.has_gaps());
}
