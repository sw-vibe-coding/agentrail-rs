# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
cargo test --workspace                    # run all tests
cargo test -p agentrail-store             # run tests for one crate
cargo test -p agentrail-store saga        # run tests matching "saga" in one crate
cargo clippy --workspace -- -D warnings   # lint (treats warnings as errors)
cargo fmt --check                         # format check
cargo fmt                                 # auto-format
```

Pre-commit gate (all must pass): `cargo test && cargo clippy -- -D warnings && cargo fmt --check`

## Architecture

Cargo workspace (`edition = "2024"`) with five crates under `crates/`:

- **agentrail-core** — Domain types and error enum. All other crates depend on this. Key types: `SagaConfig`, `StepConfig`, `StepRole`, `Trajectory`, `HandoffPacket`, `JobSpec`. Error type: `agentrail_core::error::Error` with `Result<T>` alias.
- **agentrail-store** — File-based persistence against the `.agentrail/` directory. Modules: `saga` (init/load/save), `step` (create/transition/list with NNN-slug dirs), `trajectory` (ICRL record save/retrieve), `session` (Claude Code JSONL snapshot).
- **agentrail-cli** — Binary crate (`agentrail`). Currently a stub; planned commands in `docs/continue-from-bootstrap.md`.
- **agentrail-exec** — Deterministic step executors (stub, not yet implemented).
- **agentrail-validate** — Output validators and acceptance contracts (stub, not yet implemented).

Dependency flow: `cli → store, exec, validate → core`

## Key Concepts

- **Step roles** (Meta → Production → Deterministic → Validation): orchestration loop where meta agents prepare handoff packets, production agents do semantic work, deterministic steps run without agents, validation steps check outputs.
- **ICRL trajectories**: state/action/reward records stored per task type at `.agentrail/trajectories/{task_type}/run_NNN.json`. Used to retrieve past successes for in-context reinforcement learning.
- **Handoff packets**: structured briefings (`HandoffPacket`) from meta agent to production agent with objective, procedure, success patterns, and output contracts.
- **Step transitions** enforce: Pending → InProgress → Completed|Blocked.

## Storage Layout

All runtime data in `.agentrail/` (never `.avoid-compaction/`):
```
.agentrail/saga.toml
.agentrail/plan.md
.agentrail/steps/NNN-slug/{step.toml, prompt.md, summary.md}
.agentrail/trajectories/{task_type}/run_NNN.json
.agentrail/sessions/{session-id}.jsonl
```

## Development Practices

- TDD: write failing test first, implement minimum logic, refactor.
- Test pattern: integration tests using `tempfile::tempdir()` (see `docs/continue-from-bootstrap.md` for planned test list).
- Evolved from [avoid-compaction](https://github.com/softwarewrighter/avoid-compaction) — reference its `tests/saga_tests.rs` for integration test patterns.
