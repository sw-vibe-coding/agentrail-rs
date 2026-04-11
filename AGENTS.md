# AGENTS.md

Instructions for AI coding agents working in this repository. See `CLAUDE.md`
for the full set of rules; this file surfaces the ones most likely to be
violated by an agent that has not read the rest.

## Handling `.agentrail/` (CRITICAL)

- **Always track `.agentrail/` in git.** Never add it to `.gitignore`.
- **Never `rm`, `mv`, or hand-edit files under `.agentrail/`.** Use
  `agentrail` subcommands only (`init`, `add`, `complete`, `archive`,
  `abort`, `plan`, `audit`). Direct deletion of untracked step files is
  unrecoverable.
- **Commit before `agentrail complete`.** `complete` records the current
  `HEAD` hash into the step's `commits` field. If you complete before
  committing, the linkage is empty and `agentrail audit` cannot match the
  step back to its commit.
- **If an agent (you or another) has deleted step files**: run
  `agentrail audit` to see what can be reconstructed. With
  `--emit-commands`, it prints a shell script of `agentrail add` lines
  seeded from commit subjects. Review and edit them before running.

## The session protocol

1. `agentrail next` — read the step prompt and context.
2. `agentrail begin` — mark in-progress.
3. Do the work.
4. `git add` + `git commit` — commit *before* `complete`.
5. `agentrail complete --summary "..." --reward ±1` — records HEAD and
   advances state.
6. **Stop.** Do not make further changes after `complete`.

## Repository layout

Rust workspace. `cargo test --workspace` runs everything. See `CLAUDE.md`
for crate layout and build commands.
