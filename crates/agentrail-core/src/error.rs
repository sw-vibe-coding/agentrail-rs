use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("No saga found at {path}/.agentrail/")]
    SagaNotFound { path: PathBuf },

    #[error("Saga already exists at {path}/.agentrail/")]
    SagaAlreadyExists { path: PathBuf },

    #[error("Invalid step transition: {from} -> {to}")]
    InvalidStepTransition { from: String, to: String },

    #[error("No current step found")]
    NoCurrentStep,

    #[error("Saga is already complete")]
    SagaComplete,

    #[error("No steps defined yet")]
    NoSteps,

    #[error("Multiple flags cannot read from stdin")]
    MultipleStdin,

    #[error("Job execution failed: {0}")]
    JobFailed(String),

    #[error("Validation failed: {0}")]
    ValidationFailed(String),

    #[error("{0}")]
    Other(String),

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error("TOML parse error: {0}")]
    TomlDeserialize(#[from] toml::de::Error),

    #[error("TOML serialize error: {0}")]
    TomlSerialize(#[from] toml::ser::Error),

    #[error("JSON error: {0}")]
    Json(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
