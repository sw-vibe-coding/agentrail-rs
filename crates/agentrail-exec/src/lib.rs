//! Deterministic step executors.
//!
//! Executors run fully-specified jobs without agent involvement.
//! The shell executor pipes JobSpec params as JSON to a script's stdin
//! and reads a result JSON from stdout.

use agentrail_core::JobSpec;
use agentrail_core::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

/// Result of executing a job.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExecutionResult {
    pub success: bool,
    #[serde(default)]
    pub outputs: serde_json::Value,
    #[serde(default)]
    pub error: Option<String>,
}

/// Execute a job by finding the executor script in a domain directory
/// and running it with the job params piped as JSON to stdin.
pub fn run_shell_executor(domain_dir: &Path, job: &JobSpec) -> Result<ExecutionResult> {
    let executor_path = find_executor(domain_dir, &job.kind)?;
    let params_json = serde_json::to_string(&job.params)?;

    let output = Command::new("bash")
        .arg(&executor_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(params_json.as_bytes())?;
            }
            child.wait_with_output()
        })
        .map_err(|e| Error::JobFailed(format!("Failed to run executor {}: {}", job.kind, e)))?;

    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let result: ExecutionResult =
            serde_json::from_str(stdout.trim()).unwrap_or(ExecutionResult {
                success: true,
                outputs: serde_json::json!({"stdout": stdout.trim()}),
                error: None,
            });
        Ok(result)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        Ok(ExecutionResult {
            success: false,
            outputs: serde_json::Value::Null,
            error: Some(stderr.trim().to_string()),
        })
    }
}

fn find_executor(domain_dir: &Path, kind: &str) -> Result<std::path::PathBuf> {
    let executors_dir = domain_dir.join("executors");
    for ext in ["sh", "py", "rs"] {
        let path = executors_dir.join(format!("{kind}.{ext}"));
        if path.is_file() {
            return Ok(path);
        }
    }
    Err(Error::JobFailed(format!(
        "No executor found for kind '{}' in {}",
        kind,
        executors_dir.display()
    )))
}
