# Continue from Bootstrap

This document is the handoff from the bootstrapping session to the first real
development session in the agentrail-rs repo.

## What exists now

A Cargo workspace that compiles clean (`cargo check` passes, zero warnings).

### Crate layout

```
agentrail-rs/
├── Cargo.toml              # workspace root, edition 2024
├── CLAUDE.md               # project context (read this first)
├── .gitignore
├── docs/
│   ├── research.txt        # full ICRL research + architecture design document
│   └── continue-from-bootstrap.md  # this file
└── crates/
    ├── agentrail-core/     # domain model (IMPLEMENTED)
    ├── agentrail-store/    # persistence (IMPLEMENTED)
    ├── agentrail-cli/      # CLI binary (STUB — prints "not yet implemented")
    ├── agentrail-exec/     # deterministic executors (STUB — empty)
    └── agentrail-validate/ # validators (STUB — empty)
```

### What is implemented

**agentrail-core** (`crates/agentrail-core/src/`):
- `lib.rs` — Full domain model:
  - `SagaConfig`, `SagaStatus` (from avoid-compaction)
  - `StepConfig`, `StepStatus` (extended with `role`, `job_spec`, `packet_file`)
  - `StepRole` enum: Meta, Production, Deterministic, Validation, Legacy
  - `JobSpec` — deterministic job specification (kind + params as serde_json::Value)
  - `Trajectory` — ICRL record (task_type, state, action, result, reward, timestamp)
  - `HandoffPacket` + `OutputContract` — meta-to-production structured briefing
  - Utility functions: `read_input()`, `truncate()`, `timestamp()`, `timestamp_iso()`
- `error.rs` — Error enum with thiserror: SagaNotFound, SagaAlreadyExists,
  InvalidStepTransition, NoCurrentStep, SagaComplete, NoSteps, MultipleStdin,
  JobFailed, ValidationFailed, Other, Io, TomlDeserialize, TomlSerialize, Json

**agentrail-store** (`crates/agentrail-store/src/`):
- `saga.rs` — init/load/save saga in `.agentrail/` directory
  - Creates `steps/`, `trajectories/`, `sessions/` subdirs on init
- `step.rs` — create/load/save/transition/list steps (NNN-slug format)
  - `create_step()` now takes a `StepRole` parameter
  - `transition_step()` enforces Pending→InProgress→Completed|Blocked
- `session.rs` — Claude Code JSONL session snapshotting (from avoid-compaction)
  - `snapshot_session()`, `extract_conversation()`, `claude_projects_dir()`
- `trajectory.rs` — ICRL trajectory persistence:
  - `save_trajectory()` — writes to `.agentrail/trajectories/{task_type}/run_NNN.json`
  - `retrieve_successes()` — loads N most recent successful (reward > 0) trajectories
  - `load_all_trajectories()` — loads all for a task type

**agentrail-cli** (`crates/agentrail-cli/src/`):
- `main.rs` — Stub only. Prints error and exits 1.

**agentrail-exec** and **agentrail-validate**:
- Stub `lib.rs` with comments only. No implementation.

### What has NO tests yet

Nothing has tests. The avoid-compaction project has 48+ integration tests in
`tests/saga_tests.rs` using tempdir — that pattern should be replicated here.

### Storage directory

All runtime data lives in `.agentrail/` (NOT `.avoid-compaction/`).

```
.agentrail/
├── saga.toml                    # saga metadata
├── plan.md                      # evolving plan
├── steps/
│   └── NNN-slug/
│       ├── step.toml            # step config (includes role, job_spec, packet_file)
│       ├── prompt.md            # instructions for this step
│       └── summary.md           # what was accomplished
├── trajectories/
│   └── {task_type}/
│       └── run_NNN.json         # individual trajectory records
└── sessions/
    └── {session-id}.jsonl       # Claude Code conversation snapshots
```

## Predecessor project

Code was adapted from `avoid-compaction` at `/Users/mike/github/softwarewrighter/avoid-compaction`.
Key reference files if you need to check the original patterns:
- `src/main.rs` — CLI with clap derive, dispatch, exit codes, AI agent instructions in --help
- `src/commands/next.rs` — checklist-style output for fresh agent sessions
- `src/commands/complete.rs` — step completion, session snapshot, next step creation
- `tests/saga_tests.rs` — 48+ integration tests with tempdir pattern

## Phase 0 — Walking skeleton (FIRST PRIORITY)

Goal: a working CLI that can create, advance, and complete a saga.

### Step 1: Write integration tests first

Create `crates/agentrail-store/tests/saga_tests.rs` with tests for:
- `init_creates_saga_directory_and_files`
- `init_sets_correct_defaults`
- `init_fails_if_saga_already_exists`
- `load_saga_fails_on_empty_dir`
- `save_and_load_saga_roundtrips`
- `create_step_with_role`
- `transition_step_valid_transitions`
- `transition_step_invalid_transitions`
- `list_steps_sorted_by_number`

Create `crates/agentrail-store/tests/trajectory_tests.rs` with tests for:
- `save_and_load_trajectory`
- `retrieve_successes_filters_by_reward`
- `retrieve_successes_respects_limit`
- `next_run_number_increments`

### Step 2: Wire up CLI commands

In `crates/agentrail-cli/src/main.rs`, implement using clap derive:
- `agentrail init --name <name> --plan <file|text|->`
- `agentrail status`
- `agentrail next`
- `agentrail begin`
- `agentrail complete --summary <text> [--next-slug <slug> --next-prompt <text>] [--next-context <files>] [--next-role <role>] [--planned <"slug: desc">...] [--done]`
- `agentrail plan [--update <file|text|->]`
- `agentrail history`
- `agentrail abort [--reason <text>]`

The `next` command should output the same checklist-style format as avoid-compaction
(plan, step list, current prompt, context files, "when done" instructions) but
reference `agentrail` instead of `avoid-compaction`.

New vs avoid-compaction:
- `complete --next-role <meta|production|deterministic|validation>` to set role of next step
- `next` should display step role in output
- `init` creates `trajectories/` and `sessions/` dirs

### Step 3: Add CLI integration tests

Create `crates/agentrail-cli/tests/cli_tests.rs` that exercises the binary
via `std::process::Command` or by calling dispatch functions directly.

### Step 4: Pre-commit quality gates

All of these must pass before any commit:
```
cargo test --workspace
cargo clippy --workspace -- -D warnings
cargo fmt --check
```

## Phase 1 — Deterministic step execution

After Phase 0 is done-done:
- Define `GenerateTts` job kind in agentrail-exec
- Parameter struct with typed fields (script_path, output_wav, service_url, etc.)
- Executor that builds and runs the command
- Validator in agentrail-validate (file exists, duration > 0)
- Wire `agentrail run-next` to auto-execute deterministic steps
- Record trajectory on success/failure

## Phase 2 — Meta handoff packets

- Packet schema (already defined in core as HandoffPacket)
- `agentrail prepare-packet` command
- Packet save/load in step directory
- Prompt rendering from packet for production agent consumption

## Phase 3 — Trajectory retrieval and ICRL

- Before each step, retrieve top N successful trajectories for the task type
- Inject into the prompt/packet
- Policy distillation: summarize multiple trajectories into policy hints
- `agentrail trajectories <task_type>` command to inspect history

## Phase 4 — Hybrid orchestrator loop

- Meta → Production → Deterministic → Validation cycle
- Auto-advance through deterministic steps
- Escalate to agent only on failure or for semantic work
- Resume after interruption

## Cargo.lock decision

Currently in `.gitignore`. For a binary project, it's conventional to track it.
Remove from `.gitignore` and commit it when ready.

## GitHub remote

The repo is at `https://github.com/softwarewrighter/agentrail-rs`.
Origin is already configured. The bootstrapping session created the scaffolding
but has NOT pushed it yet — the only thing on remote is an empty README.md.

You should commit and push the scaffolding as the first real commit.
