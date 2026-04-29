# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Build & Test Commands

```bash
cargo test --workspace                                 # run all tests
cargo test -p agentrail-store                          # run tests for one crate
cargo test -p agentrail-store saga                     # run tests matching "saga" in one crate
cargo clippy --workspace --all-targets -- -D warnings  # lint (treats warnings as errors)
cargo fmt --check                                      # format check
cargo fmt                                              # auto-format
```

Pre-commit gate (all must pass): `cargo test --workspace && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --check`

`--all-targets` is required — without it, clippy skips test and example
targets, so test-only lints (e.g., `clippy::cloned_ref_to_slice_refs`)
will slip through local gates and fail later in CI or on the next run.

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
- **agentrail-store** -- File-based persistence against `.agentrail/`. Modules: `saga` (init/load/save), `step` (create/transition/list with NNN-slug dirs), `trajectory` (ICRL record save/retrieve), `session` (Claude Code JSONL snapshot), `instructions` (briefing renderer + marker apply for shared agent rules).
- **agentrail-cli** -- Binary crate (`agentrail`). Commands include init, status, next, begin, complete, plan, history, abort, insert, reorder, reopen, audit, snapshot, archive, gen-agents-doc, and `instructions` (status/apply/diff/show/list). Has `lib.rs` exporting `commands` module for testability.
- **agentrail-exec** -- Deterministic step executors (stub; will become trait + shell executor routing to domain repos).
- **agentrail-validate** -- Output validators (stub; will become trait + shell validator routing to domain repos).

Dependency flow: `cli -> store, exec, validate -> core`

## Key Concepts

- **Dual memory (XSkill pattern)**: skills (strategic workflow docs per task category) + experiences (tactical per-run records). See `docs/dual-memory.md`.
- **Step roles** (Meta, Production, Deterministic, Validation): orchestration loop. Meta prepares handoffs, production does semantic work, deterministic runs without agents, validation checks outputs.
- **ICRL injection**: `agentrail next` retrieves successful experiences for the step's task_type and injects them into the prompt output.
- **Step transitions** enforce: Pending -> InProgress -> Completed|Blocked,
  plus Completed|Blocked -> InProgress (reopen/unblock). Reopening clears
  `completed_at` but preserves the step's recorded `commits` so the
  git-history linkage survives.
- **Mid-saga editing**: `agentrail insert --after N`, `agentrail reorder N --to M`,
  and `agentrail reopen N` let an agent adjust the saga when a surprise
  lands. All three refuse to touch completed steps: completed steps
  never renumber, so git-tracked history stays stable. `insert` and
  `reorder` apply **cursor preemption** — if the new/moved step lands
  at or ahead of the current cursor, focus follows it so `agentrail next`
  surfaces the blocker before the preempted step. Steps placed behind
  the cursor are queued without disturbing focus.
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
.agentrail/instruction-profile.toml         (optional, briefing config)
.agentrail/instruction-lock.toml            (briefing apply record)
```

## Briefing / clearinghouse (`agentrail instructions`)

A central place for shared agent rules. The `agent-instructions/` directory at
the workspace root holds the canonical fragments; they are bundled into the
binary at compile time via `include_str!`. Each project's `CLAUDE.md` /
`AGENTS.md` carries a markered region that the binary regenerates
idempotently: an HTML comment of the form `agentrail:global:start ...` opens
the block and `agentrail:global:end` closes it (see
`MARKER_START_PREFIX` / `MARKER_END` in
`crates/agentrail-store/src/instructions.rs` for the exact prefixes — they
are not reproduced literally here so this documentation does not get
treated as a real briefing block by `find_block`).

Anything outside the markers is preserved verbatim, so repo-local rules and
the global briefing coexist in the same file.

Commands:
- `agentrail instructions status` — exit 0 if up to date, 1 if any target is
  stale, missing, or has no briefing block.
- `agentrail instructions apply` — render the embedded block into the resolved
  targets and write `.agentrail/instruction-lock.toml`. Idempotent.
- `agentrail instructions diff` — line-level diff between embedded and current.
- `agentrail instructions show` — print the rendered default profile body.
- `agentrail instructions list` — list embedded profiles and fragments.

Update flow: edit `agent-instructions/<...>.md` → commit → rebuild
(`sw-install`) → in each project, `agentrail instructions apply` →
commit the regenerated block. The lock file's `content_hash` is a stable
FNV-1a hash of the rendered body, so drift is detectable without a network
round-trip.

Per-repo override (optional, `.agentrail/instruction-profile.toml`):
```toml
profile = "default"
targets = ["CLAUDE.md", "AGENTS.md"]   # default: any of these that exist
```

## Key Documentation

- `docs/architecture.md` -- Two-layer design, layer interaction, domain discovery
- `docs/dual-memory.md` -- Skills and experiences schemas, retrieval strategy, distillation
- `docs/domain-repos.md` -- Domain repo structure, executor/validator interfaces
- `docs/research-foundations.md` -- Research papers and how they map to the architecture
- `docs/implementation-plan.md` -- Phased roadmap (replaces continue-from-bootstrap.md phases)
- `docs/continue-from-bootstrap.md` -- Original bootstrapping handoff (historical)

## Handling `.agentrail/` in git (CRITICAL)

The `.agentrail/` directory is the durable record of saga/step history. Treat
it like source code:

- **Always track `.agentrail/` in git.** Never add it to `.gitignore`. If you
  inherit a repo that has it ignored, that is a bug — unignore it.
- **Commit step artifacts as each step completes.** `agentrail complete`
  records the current `HEAD` hash into the step's `commits` field, so the
  commit must happen *before* `agentrail complete` for the linkage to be
  accurate. Order: work -> `git add` + `git commit` -> `agentrail complete`.
- **Never edit or delete files under `.agentrail/` by hand.** Always go
  through `agentrail` commands (`init`, `add`, `complete`, `archive`, etc.).
  Direct `rm`/`rm -rf` on untracked step files is unrecoverable — git reflog
  cannot restore what was never added.
- If you accidentally end up with gaps, `agentrail audit` compares git history
  against saga history and emits a shell script of `agentrail add` lines to
  reconstruct the missing steps. `agentrail audit --emit-commands` for the
  script; review and edit before running.

## Development Practices

- TDD: write failing test first, implement minimum logic, refactor.
- Test pattern: integration tests using `tempfile::tempdir()`.
- Evolved from [avoid-compaction](https://github.com/softwarewrighter/avoid-compaction).
- Layer 1 must remain task-agnostic. No domain-specific imports in agentrail-rs.
