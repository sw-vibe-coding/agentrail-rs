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
        /// Mark this saga as retroactive (reconstructed from prior git history).
        /// `agentrail audit` will treat its commits as claimed when looking
        /// for gaps in future sagas.
        #[arg(long)]
        retroactive: bool,
    },
    /// Bootstrap a project: create saga, CLAUDE.md, and register domain
    Setup {
        /// Name for the saga
        #[arg(long)]
        name: String,
        /// Plan: file path, literal text, or "-" for stdin
        #[arg(long)]
        plan: String,
        /// Path to a domain repo to register
        #[arg(long)]
        domain: Option<String>,
    },
    /// Add a step to the saga (for maintenance mode or ad-hoc tasks)
    Add {
        /// Step slug (short name)
        #[arg(long)]
        slug: String,
        /// Step prompt: text, file path, or "-" for stdin
        #[arg(long)]
        prompt: String,
        /// Step role (production, deterministic, validation, meta)
        #[arg(long, default_value = "production")]
        role: String,
        /// Task type for skill/trajectory lookup
        #[arg(long)]
        task_type: Option<String>,
        /// Git commit reference(s) to associate with this step. Accepts any
        /// revision git rev-parse understands: full SHA, short SHA, tag,
        /// `HEAD~N`, branch name, etc. References are resolved to their full
        /// 40-char commit SHA at add time (via `git rev-parse --verify
        /// <ref>^{commit}`) and stored canonically in `step.toml`, so
        /// `agentrail audit` matches them exactly against git history.
        /// Unresolvable references are rejected. Repeat the flag for
        /// multiple commits.
        #[arg(long = "commit")]
        commits: Vec<String>,
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
    /// Archive current saga to .agentrail-archive/ and clear .agentrail/
    Archive {
        /// Optional reason for archiving (stored in archive-reason.txt)
        #[arg(long)]
        reason: Option<String>,
    },
    /// Compare git history with saga history, report gaps
    ///
    /// Walks first-parent git history on the current branch and matches
    /// commits to saga steps, either by the step's recorded commit hash
    /// (exact) or by timestamp window (heuristic fallback for legacy steps).
    /// Archived sagas' commits are treated as claimed and excluded from gap
    /// analysis.
    ///
    /// Default output is a markdown report with four sections: matched,
    /// orphan commits (no matching step), orphan steps (no matching commit),
    /// and working-tree changes. With `--emit-commands`, prints a shell
    /// script of suggested `agentrail add` lines for orphan commits,
    /// pre-seeded from commit subjects. Review and edit the slugs and
    /// prompts before running the script.
    ///
    /// Handles three cases:
    ///   1. Active saga with gaps: emits `add` lines for orphan commits.
    ///   2. No `.agentrail/` at all: emits `init --retroactive` followed by
    ///      one `add` per historical commit, for bootstrapping an old repo.
    ///   3. Clean: reports no gaps.
    Audit {
        /// Limit to commits after this git revision (e.g. "HEAD~50", "v1.0").
        /// When omitted, scans all first-parent history on the current branch.
        #[arg(long)]
        since: Option<String>,
        /// Emit a shell script of suggested `agentrail add` / `agentrail init`
        /// lines instead of a human-readable report. The script is a draft
        /// meant for human or agent review before execution — slugs and
        /// prompts are derived from commit subjects and usually need
        /// rewording.
        #[arg(long)]
        emit_commands: bool,
    },
    /// Save a point-in-time copy of .agentrail/ into the git object store
    ///
    /// Belt-and-suspenders recovery aid. Creates a proper git commit under
    /// `refs/agentrail/snapshots/<timestamp>` containing a snapshot of
    /// `.agentrail/` (and `.agentrail-archive/` if present), then points a
    /// ref at it so the blobs are reachable and survive `git gc`. The
    /// user's real `.git/index` is never touched — snapshot operations run
    /// against a throwaway index via `GIT_INDEX_FILE`, so there are no
    /// staged-file side effects and no races with pre-commit hooks.
    ///
    /// Intended use: run this manually before a risky agent operation, or
    /// after creating `.agentrail/` files you haven't yet committed, so
    /// that a stray `rm -rf` still has something to recover from. This is
    /// NOT a substitute for normal git tracking — it's a safety net for
    /// files that are not yet staged.
    ///
    /// Restore from a snapshot with a normal git command:
    ///   git restore --source=<ref> -- .agentrail .agentrail-archive
    ///
    /// `agentrail snapshot --list` shows existing snapshot refs.
    Snapshot {
        /// List existing snapshot refs instead of taking a new one
        #[arg(long)]
        list: bool,
    },
    /// Write AGENTS.example.md — an agent instructions template for agentrail
    ///
    /// Produces a self-contained Markdown file with the full session
    /// protocol, the rules for handling `.agentrail/`, and recovery
    /// guidance using `agentrail audit`. Drop it into any project that uses
    /// agentrail (rename to AGENTS.md, CLAUDE.md, .cursorrules, etc.) so
    /// that Claude Code, opencode, Cursor, and similar agents know how to
    /// use agentrail correctly.
    ///
    /// By default writes `AGENTS.example.md` in the project directory; use
    /// `--output` to pick a different path. Refuses to overwrite an
    /// existing file unless `--force` is passed.
    GenAgentsDoc {
        /// Path to write the template to (default: AGENTS.example.md in
        /// the project directory)
        #[arg(long)]
        output: Option<String>,
        /// Overwrite the target file if it already exists
        #[arg(long)]
        force: bool,
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
        Commands::Init {
            name,
            plan,
            retroactive,
        } => commands::init::run(saga_path, &name, &plan, retroactive).map(|_| 0),
        Commands::Setup { name, plan, domain } => {
            commands::setup::run(saga_path, &name, &plan, domain.as_deref()).map(|_| 0)
        }
        Commands::Add {
            slug,
            prompt,
            role,
            task_type,
            commits,
        } => commands::add::run(
            saga_path,
            &slug,
            &prompt,
            &role,
            task_type.as_deref(),
            &commits,
        )
        .map(|_| 0),
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
        Commands::Archive { reason } => {
            commands::archive::run(saga_path, reason.as_deref()).map(|_| 0)
        }
        Commands::Audit {
            since,
            emit_commands,
        } => {
            let args = commands::audit::AuditArgs {
                since: since.as_deref(),
                emit_commands,
            };
            commands::audit::run(saga_path, &args).map(|_| 0)
        }
        Commands::GenAgentsDoc { output, force } => {
            let args = commands::gen_agents_doc::GenArgs {
                output: output.as_deref(),
                force,
            };
            commands::gen_agents_doc::run(saga_path, &args).map(|_| 0)
        }
        Commands::Snapshot { list } => {
            let args = commands::snapshot::SnapshotArgs { list };
            commands::snapshot::run(saga_path, &args).map(|_| 0)
        }
    }
}
