use agentrail_core::Skill;
use agentrail_core::error::Result;
use std::path::{Path, PathBuf};

/// Path to a skill file within the saga directory.
fn skill_path(saga_dir: &Path, task_type: &str) -> PathBuf {
    saga_dir.join("skills").join(format!("{task_type}.toml"))
}

/// Load a skill document for a task type. Returns None if not found.
pub fn load_skill(saga_dir: &Path, task_type: &str) -> Result<Option<Skill>> {
    let path = skill_path(saga_dir, task_type);
    if !path.is_file() {
        return Ok(None);
    }
    let content = std::fs::read_to_string(&path)?;
    let skill: Skill = toml::from_str(&content)?;
    Ok(Some(skill))
}

/// Save a skill document. Creates the skills/ directory if needed.
pub fn save_skill(saga_dir: &Path, skill: &Skill) -> Result<PathBuf> {
    let dir = saga_dir.join("skills");
    std::fs::create_dir_all(&dir)?;
    let path = skill_path(saga_dir, &skill.task_type);
    let content = toml::to_string_pretty(skill)?;
    std::fs::write(&path, content)?;
    Ok(path)
}

/// List all available skill task types.
pub fn list_skills(saga_dir: &Path) -> Result<Vec<String>> {
    let dir = saga_dir.join("skills");
    if !dir.is_dir() {
        return Ok(vec![]);
    }
    let mut types = Vec::new();
    for entry in std::fs::read_dir(&dir)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "toml")
            && let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        {
            types.push(stem.to_string());
        }
    }
    types.sort();
    Ok(types)
}
