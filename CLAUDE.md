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

## Two-Layer Architecture

See `docs/architecture.md` for full details.

**Layer 1 (this repo)**: Generic inference-time learning engine. Task-agnostic
orchestration, dual-memory (skills + experiences), ICRL trajectory injection,
distillation. Does NOT know about specific domains (TTS, ffmpeg, etc.).

**Layer 2 (separate repos)**: Domain-specific knowledge. Skill documents,
curated experiences, executor implementations, validators. See
`docs/domain-repos.md` for the domain repo contract.

## Crate Layout

Cargo workspace (`edition = "2024"`) with five crates under `crates/`:

- **agentrail-core** -- Domain types and error enum. All other crates depend on this. Key types: `SagaConfig`, `StepConfig`, `StepRole`, `Trajectory`, `HandoffPacket`, `JobSpec`. Error type: `agentrail_core::error::Error` with `Result<T>` alias.
- **agentrail-store** -- File-based persistence against `.agentrail/`. Modules: `saga` (init/load/save), `step` (create/transition/list with NNN-slug dirs), `trajectory` (ICRL record save/retrieve), `session` (Claude Code JSONL snapshot).
- **agentrail-cli** -- Binary crate (`agentrail`). 8 commands: init, status, next, begin, complete, plan, history, abort. Has `lib.rs` exporting `commands` module for testability.
- **agentrail-exec** -- Deterministic step executors (stub; will become trait + shell executor routing to domain repos).
- **agentrail-validate** -- Output validators (stub; will become trait + shell validator routing to domain repos).

Dependency flow: `cli -> store, exec, validate -> core`

## Key Concepts

- **Dual memory (XSkill pattern)**: skills (strategic workflow docs per task category) + experiences (tactical per-run records). See `docs/dual-memory.md`.
- **Step roles** (Meta, Production, Deterministic, Validation): orchestration loop. Meta prepares handoffs, production does semantic work, deterministic runs without agents, validation checks outputs.
- **ICRL injection**: `agentrail next` retrieves successful experiences for the step's task_type and injects them into the prompt output.
- **Step transitions** enforce: Pending -> InProgress -> Completed|Blocked.
- **Domain repos**: per-domain knowledge (skills, experiences, executors, validators) in separate repos. See `docs/domain-repos.md`.

## Storage Layout

All runtime data in `.agentrail/` (never `.avoid-compaction/`):
```
.agentrail/saga.toml
.agentrail/plan.md
.agentrail/steps/NNN-slug/{step.toml, prompt.md, summary.md}
.agentrail/skills/{task_type}.toml          (planned)
.agentrail/experiences/{task_type}/run_NNN.json  (planned)
.agentrail/trajectories/{task_type}/run_NNN.json (existing, backward compat)
.agentrail/domains.toml                     (planned)
.agentrail/sessions/{session-id}.jsonl
```

## Key Documentation

- `docs/architecture.md` -- Two-layer design, layer interaction, domain discovery
- `docs/dual-memory.md` -- Skills and experiences schemas, retrieval strategy, distillation
- `docs/domain-repos.md` -- Domain repo structure, executor/validator interfaces
- `docs/research-foundations.md` -- Research papers and how they map to the architecture
- `docs/implementation-plan.md` -- Phased roadmap (replaces continue-from-bootstrap.md phases)
- `docs/continue-from-bootstrap.md` -- Original bootstrapping handoff (historical)

## Development Practices

- TDD: write failing test first, implement minimum logic, refactor.
- Test pattern: integration tests using `tempfile::tempdir()`.
- Evolved from [avoid-compaction](https://github.com/softwarewrighter/avoid-compaction).
- Layer 1 must remain task-agnostic. No domain-specific imports in agentrail-rs.
