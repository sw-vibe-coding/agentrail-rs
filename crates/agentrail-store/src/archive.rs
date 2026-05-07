use agentrail_core::SagaConfig;
use agentrail_core::error::Result;
use std::path::{Path, PathBuf};

use crate::saga;

const ARCHIVE_DIR: &str = ".agentrail-archive";

/// One archived saga, as enumerated by [`list_archives`]. The `dir` is a
/// drop-in replacement for `saga::saga_dir(...)` — it has the same
/// `saga.toml` / `plan.md` / `steps/` / `trajectories/` layout, so step
/// and trajectory readers work on it directly.
#[derive(Debug, Clone)]
pub struct ArchivedSaga {
    pub dir: PathBuf,
    pub config: SagaConfig,
    /// The directory-name suffix that follows `<saga-name>-`. Usually a
    /// timestamp like `20260418T135657`, possibly with a `-2`/`-3`
    /// collision counter.
    pub suffix: String,
    pub reason: Option<String>,
}

/// Enumerate all archived sagas under `.agentrail-archive/`, newest first.
/// Returns an empty vec if there is no archive directory.
pub fn list_archives(path: &Path) -> Result<Vec<ArchivedSaga>> {
    let archive_base = path.join(ARCHIVE_DIR);
    if !archive_base.is_dir() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for entry in std::fs::read_dir(&archive_base)? {
        let entry = entry?;
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let saga_toml = dir.join("saga.toml");
        if !saga_toml.is_file() {
            continue;
        }
        let raw = std::fs::read_to_string(&saga_toml)?;
        let config: SagaConfig = toml::from_str(&raw)?;
        let dir_name = dir
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();
        let prefix = format!("{}-", config.name);
        let suffix = dir_name
            .strip_prefix(&prefix)
            .map(|s| s.to_string())
            .unwrap_or(dir_name);
        let reason = std::fs::read_to_string(dir.join("archive-reason.txt")).ok();
        out.push(ArchivedSaga {
            dir,
            config,
            suffix,
            reason,
        });
    }
    // Sort newest first by suffix (timestamps sort lexicographically).
    out.sort_by(|a, b| b.suffix.cmp(&a.suffix));
    Ok(out)
}

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
    fn list_archives_returns_empty_when_no_archive_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let archives = list_archives(tmp.path()).unwrap();
        assert!(archives.is_empty());
    }

    #[test]
    fn list_archives_finds_archived_sagas_newest_first() {
        let tmp = tempfile::tempdir().unwrap();
        let root = tmp.path();

        // Archive two sagas; force ordering by hand-creating dirs with
        // distinct suffixes (timestamp() may collide within a second).
        std::fs::create_dir_all(root.join(".agentrail-archive/old-saga-20260101T000000")).unwrap();
        std::fs::write(
            root.join(".agentrail-archive/old-saga-20260101T000000/saga.toml"),
            "name = \"old-saga\"\nstatus = \"completed\"\ncurrent_step = 0\ncreated_at = \"2026-01-01\"\nplan_file = \".agentrail/plan.md\"\nretroactive = false\n",
        )
        .unwrap();
        std::fs::write(
            root.join(".agentrail-archive/old-saga-20260101T000000/archive-reason.txt"),
            "shipped",
        )
        .unwrap();

        std::fs::create_dir_all(root.join(".agentrail-archive/new-saga-20260301T000000")).unwrap();
        std::fs::write(
            root.join(".agentrail-archive/new-saga-20260301T000000/saga.toml"),
            "name = \"new-saga\"\nstatus = \"active\"\ncurrent_step = 3\ncreated_at = \"2026-03-01\"\nplan_file = \".agentrail/plan.md\"\nretroactive = false\n",
        )
        .unwrap();

        let archives = list_archives(root).unwrap();
        assert_eq!(archives.len(), 2);
        // Newest first.
        assert_eq!(archives[0].config.name, "new-saga");
        assert_eq!(archives[1].config.name, "old-saga");
        assert_eq!(archives[0].suffix, "20260301T000000");
        assert_eq!(archives[1].suffix, "20260101T000000");
        assert!(archives[0].reason.is_none());
        assert_eq!(archives[1].reason.as_deref(), Some("shipped"));
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
