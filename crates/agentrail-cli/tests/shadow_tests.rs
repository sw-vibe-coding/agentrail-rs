//! `agentrail shadow` end to end through the binary.

use std::path::Path;
use std::process::{Command, Output};
use tempfile::tempdir;

fn agentrail(repo: &Path, store: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_agentrail"))
        .arg("--saga")
        .arg(repo)
        .args(args)
        .env("AGENTRAIL_SESSIONS_DIR", store)
        .env_remove("AGENTRAIL_SHADOW")
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
fn shadow_set_push_status_pull() {
    if !agentrail_store::shadow::xz_available() {
        eprintln!("skipping: xz not installed");
        return;
    }
    let repo = tempdir().unwrap();
    let store = tempdir().unwrap();
    let shadow = tempdir().unwrap();
    let run = |args: &[&str]| agentrail(repo.path(), store.path(), args);

    // Nothing set: push explains how to set one, status exits 1.
    let out = run(&["shadow", "push"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(
        text(&out).contains("agentrail shadow set"),
        "{}",
        text(&out)
    );
    assert_eq!(run(&["shadow", "status"]).status.code(), Some(1));

    // A typo fails at set time.
    let out = run(&["shadow", "set", "/no/such/dir"]);
    assert_eq!(out.status.code(), Some(1));

    let key = agentrail_store::session::projects_dir_name(repo.path());
    let logs = store.path().join(&key);
    std::fs::create_dir_all(&logs).unwrap();
    let body = "{\"n\":1}\n".repeat(200);
    std::fs::write(logs.join("sess.jsonl"), &body).unwrap();

    let shadow_dir = shadow.path().to_string_lossy().to_string();
    let out = run(&["shadow", "set", &shadow_dir]);
    assert!(out.status.success(), "{}", text(&out));

    let out = run(&["shadow", "status"]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(text(&out).contains("to push"));

    let out = run(&["shadow", "push"]);
    assert!(out.status.success(), "{}", text(&out));
    assert!(text(&out).contains("1 log(s) written"));
    assert!(shadow.path().join("sessions/sess.jsonl.xz").is_file());

    let out = run(&["shadow", "status"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(text(&out).contains("in sync"));

    // The log is lost locally; pull brings it back intact.
    std::fs::remove_file(logs.join("sess.jsonl")).unwrap();
    assert!(text(&run(&["shadow", "status"])).contains("to pull"));
    let out = run(&["shadow", "pull"]);
    assert!(out.status.success(), "{}", text(&out));
    assert_eq!(
        std::fs::read_to_string(logs.join("sess.jsonl")).unwrap(),
        body
    );

    // The setting is not in the repository.
    assert!(!repo.path().join(".agentrail").exists());
    let out = run(&["shadow", "unset"]);
    assert!(text(&out).contains("forgotten"));
    assert_eq!(run(&["shadow", "push"]).status.code(), Some(1));
}
