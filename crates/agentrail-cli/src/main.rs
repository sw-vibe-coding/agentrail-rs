use clap::{Parser, Subcommand};
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::LazyLock;

use agentrail_cli::commands;

static VERSION: LazyLock<String> = LazyLock::new(|| {
    let version = env!("CARGO_PKG_VERSION");
    let commit = env!("BUILD_COMMIT");
    let commit_full = env!("BUILD_COMMIT_FULL");
    let timestamp = env!("BUILD_TIMESTAMP");
    let host = env!("BUILD_HOST");
    format!(
        "{version}\n\
         Copyright (c) 2026 Michael A Wright\n\
         License: MIT\n\
         Repository: https://github.com/sw-vibe-coding/agentrail-rs\n\
         \n\
         Build Information:\n\
         {}\
         {}\
         {}",
        if !host.is_empty() {
            format!("  Host: {host}\n")
        } else {
            String::new()
        },
        if !commit.is_empty() {
            format!("  Commit: {commit} ({commit_full})\n")
        } else {
            String::new()
        },
        if !timestamp.is_empty() {
            format!("  Timestamp: {timestamp}\n")
        } else {
            String::new()
        },
    )
});

#[derive(Parser)]
#[command(name = "agentrail", version = &**VERSION)]
#[command(about = "Workflow CLI for keeping AI agents on track")]
struct Cli {
    /// Path to the project directory (default: current directory)
    #[arg(long, default_value = ".")]
    saga: PathBuf,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new saga
    Init {
        /// Name for the saga
        #[arg(long)]
        name: String,
        /// Plan: file path, literal text, or "-" for stdin
        #[arg(long)]
        plan: String,
    },
    /// Show current saga state
    Status,
    /// Output current step prompt and context for a fresh agent session
    Next,
    /// Mark the current step as in-progress
    Begin,
    /// Complete current step, optionally define next step
    Complete {
        /// Summary: text, file path, or "-" for stdin
        #[arg(long)]
        summary: Option<String>,
        /// Slug for the next step
        #[arg(long)]
        next_slug: Option<String>,
        /// Prompt for the next step: text, file path, or "-" for stdin
        #[arg(long)]
        next_prompt: Option<String>,
        /// Context file paths for next step
        #[arg(long, value_delimiter = ',')]
        next_context: Vec<String>,
        /// Role for the next step (meta, production, deterministic, validation)
        #[arg(long, default_value = "legacy")]
        next_role: String,
        /// Task type for the next step (e.g., "tts", "ffmpeg-concat")
        #[arg(long)]
        next_task_type: Option<String>,
        /// Planned future steps, each "slug: description"
        #[arg(long)]
        planned: Vec<String>,
        /// Mark the saga as complete
        #[arg(long)]
        done: bool,
        /// Reward for trajectory recording (-1, 0, or 1; default: 1)
        #[arg(long)]
        reward: Option<i8>,
        /// Actions taken (for trajectory recording; defaults to summary)
        #[arg(long)]
        actions: Option<String>,
        /// Failure mode identifier (for trajectory on failure)
        #[arg(long)]
        failure_mode: Option<String>,
    },
    /// View or update the saga plan
    Plan {
        /// Update plan: file path, literal text, or "-" for stdin
        #[arg(long)]
        update: Option<String>,
    },
    /// Show all step summaries
    History,
    /// Distill trajectories into a skill document
    Distill {
        /// Task type to distill (e.g., "tts", "ffmpeg-concat")
        task_type: String,
    },
    /// Auto-execute deterministic steps, pause at agent steps
    RunLoop,
    /// Mark current step as blocked
    Abort {
        /// Reason for blocking
        #[arg(long)]
        reason: Option<String>,
    },
}

fn main() -> ExitCode {
    let cli = Cli::parse();
    match dispatch(&cli.saga, cli.command) {
        Ok(code) => ExitCode::from(code),
        Err(e) => {
            eprintln!("Error: {e}");
            ExitCode::from(1)
        }
    }
}

fn dispatch(saga_path: &std::path::Path, command: Commands) -> agentrail_core::error::Result<u8> {
    match command {
        Commands::Init { name, plan } => commands::init::run(saga_path, &name, &plan).map(|_| 0),
        Commands::Status => commands::status::run(saga_path).map(|_| 0),
        Commands::Next => commands::next::run(saga_path),
        Commands::Begin => commands::begin::run(saga_path).map(|_| 0),
        Commands::Complete {
            summary,
            next_slug,
            next_prompt,
            next_context,
            next_role,
            next_task_type,
            planned,
            done,
            reward,
            actions,
            failure_mode,
        } => {
            let args = commands::complete::CompleteArgs {
                summary: summary.as_deref(),
                next_slug: next_slug.as_deref(),
                next_prompt: next_prompt.as_deref(),
                next_context,
                next_role: &next_role,
                next_task_type: next_task_type.as_deref(),
                planned,
                done,
                reward,
                actions: actions.as_deref(),
                failure_mode: failure_mode.as_deref(),
            };
            commands::complete::run(saga_path, &args).map(|_| 0)
        }
        Commands::Distill { task_type } => commands::distill::run(saga_path, &task_type).map(|_| 0),
        Commands::RunLoop => commands::run_loop::run(saga_path),
        Commands::Plan { update } => commands::plan::run(saga_path, update.as_deref()).map(|_| 0),
        Commands::History => commands::history::run(saga_path).map(|_| 0),
        Commands::Abort { reason } => commands::abort::run(saga_path, reason.as_deref()).map(|_| 0),
    }
}
