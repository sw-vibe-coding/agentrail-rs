//! `agentrail remap` end to end: a real history rewrite, then repair.

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
    assert!(
        out.status.success(),
        "git {args:?}: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    String::from_utf8_lossy(&out.stdout).trim().to_string()
}

fn agentrail(repo: &Path, home: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_agentrail"))
        .arg("--saga")
        .arg(repo)
        .args(args)
        .env("HOME", home)
        .env("AGENTRAIL_SESSIONS_DIR", home.join("sessions"))
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

fn commit(repo: &Path, file: &str, msg: &str) -> String {
    std::fs::write(repo.join(file), msg).unwrap();
    git(repo, &["add", file]);
    git(repo, &["commit", "-q", "-m", msg]);
    git(repo, &["rev-parse", "HEAD"])
}

#[test]
fn remap_repairs_commits_after_a_history_rewrite() {
    let repo = tempdir().unwrap();
    let home = tempdir().unwrap();
    let r = repo.path();
    git(r, &["init", "-q"]);
    git(r, &["config", "user.email", "t@t"]);
    git(r, &["config", "user.name", "t"]);
    let run = |args: &[&str]| agentrail(r, home.path(), args);

    commit(r, "base", "base");
    assert!(
        run(&["init", "--name", "s", "--plan", "p"])
            .status
            .success()
    );
    let old_one = commit(r, "one", "work one");
    assert!(
        run(&[
            "add", "--slug", "one", "--prompt", "p", "--commit", &old_one
        ])
        .status
        .success()
    );
    let old_two = commit(r, "two", "work two");
    assert!(
        run(&[
            "add", "--slug", "two", "--prompt", "p", "--commit", &old_two
        ])
        .status
        .success()
    );

    // Clean before the rewrite.
    let out = run(&["remap"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(text(&out).contains("0 recorded commit(s) dangling"));

    // Rewrite history: same work, new hashes; the old commits are gone.
    git(r, &["reset", "-q", "--hard", "HEAD~2"]);
    let new_one = commit(r, "one", "work one (rewritten)");
    let new_two = commit(r, "two", "work two (rewritten)");
    git(r, &["reflog", "expire", "--expire=now", "--all"]);
    git(r, &["gc", "-q", "--prune=now"]);

    let out = run(&["remap"]);
    assert_eq!(out.status.code(), Some(1), "{}", text(&out));
    assert!(text(&out).contains("2 recorded commit(s) dangling"));

    // One via a filter-repo style commit map, one by hand with a prefix.
    let map = home.path().join("commit-map");
    std::fs::write(&map, format!("old new\n{old_one} {new_one}\n")).unwrap();
    let map = map.to_string_lossy().to_string();

    let out = run(&["remap", &map, "--dry-run"]);
    assert!(text(&out).contains("would rewrite"), "{}", text(&out));
    assert!(text(&run(&["remap"])).contains("2 recorded commit(s) dangling"));

    let out = run(&["remap", &map, "--from", &old_two[..10], "--to", "HEAD"]);
    assert_eq!(out.status.code(), Some(0), "{}", text(&out));
    assert!(text(&out).contains("Rewrote 2 commit hash(es)"));

    let steps = std::fs::read_to_string(r.join(".agentrail/steps/001-one/step.toml")).unwrap()
        + &std::fs::read_to_string(r.join(".agentrail/steps/002-two/step.toml")).unwrap();
    assert!(steps.contains(&new_one) && steps.contains(&new_two));
    assert!(!steps.contains(&old_one) && !steps.contains(&old_two));

    // Unpaired --from/--to is a usage error, not a silent no-op.
    let out = run(&["remap", "--from", "abcdef1"]);
    assert_eq!(out.status.code(), Some(1));
    assert!(text(&out).contains("come in pairs"));
}
