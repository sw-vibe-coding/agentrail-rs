//! Session logs live outside the repository, end to end through the binary.
//!
//! Driven through the real binary so each test can point HOME and
//! `AGENTRAIL_SESSIONS_DIR` at temp directories without touching the
//! process-global environment of the other tests.

use std::path::Path;
use std::process::{Command, Output};
use tempfile::tempdir;

fn git(path: &Path, args: &[&str]) -> String {
    let out = Command::new("git")
        .arg("-C")
        .arg(path)
        .args(args)
        .output()
        .unwrap();
    assert!(out.status.success(), "git {args:?} failed");
    String::from_utf8_lossy(&out.stdout).to_string()
}

fn init_git(path: &Path) {
    git(path, &["init", "-q"]);
    git(path, &["config", "user.email", "t@t"]);
    git(path, &["config", "user.name", "t"]);
}

fn agentrail(repo: &Path, home: &Path, store: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_agentrail"))
        .arg("--saga")
        .arg(repo)
        .args(args)
        .env("HOME", home)
        .env("AGENTRAIL_SESSIONS_DIR", store)
        .output()
        .expect("run agentrail")
}

fn text(out: &Output) -> String {
    format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    )
}

#[test]
fn complete_snapshots_the_log_outside_the_repository() {
    let repo = tempdir().unwrap();
    let home = tempdir().unwrap();
    let store = tempdir().unwrap();
    init_git(repo.path());

    // A Claude Code session log for this repository.
    let projects = home
        .path()
        .join(".claude/projects")
        .join(agentrail_store::session::projects_dir_name(repo.path()));
    std::fs::create_dir_all(&projects).unwrap();
    std::fs::write(projects.join("sess-1.jsonl"), "{\"n\":1}\n{\"n\":2}\n").unwrap();

    let out = agentrail(
        repo.path(),
        home.path(),
        store.path(),
        &["init", "--name", "s", "--plan", "p"],
    );
    assert!(out.status.success(), "{}", text(&out));
    let out = agentrail(
        repo.path(),
        home.path(),
        store.path(),
        &["complete", "--summary", "did it", "--done"],
    );
    assert!(out.status.success(), "{}", text(&out));

    // The log is in the store...
    let key = agentrail_store::session::projects_dir_name(repo.path());
    let copy = store.path().join(key).join("sess-1.jsonl");
    assert_eq!(
        std::fs::read_to_string(&copy).unwrap(),
        "{\"n\":1}\n{\"n\":2}\n"
    );
    // ...and nothing session-shaped is anywhere git could commit it.
    assert!(!repo.path().join(".agentrail/sessions").exists());
    let committable = git(
        repo.path(),
        &["ls-files", "--others", "--cached", "--exclude-standard"],
    );
    assert!(!committable.contains("jsonl"), "{committable}");

    let out = agentrail(
        repo.path(),
        home.path(),
        store.path(),
        &["sessions", "path"],
    );
    assert_eq!(
        text(&out).trim(),
        copy.parent().unwrap().display().to_string()
    );
}

#[test]
fn in_repo_logs_are_reported_then_migrated() {
    let repo = tempdir().unwrap();
    let home = tempdir().unwrap();
    let store = tempdir().unwrap();
    init_git(repo.path());
    let run = |args: &[&str]| agentrail(repo.path(), home.path(), store.path(), args);

    assert!(
        run(&["init", "--name", "s", "--plan", "p"])
            .status
            .success()
    );
    // What an older agentrail left behind, committed.
    std::fs::create_dir_all(repo.path().join(".agentrail/sessions")).unwrap();
    std::fs::write(repo.path().join(".agentrail/sessions/old.jsonl"), "1\n").unwrap();
    git(repo.path(), &["add", "-A"]);
    git(repo.path(), &["commit", "-q", "-m", "saga"]);

    for cmd in ["next", "status"] {
        let out = run(&[cmd]);
        assert!(
            text(&out).contains("agentrail sessions migrate"),
            "{cmd} should point at the fix: {}",
            text(&out)
        );
    }

    let out = run(&["sessions", "migrate"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("moved"));
    assert!(text(&out).contains("removal is staged"));

    assert!(!repo.path().join(".agentrail/sessions").exists());
    assert!(!git(repo.path(), &["ls-files"]).contains("sessions"));
    let key = agentrail_store::session::projects_dir_name(repo.path());
    assert!(store.path().join(key).join("old.jsonl").is_file());

    let out = run(&["status"]);
    assert!(!text(&out).contains("sessions migrate"), "{}", text(&out));
    let out = run(&["sessions", "migrate"]);
    assert!(text(&out).contains("Nothing to do"));
}

#[test]
fn check_size_passes_on_a_normal_saga() {
    let repo = tempdir().unwrap();
    let home = tempdir().unwrap();
    let store = tempdir().unwrap();
    init_git(repo.path());
    let run = |args: &[&str]| agentrail(repo.path(), home.path(), store.path(), args);
    assert!(
        run(&["init", "--name", "s", "--plan", "p"])
            .status
            .success()
    );
    for args in [["check-size", ""], ["check-size", "--staged"]] {
        let args: Vec<&str> = args.into_iter().filter(|a| !a.is_empty()).collect();
        let out = run(&args);
        assert_eq!(out.status.code(), Some(0), "{}", text(&out));
        assert!(text(&out).contains("No oversize files"));
    }
}

#[test]
fn briefing_carries_the_privacy_rule_and_no_longer_commits_logs() {
    let body = agentrail_store::instructions::EMBEDDED_FRAGMENTS
        .iter()
        .map(|(_, c)| *c)
        .collect::<String>();
    assert!(body.contains("Session logs stay out of the repository"));
    assert!(body.contains("quotes every file you read"));
    assert!(!body.contains("JSONLs under `sessions/`"));
}
