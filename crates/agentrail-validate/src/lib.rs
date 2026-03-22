//! Output validators and acceptance contracts.
//!
//! Validators check outputs against named acceptance checks.
//! The shell validator pipes context JSON to a script's stdin
//! and checks the exit code (0 = valid, non-zero = invalid).

use agentrail_core::error::{Error, Result};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::process::Command;

/// Result of a validation check.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ValidationResult {
    pub valid: bool,
    #[serde(default)]
    pub details: serde_json::Value,
    #[serde(default)]
    pub error: Option<String>,
}

/// Run a named validator from a domain directory.
/// Pipes context as JSON to stdin, checks exit code and stdout.
pub fn run_shell_validator(
    domain_dir: &Path,
    check_name: &str,
    context: &serde_json::Value,
) -> Result<ValidationResult> {
    let validator_path = find_validator(domain_dir, check_name)?;
    let context_json = serde_json::to_string(context)?;

    let output = Command::new("bash")
        .arg(&validator_path)
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .and_then(|mut child| {
            use std::io::Write;
            if let Some(ref mut stdin) = child.stdin {
                stdin.write_all(context_json.as_bytes())?;
            }
            child.wait_with_output()
        })
        .map_err(|e| {
            Error::ValidationFailed(format!("Failed to run validator {}: {}", check_name, e))
        })?;

    let stdout = String::from_utf8_lossy(&output.stdout);

    if output.status.success() {
        let result: ValidationResult =
            serde_json::from_str(stdout.trim()).unwrap_or(ValidationResult {
                valid: true,
                details: serde_json::json!({"stdout": stdout.trim()}),
                error: None,
            });
        Ok(result)
    } else {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let result: ValidationResult =
            serde_json::from_str(stdout.trim()).unwrap_or(ValidationResult {
                valid: false,
                details: serde_json::Value::Null,
                error: Some(stderr.trim().to_string()),
            });
        Ok(result)
    }
}

fn find_validator(domain_dir: &Path, check_name: &str) -> Result<std::path::PathBuf> {
    let validators_dir = domain_dir.join("validators");
    for ext in ["sh", "py", "rs"] {
        let path = validators_dir.join(format!("{check_name}.{ext}"));
        if path.is_file() {
            return Ok(path);
        }
    }
    Err(Error::ValidationFailed(format!(
        "No validator found for '{}' in {}",
        check_name,
        validators_dir.display()
    )))
}
