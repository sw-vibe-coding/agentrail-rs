use std::process::Command;

fn main() {
    // Re-run when HEAD moves so the embedded commit/timestamp doesn't go
    // stale if main.rs hasn't been touched. Without these, a `cargo build`
    // after a new commit may keep the old BUILD_COMMIT compiled in.
    println!("cargo:rerun-if-changed=../../.git/HEAD");
    println!("cargo:rerun-if-changed=../../.git/refs/heads");

    // Git commit hash
    let commit = Command::new("git")
        .args(["rev-parse", "--short", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    println!("cargo:rustc-env=BUILD_COMMIT={}", commit.trim());

    // Full commit hash
    let commit_full = Command::new("git")
        .args(["rev-parse", "HEAD"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    println!("cargo:rustc-env=BUILD_COMMIT_FULL={}", commit_full.trim());

    // Build timestamp
    let timestamp = Command::new("date")
        .args(["+%Y-%m-%dT%H:%M:%S%z"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    println!("cargo:rustc-env=BUILD_TIMESTAMP={}", timestamp.trim());

    // Hostname
    let host = Command::new("hostname")
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .unwrap_or_default();
    println!("cargo:rustc-env=BUILD_HOST={}", host.trim());
}
