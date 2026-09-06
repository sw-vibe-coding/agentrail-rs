//! Help-text contract.
//!
//! `-h` is the terse form and `--help` is the full one; agents are told
//! to use `--help`, so that is where the usable guidance has to live.
//! These assert the split holds — a `after_help` (rather than
//! `after_long_help`) would leak the examples into `-h` and break it.

use std::process::Command;

fn help(args: &[&str]) -> String {
    let out = Command::new(env!("CARGO_BIN_EXE_agentrail"))
        .args(args)
        .output()
        .expect("run agentrail");
    assert!(out.status.success(), "help should exit 0: {args:?}");
    String::from_utf8(out.stdout).unwrap()
}

#[test]
fn top_level_help_points_agents_at_rename_for_parallel_branches() {
    let text = help(&["--help"]);
    let line = text
        .lines()
        .find(|l| l.trim_start().starts_with("rename "))
        .expect("rename should be listed in top-level help");
    // The one-liner is all an agent sees when scanning the command list,
    // so it has to carry the trigger words for the problem it solves.
    for keyword in ["parallel", "retroactive"] {
        assert!(
            line.to_lowercase().contains(keyword),
            "top-level rename summary should mention {keyword:?}: {line}"
        );
    }
}

#[test]
fn rename_long_help_carries_the_retroactive_recipe() {
    for args in [
        ["rename", "--help"].as_slice(),
        ["rename", "prefix", "--help"].as_slice(),
    ] {
        let text = help(args);
        assert!(
            text.contains("agentrail rename prefix rtx5060"),
            "{args:?} should show a worked example"
        );
        assert!(
            text.contains("--dry-run"),
            "{args:?} should show the preview flag"
        );
        assert!(
            text.contains("git add -A"),
            "{args:?} should show how to commit a directory move"
        );
        assert!(
            text.to_lowercase().contains("already"),
            "{args:?} should say it is safe on work already done"
        );
        assert!(
            text.to_lowercase().contains("archive"),
            "{args:?} should mention archiving before the merge"
        );
    }
}

#[test]
fn rename_short_help_stays_short() {
    for (short, long) in [
        (["rename", "-h"].as_slice(), ["rename", "--help"].as_slice()),
        (
            ["rename", "prefix", "-h"].as_slice(),
            ["rename", "prefix", "--help"].as_slice(),
        ),
    ] {
        let terse = help(short);
        assert!(
            !terse.contains("EXAMPLE"),
            "-h must stay terse; examples belong in --help only: {short:?}"
        );
        assert!(
            terse.len() < help(long).len(),
            "-h should be shorter than --help: {short:?}"
        );
    }
}
