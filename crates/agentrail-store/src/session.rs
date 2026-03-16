//! Reads and snapshots Claude Code session JSONL files from ~/.claude/projects/.

use agentrail_core::error::{Error, Result};
use serde_json::Value;
use std::path::{Path, PathBuf};

/// Derive the Claude projects directory name from a working directory path.
pub fn projects_dir_name(cwd: &Path) -> String {
    let abs = cwd.canonicalize().unwrap_or_else(|_| cwd.to_path_buf());
    mangle_path(&abs)
}

fn projects_dir_name_raw(cwd: &Path) -> String {
    mangle_path(cwd)
}

fn mangle_path(path: &Path) -> String {
    let name = path.to_string_lossy().replace('/', "-");
    if name.len() > 1 {
        name.trim_end_matches('-').to_string()
    } else {
        name
    }
}

/// Return the full path to the Claude projects directory for a given cwd.
pub fn claude_projects_dir(cwd: &Path) -> Result<PathBuf> {
    let home =
        dirs::home_dir().ok_or_else(|| Error::Other("Cannot determine home directory".into()))?;
    let projects = home.join(".claude").join("projects");

    let canonical_dir = projects.join(projects_dir_name(cwd));
    if canonical_dir.is_dir() {
        return Ok(canonical_dir);
    }

    let raw_dir = projects.join(projects_dir_name_raw(cwd));
    if raw_dir.is_dir() {
        return Ok(raw_dir);
    }

    Ok(canonical_dir)
}

/// Find all session JSONL files, sorted by modification time (most recent last).
pub fn find_session_files(projects_dir: &Path) -> Result<Vec<PathBuf>> {
    if !projects_dir.is_dir() {
        return Ok(vec![]);
    }
    let mut files: Vec<PathBuf> = std::fs::read_dir(projects_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().extension().is_some_and(|ext| ext == "jsonl"))
        .map(|e| e.path())
        .collect();

    files.sort_by_key(|p| {
        p.metadata()
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH)
    });
    Ok(files)
}

/// Extract human-readable conversation text from a JSONL file.
pub fn extract_conversation(jsonl_path: &Path) -> Result<String> {
    let content = std::fs::read_to_string(jsonl_path)?;
    let mut output = String::new();

    for line in content.lines() {
        let Some(v) = serde_json::from_str::<Value>(line).ok() else {
            continue;
        };
        let Some(record_type) = v.get("type").and_then(|t| t.as_str()) else {
            continue;
        };
        let ts = v.get("timestamp").and_then(|t| t.as_str()).unwrap_or("?");

        match record_type {
            "user" => {
                if let Some(msg) = v.get("message") {
                    let text = extract_text_content(msg);
                    if !text.is_empty() {
                        output.push_str(&format!("[{ts}] USER:\n{text}\n\n"));
                    }
                }
            }
            "assistant" => {
                if let Some(msg) = v.get("message") {
                    let text = extract_assistant_blocks(msg);
                    if !text.is_empty() {
                        output.push_str(&format!("[{ts}] ASSISTANT:\n{text}\n\n"));
                    }
                }
            }
            _ => {}
        }
    }

    Ok(output)
}

fn extract_text_content(msg: &Value) -> String {
    if let Some(content) = msg.get("content") {
        if let Some(s) = content.as_str() {
            return s.trim().to_string();
        }
        if let Some(arr) = content.as_array() {
            let mut parts = Vec::new();
            for item in arr {
                if let Some(text) = item.get("text").and_then(|t| t.as_str()) {
                    let trimmed = text.trim();
                    if !trimmed.is_empty() {
                        parts.push(trimmed.to_string());
                    }
                }
            }
            return parts.join("\n");
        }
    }
    String::new()
}

fn extract_assistant_blocks(msg: &Value) -> String {
    if let Some(arr) = msg.get("content").and_then(|c| c.as_array()) {
        let mut parts = Vec::new();
        for item in arr {
            if item.get("type").and_then(|t| t.as_str()) == Some("text")
                && let Some(text) = item.get("text").and_then(|t| t.as_str())
            {
                let trimmed = text.trim();
                if !trimmed.is_empty() {
                    parts.push(trimmed.to_string());
                }
            }
            if item.get("type").and_then(|t| t.as_str()) == Some("tool_use")
                && let Some(name) = item.get("name").and_then(|n| n.as_str())
            {
                parts.push(format!("[tool: {name}]"));
            }
        }
        return parts.join("\n");
    }
    String::new()
}

/// Snapshot the latest session JSONL into the saga's sessions directory.
pub fn snapshot_session(saga_dir: &Path, cwd: &Path) -> Result<(PathBuf, usize)> {
    let projects_dir = claude_projects_dir(cwd)?;
    let session_files = find_session_files(&projects_dir)?;

    let latest = session_files.last().ok_or_else(|| {
        Error::Other(format!(
            "No session JSONL files found in {}",
            projects_dir.display()
        ))
    })?;

    let sessions_dir = saga_dir.join("sessions");
    std::fs::create_dir_all(&sessions_dir)?;

    let session_name = latest
        .file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .to_string();

    let snapshot_path = sessions_dir.join(format!("{session_name}.jsonl"));
    let new_lines = append_new_lines(latest, &snapshot_path)?;

    Ok((snapshot_path, new_lines))
}

fn append_new_lines(source: &Path, snapshot: &Path) -> Result<usize> {
    let source_content = std::fs::read_to_string(source)?;
    let source_lines: Vec<&str> = source_content.lines().collect();

    let existing_lines = if snapshot.is_file() {
        std::fs::read_to_string(snapshot)?.lines().count()
    } else {
        0
    };

    let new_lines = source_lines.len().saturating_sub(existing_lines);

    if new_lines > 0 {
        let new_content: String = source_lines[existing_lines..]
            .iter()
            .map(|l| format!("{l}\n"))
            .collect();

        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(snapshot)?;
        file.write_all(new_content.as_bytes())?;
    }

    Ok(new_lines)
}
