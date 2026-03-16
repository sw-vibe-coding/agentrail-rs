//! Trajectory storage and retrieval for ICRL.
//!
//! Trajectories are stored as individual JSON files under:
//!   .agentrail/trajectories/{task_type}/run_NNN.json

use agentrail_core::Trajectory;
use agentrail_core::error::Result;
use std::path::{Path, PathBuf};

/// Save a trajectory record.
pub fn save_trajectory(saga_dir: &Path, trajectory: &Trajectory) -> Result<PathBuf> {
    let task_dir = saga_dir.join("trajectories").join(&trajectory.task_type);
    std::fs::create_dir_all(&task_dir)?;

    let next_num = next_run_number(&task_dir)?;
    let filename = format!("run_{:03}.json", next_num);
    let path = task_dir.join(&filename);

    let content = serde_json::to_string_pretty(trajectory)?;
    std::fs::write(&path, content)?;

    Ok(path)
}

/// Retrieve the N most recent successful trajectories for a task type.
pub fn retrieve_successes(
    saga_dir: &Path,
    task_type: &str,
    limit: usize,
) -> Result<Vec<Trajectory>> {
    let task_dir = saga_dir.join("trajectories").join(task_type);
    if !task_dir.is_dir() {
        return Ok(vec![]);
    }

    let mut trajectories = load_all_trajectories(&task_dir)?;
    // Keep only successes (reward > 0), most recent first
    trajectories.retain(|t| t.reward > 0);
    trajectories.reverse();
    trajectories.truncate(limit);
    Ok(trajectories)
}

/// Retrieve all trajectories for a task type.
pub fn load_all_trajectories(task_dir: &Path) -> Result<Vec<Trajectory>> {
    let mut entries: Vec<PathBuf> = std::fs::read_dir(task_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "json"))
        .map(|e| e.path())
        .collect();

    entries.sort();

    let mut trajectories = Vec::new();
    for path in entries {
        let content = std::fs::read_to_string(&path)?;
        let t: Trajectory = serde_json::from_str(&content)?;
        trajectories.push(t);
    }

    Ok(trajectories)
}

fn next_run_number(task_dir: &Path) -> Result<u32> {
    let mut max = 0u32;
    if task_dir.is_dir() {
        for entry in std::fs::read_dir(task_dir)? {
            let entry = entry?;
            if let Some(name) = entry.file_name().to_str()
                && let Some(num_str) = name
                    .strip_prefix("run_")
                    .and_then(|s| s.strip_suffix(".json"))
                && let Ok(n) = num_str.parse::<u32>()
            {
                max = max.max(n);
            }
        }
    }
    Ok(max + 1)
}
