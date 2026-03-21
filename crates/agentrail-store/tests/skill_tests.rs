use agentrail_core::{FailureMode, OutputContract, Procedure, Skill};
use agentrail_store::{saga, skill};
use tempfile::tempdir;

fn make_skill(task_type: &str) -> Skill {
    Skill {
        task_type: task_type.to_string(),
        version: 1,
        updated_at: "2026-03-21T10:00:00".to_string(),
        distilled_from: 0,
        procedure: Procedure {
            summary: "Generate TTS audio from a script".to_string(),
            steps: vec![
                "Read the narration script".to_string(),
                "Call the TTS API".to_string(),
                "Save the output WAV".to_string(),
            ],
        },
        success_patterns: vec!["Use Gradio client".to_string()],
        common_failures: vec![FailureMode {
            mode: "wrong_api".to_string(),
            description: "Used HTTP instead of Gradio".to_string(),
            frequency: 3,
        }],
        output_contract: OutputContract {
            required_files: vec!["output.wav".to_string()],
            acceptance_checks: vec!["file-exists".to_string()],
        },
    }
}

#[test]
fn save_and_load_skill() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    let skill = make_skill("tts");
    let path = skill::save_skill(&saga_dir, &skill).unwrap();
    assert!(path.exists());
    assert!(path.to_string_lossy().contains("skills/tts.toml"));

    let loaded = skill::load_skill(&saga_dir, "tts").unwrap().unwrap();
    assert_eq!(loaded.task_type, "tts");
    assert_eq!(loaded.version, 1);
    assert_eq!(loaded.procedure.steps.len(), 3);
    assert_eq!(loaded.success_patterns.len(), 1);
    assert_eq!(loaded.common_failures.len(), 1);
    assert_eq!(loaded.common_failures[0].mode, "wrong_api");
    assert_eq!(loaded.common_failures[0].frequency, 3);
    assert_eq!(loaded.output_contract.required_files, vec!["output.wav"]);
}

#[test]
fn load_skill_returns_none_when_missing() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    let result = skill::load_skill(&saga_dir, "nonexistent").unwrap();
    assert!(result.is_none());
}

#[test]
fn list_skills_empty() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    let types = skill::list_skills(&saga_dir).unwrap();
    assert!(types.is_empty());
}

#[test]
fn list_skills_returns_sorted() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    skill::save_skill(&saga_dir, &make_skill("ffmpeg-concat")).unwrap();
    skill::save_skill(&saga_dir, &make_skill("tts")).unwrap();
    skill::save_skill(&saga_dir, &make_skill("probe")).unwrap();

    let types = skill::list_skills(&saga_dir).unwrap();
    assert_eq!(types, vec!["ffmpeg-concat", "probe", "tts"]);
}

#[test]
fn save_skill_overwrites_existing() {
    let tmp = tempdir().unwrap();
    saga::init_saga(tmp.path(), "s", "p").unwrap();
    let saga_dir = saga::saga_dir(tmp.path());

    let mut skill = make_skill("tts");
    skill::save_skill(&saga_dir, &skill).unwrap();

    skill.version = 2;
    skill.distilled_from = 5;
    skill.procedure.steps.push("Validate duration".to_string());
    skill::save_skill(&saga_dir, &skill).unwrap();

    let loaded = skill::load_skill(&saga_dir, "tts").unwrap().unwrap();
    assert_eq!(loaded.version, 2);
    assert_eq!(loaded.distilled_from, 5);
    assert_eq!(loaded.procedure.steps.len(), 4);
}
