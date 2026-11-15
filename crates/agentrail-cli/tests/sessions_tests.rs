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

#[test]
fn complete_writes_a_transcript_of_just_this_step() {
    let repo = tempdir().unwrap();
    let home = tempdir().unwrap();
    let store = tempdir().unwrap();
    init_git(repo.path());
    let run = |args: &[&str]| {
        let out = agentrail(repo.path(), home.path(), store.path(), args);
        assert!(out.status.success(), "{args:?}: {}", text(&out));
        text(&out)
    };

    let projects = home
        .path()
        .join(".claude/projects")
        .join(agentrail_store::session::projects_dir_name(repo.path()));
    std::fs::create_dir_all(&projects).unwrap();
    let log = projects.join("sess-1.jsonl");
    let append = |lines: &[&str]| {
        use std::io::Write;
        let mut f = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log)
            .unwrap();
        for l in lines {
            writeln!(f, "{l}").unwrap();
        }
    };

    run(&["init", "--name", "s", "--plan", "p"]);
    append(&[r#"{"type":"user","timestamp":"T0","message":{"content":"planning chatter"}}"#]);
    run(&[
        "complete",
        "--summary",
        "planned",
        "--next-slug",
        "one",
        "--next-prompt",
        "do one",
    ]);

    append(&[
        r#"{"type":"user","timestamp":"T1","message":{"content":"please do step one"}}"#,
        r#"{"type":"assistant","timestamp":"T2","message":{"id":"m1","content":[{"type":"tool_use","name":"Read","input":{"file_path":"/private/thing"}}],"usage":{"input_tokens":3,"output_tokens":40}}}"#,
        r#"{"type":"user","timestamp":"T3","toolUseResult":"PRIVATE BODY","message":{"content":[{"type":"tool_result","content":"PRIVATE BODY"}]}}"#,
        r#"{"type":"assistant","timestamp":"T4","message":{"id":"m2","content":[{"type":"text","text":"Step one is done."}],"usage":{"input_tokens":5,"output_tokens":9}}}"#,
    ]);
    let out = run(&[
        "complete",
        "--summary",
        "did one",
        "--next-slug",
        "two",
        "--next-prompt",
        "do two",
    ]);
    assert!(out.contains("transcript.md"), "{out}");

    let transcript =
        std::fs::read_to_string(repo.path().join(".agentrail/steps/001-one/transcript.md"))
            .unwrap();
    assert!(transcript.contains("# Transcript: step 001-one"));
    assert!(transcript.contains("please do step one"));
    assert!(transcript.contains("Step one is done."));
    assert!(transcript.contains("- log lines: 2–5 (4 lines)"));
    assert!(transcript.contains("8 input, 49 output"));
    // Not the earlier step's conversation, and nothing the agent read.
    assert!(!transcript.contains("planning chatter"));
    assert!(!transcript.contains("PRIVATE"));
    assert!(!transcript.contains("/private/thing"));
    // The recorded hash is the store copy's.
    let key = agentrail_store::session::projects_dir_name(repo.path());
    let sha =
        agentrail_store::digest::sha256_file(&store.path().join(key).join("sess-1.jsonl")).unwrap();
    assert!(transcript.contains(&sha));

    // Opting out: no transcript for the next step.
    run(&["sessions", "transcript", "off"]);
    append(&[r#"{"type":"user","timestamp":"T5","message":{"content":"now step two"}}"#]);
    run(&["complete", "--summary", "did two", "--done"]);
    assert!(
        !repo
            .path()
            .join(".agentrail/steps/002-two/transcript.md")
            .exists()
    );
    let saga = std::fs::read_to_string(repo.path().join(".agentrail/saga.toml")).unwrap();
    assert!(saga.contains("no_transcript = true"));
}
