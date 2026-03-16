pub mod error;

use serde::{Deserialize, Serialize};
use std::fmt;

// ---------------------------------------------------------------------------
// Saga model (evolved from avoid-compaction)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SagaConfig {
    pub name: String,
    pub status: SagaStatus,
    pub current_step: u32,
    pub created_at: String,
    pub plan_file: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SagaStatus {
    Active,
    Completed,
}

impl fmt::Display for SagaStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SagaStatus::Active => write!(f, "active"),
            SagaStatus::Completed => write!(f, "completed"),
        }
    }
}

// ---------------------------------------------------------------------------
// Step model (extended with role and execution mode)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepConfig {
    pub number: u32,
    pub slug: String,
    pub status: StepStatus,
    pub description: String,
    pub role: StepRole,
    pub context_files: Vec<String>,
    pub created_at: String,
    #[serde(default)]
    pub completed_at: Option<String>,
    #[serde(default)]
    pub transcript_file: Option<String>,
    /// For deterministic steps: the job spec to execute
    #[serde(default)]
    pub job_spec: Option<JobSpec>,
    /// For agentic steps: the handoff packet path
    #[serde(default)]
    pub packet_file: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepStatus {
    Pending,
    #[serde(rename = "in-progress")]
    InProgress,
    Completed,
    Blocked,
}

impl fmt::Display for StepStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StepStatus::Pending => write!(f, "pending"),
            StepStatus::InProgress => write!(f, "in-progress"),
            StepStatus::Completed => write!(f, "completed"),
            StepStatus::Blocked => write!(f, "blocked"),
        }
    }
}

/// What role does this step play in the orchestration loop?
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StepRole {
    /// Meta agent: prepares ICRL packet / handoff for the next production step
    Meta,
    /// Production agent: executes semantic work using a prepared packet
    Production,
    /// Deterministic: fully specified, no agent needed
    Deterministic,
    /// Validation: check outputs, record results
    Validation,
    /// Legacy: untyped step (backward compat with avoid-compaction)
    #[default]
    Legacy,
}

impl fmt::Display for StepRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            StepRole::Meta => write!(f, "meta"),
            StepRole::Production => write!(f, "production"),
            StepRole::Deterministic => write!(f, "deterministic"),
            StepRole::Validation => write!(f, "validation"),
            StepRole::Legacy => write!(f, "legacy"),
        }
    }
}

// ---------------------------------------------------------------------------
// Deterministic job spec
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JobSpec {
    pub kind: String,
    pub params: serde_json::Value,
}

// ---------------------------------------------------------------------------
// ICRL trajectory model
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Trajectory {
    pub task_type: String,
    pub state: serde_json::Value,
    pub action: String,
    pub result: String,
    pub reward: i8,
    pub timestamp: String,
}

// ---------------------------------------------------------------------------
// Meta handoff packet
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HandoffPacket {
    pub task_id: String,
    pub objective: String,
    #[serde(default)]
    pub inputs: serde_json::Value,
    #[serde(default)]
    pub success_patterns: Vec<String>,
    #[serde(default)]
    pub common_failures: Vec<String>,
    #[serde(default)]
    pub procedure: Vec<String>,
    #[serde(default)]
    pub output_contract: OutputContract,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OutputContract {
    #[serde(default)]
    pub required_files: Vec<String>,
    #[serde(default)]
    pub acceptance_checks: Vec<String>,
}

// ---------------------------------------------------------------------------
// Utilities (from avoid-compaction)
// ---------------------------------------------------------------------------

/// Read input that could be literal text, a file path, or "-" for stdin.
pub fn read_input(value: &str) -> error::Result<String> {
    if value == "-" {
        use std::io::Read;
        let mut buf = String::new();
        std::io::stdin().read_to_string(&mut buf)?;
        Ok(buf)
    } else if std::path::Path::new(value).is_file() {
        Ok(std::fs::read_to_string(value)?)
    } else {
        Ok(value.to_string())
    }
}

/// Truncate a string to `max` characters, appending "..." if truncated.
pub fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        format!("{}...", &s[..max.saturating_sub(3)])
    }
}

/// Get current timestamp as YYYYMMDDTHHMMSS string.
pub fn timestamp() -> String {
    chrono::Local::now().format("%Y%m%dT%H%M%S").to_string()
}

/// Get current timestamp as ISO 8601 string.
pub fn timestamp_iso() -> String {
    chrono::Local::now().format("%Y-%m-%dT%H:%M:%S").to_string()
}
