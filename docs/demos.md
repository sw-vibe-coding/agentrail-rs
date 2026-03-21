# Demos

This document describes the demo scripts, skill files, and VHS tape
recording that demonstrate agentrail's skill-guided agent prompting.

## Quick Start

```bash
# Install agentrail
cargo install --path crates/agentrail-cli

# Run a demo
bash demo/scripts/01-saga-workflow.sh
bash demo/scripts/02-skill-injection.sh
bash demo/scripts/03-rust-project-with-skills.sh

# Record the VHS tape (requires charmbracelet/vhs)
vhs demo/tapes/demo.tape
```

## Demo Scripts

### 01-saga-workflow.sh

Full saga lifecycle without skills. Shows:
- `agentrail init` to create a saga with a plan
- `agentrail next` to see current state
- `agentrail complete` with `--next-slug`, `--next-prompt`, `--planned`
- `agentrail begin` to mark step in-progress
- `agentrail history` to see completed steps
- `agentrail status` showing the completed saga

**What it demonstrates**: the basic saga state machine and how steps
progress through the workflow.

### 02-skill-injection.sh

Skill and trajectory injection into `agentrail next`. Shows:
- Loading skill TOML files into `.agentrail/skills/`
- Pre-populating trajectory records
- Creating a step with `--next-task-type rust-project-init`
- `agentrail next` outputting the full skill procedure, known failure
  modes, and past successful trajectories
- Switching to a `clippy-fix` task type with a different skill

**What it demonstrates**: the XSkill dual-memory pattern in action --
agents see both strategic workflow docs (skills) and tactical past
successes (trajectories) injected into their prompt automatically.

### 03-rust-project-with-skills.sh

End-to-end: actually creates a Rust project guided by skills. Shows:
- Skill-guided project creation with edition 2024
- Intentional clippy warning (derivable_impls) introduced
- Failed trajectory recorded with reward -1
- Transition to clippy-fix step with matching skill
- Clippy warning fixed following the skill procedure (derive Default,
  not #[allow])
- Successful trajectory recorded with reward +1
- Final verification: clippy clean, fmt clean, program runs

**What it demonstrates**: the full loop -- skill guides the agent,
agent hits a warning, records failure, gets a new skill for fixing it,
applies the correct fix, records success.

## Skill Files

### rust-project-init.toml

Guides creation of a Rust project with edition 2024 and quality gates.

Key content:
- 6-step procedure (cargo init, set edition, verify, build, clippy, fmt)
- Success patterns: always set 2024, run all three gates
- Failure modes: wrong_edition (5x), skip_clippy (2x)
- Output contract: Cargo.toml and src/main.rs must exist

### clippy-fix.toml

Guides fixing clippy warnings without bypassing them.

Key content:
- 7-step procedure (identify, fix, NEVER allow, NEVER disable, re-run, test)
- Success patterns: specific fix recipes (derivable_impls, collapsible_if, etc.)
- Failure modes: allow_attribute (8x), wrong_fix (2x), incomplete_fix (3x)

### saga-workflow.toml

Guides using the agentrail saga workflow itself (meta-skill).

Key content:
- 7-step procedure for the init/next/begin/complete loop
- Success patterns: always run next first, write clear summaries
- Failure modes: skip_next (4x), vague_summary (3x)

## VHS Tape

### demo.tape

Produces `demo/tapes/demo.gif` showing:
1. `agentrail next` with full skill + trajectory injection
2. `agentrail begin` + `agentrail complete --done`
3. `agentrail status` showing completed saga

Uses `demo/scripts/tape-setup.sh` to create a pre-populated demo
environment in a temp directory.

**To record**: `vhs demo/tapes/demo.tape`

**Output**: `demo/tapes/demo.gif` (277KB)

## File Paths

```
demo/
  scripts/
    01-saga-workflow.sh           Full saga lifecycle demo
    02-skill-injection.sh         Skill + trajectory injection demo
    03-rust-project-with-skills.sh  End-to-end Rust project creation
    tape-setup.sh                 Setup script for VHS tape recording
  skills/
    rust-project-init.toml        Skill: create Rust project with 2024 edition
    clippy-fix.toml               Skill: fix clippy warnings (never bypass)
    saga-workflow.toml            Skill: agentrail saga workflow
  tapes/
    demo.tape                     VHS tape definition
    demo.gif                      Recorded demo (generated)
```

## Adding New Demo Skills

To create a new skill demo:

1. Write a skill TOML in `demo/skills/{task_type}.toml`
2. Write a demo script in `demo/scripts/` that:
   - Creates a temp dir
   - Inits a saga
   - Copies the skill into `.agentrail/skills/`
   - Creates a step with `--next-task-type {task_type}`
   - Runs `agentrail next` to show the injection
   - Performs the actual task guided by the skill
   - Records success/failure trajectory
3. Run the script to verify it works
4. Update this document
