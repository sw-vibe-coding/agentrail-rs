use agentrail_core::SagaConfig;
use agentrail_core::error::Result;
use std::path::{Path, PathBuf};

use crate::saga;

const ARCHIVE_DIR: &str = ".agentrail-archive";

/// Archive the current saga by moving .agentrail/ contents into
/// .agentrail-archive/<name>-<timestamp>/.
///
/// Returns the archive directory path and the archived saga config.
pub fn archive_saga(path: &Path, reason: Option<&str>) -> Result<(PathBuf, SagaConfig)> {
    let config = saga::load_saga(path)?;
    let saga_dir = saga::saga_dir(path);

    let timestamp = agentrail_core::timestamp();
    let dir_name = format!("{}-{}", config.name, timestamp);

    let archive_base = path.join(ARCHIVE_DIR);
    let mut archive_dir = archive_base.join(&dir_name);

    // Handle collision: append counter suffix
    if archive_dir.exists() {
        let mut counter = 2u32;
        loop {
            let candidate = archive_base.join(format!("{dir_name}-{counter}"));
            if !candidate.exists() {
                archive_dir = candidate;
                break;
            }
            counter += 1;
        }
    }

    std::fs::create_dir_all(&archive_dir)?;

    // Move all contents of .agentrail/ into the archive directory
    for entry in std::fs::read_dir(&saga_dir)? {
        let entry = entry?;
        let dest = archive_dir.join(entry.file_name());
        std::fs::rename(entry.path(), dest)?;
    }

    // Write optional reason file
    if let Some(reason) = reason {
        std::fs::write(archive_dir.join("archive-reason.txt"), reason)?;
    }

    // Remove the now-empty .agentrail/ directory
    std::fs::remove_dir(&saga_dir)?;

    Ok((archive_dir, config))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::saga;

    #[test]
    fn archive_moves_saga_contents() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        saga::init_saga(root, "test-saga", "# Plan\nDo stuff").unwrap();
        assert!(saga::saga_exists(root));

        let (archive_dir, config) = archive_saga(root, None).unwrap();

        // .agentrail/ should be gone
        assert!(!saga::saga_exists(root));
        assert!(!root.join(".agentrail").exists());

        // Archive should contain the saga files
        assert!(archive_dir.join("saga.toml").is_file());
        assert!(archive_dir.join("plan.md").is_file());
        assert!(archive_dir.join("steps").is_dir());
        assert!(archive_dir.join("sessions").is_dir());
        assert!(archive_dir.join("trajectories").is_dir());

        // Config should match
        assert_eq!(config.name, "test-saga");

        // Archive dir should be under .agentrail-archive/
        assert!(
            archive_dir
                .to_str()
                .unwrap()
                .contains(".agentrail-archive/test-saga-")
        );
    }

    #[test]
    fn archive_with_reason_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        saga::init_saga(root, "my-saga", "# Plan").unwrap();
        let (archive_dir, _) = archive_saga(root, Some("scope changed")).unwrap();

        let reason = std::fs::read_to_string(archive_dir.join("archive-reason.txt")).unwrap();
        assert_eq!(reason, "scope changed");
    }

    #[test]
    fn archive_no_saga_returns_error() {
        let tmp = tempfile::tempdir().unwrap();
        let result = archive_saga(tmp.path(), None);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("No saga found"));
    }

    #[test]
    fn archive_allows_new_init_after() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        saga::init_saga(root, "first-saga", "# Plan 1").unwrap();
        archive_saga(root, None).unwrap();

        // Should be able to init a new saga now
        saga::init_saga(root, "second-saga", "# Plan 2").unwrap();
        let config = saga::load_saga(root).unwrap();
        assert_eq!(config.name, "second-saga");
    }

    #[test]
    fn archive_collision_appends_counter() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // First archive
        saga::init_saga(root, "dup", "# Plan").unwrap();
        let (_first_dir, _) = archive_saga(root, None).unwrap();

        // Create a new saga and archive again within the same second
        saga::init_saga(root, "dup", "# Plan").unwrap();

        // Force a collision by creating the exact path the next archive would use
        let timestamp = agentrail_core::timestamp();
        let expected_name = format!("dup-{}", timestamp);
        let expected_dir = root.join(".agentrail-archive").join(&expected_name);
        if !expected_dir.exists() {
            std::fs::create_dir_all(&expected_dir).unwrap();
        }

        let (second_dir, _) = archive_saga(root, None).unwrap();

        // Second archive should have a -2 suffix
        let second_name = second_dir.file_name().unwrap().to_str().unwrap();
        assert!(
            second_name.ends_with("-2"),
            "expected -2 suffix, got: {second_name}"
        );
    }
}
