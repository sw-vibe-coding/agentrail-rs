use agentrail_core::Trajectory;
use agentrail_store::trajectory;
use serde_json::json;
use tempfile::tempdir;

fn make_trajectory(task_type: &str, reward: i8) -> Trajectory {
    Trajectory {
        task_type: task_type.to_string(),
        state: json!({"key": "value"}),
        action: "do-thing".to_string(),
        result: "ok".to_string(),
        reward,
        timestamp: "2026-01-01T00:00:00".to_string(),
    }
}

#[test]
fn save_and_load_trajectory() {
    let tmp = tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("trajectories")).unwrap();

    let t = make_trajectory("tts", 1);
    let path = trajectory::save_trajectory(tmp.path(), &t).unwrap();
    assert!(path.exists());
    assert!(path.to_string_lossy().contains("run_001.json"));

    let loaded = trajectory::load_all_trajectories(&tmp.path().join("trajectories/tts")).unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].task_type, "tts");
    assert_eq!(loaded[0].reward, 1);
}

#[test]
fn retrieve_successes_filters_by_reward() {
    let tmp = tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("trajectories")).unwrap();

    trajectory::save_trajectory(tmp.path(), &make_trajectory("tts", -1)).unwrap();
    trajectory::save_trajectory(tmp.path(), &make_trajectory("tts", 0)).unwrap();
    trajectory::save_trajectory(tmp.path(), &make_trajectory("tts", 1)).unwrap();

    let successes = trajectory::retrieve_successes(tmp.path(), "tts", 10).unwrap();
    assert_eq!(successes.len(), 1);
    assert_eq!(successes[0].reward, 1);
}

#[test]
fn retrieve_successes_respects_limit() {
    let tmp = tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("trajectories")).unwrap();

    for _ in 0..5 {
        trajectory::save_trajectory(tmp.path(), &make_trajectory("tts", 1)).unwrap();
    }

    let successes = trajectory::retrieve_successes(tmp.path(), "tts", 2).unwrap();
    assert_eq!(successes.len(), 2);
}

#[test]
fn next_run_number_increments() {
    let tmp = tempdir().unwrap();
    std::fs::create_dir_all(tmp.path().join("trajectories")).unwrap();

    let p1 = trajectory::save_trajectory(tmp.path(), &make_trajectory("tts", 1)).unwrap();
    assert!(p1.to_string_lossy().contains("run_001.json"));

    let p2 = trajectory::save_trajectory(tmp.path(), &make_trajectory("tts", 1)).unwrap();
    assert!(p2.to_string_lossy().contains("run_002.json"));

    let p3 = trajectory::save_trajectory(tmp.path(), &make_trajectory("tts", 1)).unwrap();
    assert!(p3.to_string_lossy().contains("run_003.json"));
}
