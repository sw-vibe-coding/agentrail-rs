# User Guide

This guide covers how to use AgentRail to manage AI agent workflows,
from initial setup through dogfooding on real projects.

## Installation

```bash
# Build from source (release mode)
cargo build --release

# Install via sw-install
sw-install -p /path/to/agentrail-rs

# Verify
agentrail --version
```

## Core Concepts

**Saga**: a persistent, append-only record of a multi-step project.
Lives in `.agentrail/` inside your project directory.

**Step**: a discrete unit of work within a saga. Steps have roles
(Production, Deterministic, Validation, Meta, Legacy), statuses
(Pending, InProgress, Completed, Blocked), and optional task types.

**Skill**: a strategic workflow document for a task category. Describes
procedure, success patterns, and known failure modes. Loaded from
`.agentrail/skills/` or from a domain repo.

**Trajectory**: a per-run record (state, action, result, reward) stored
per task type. Injected into agent prompts by `agentrail next` so
agents learn from past successes.

**Domain repo**: a separate repository containing skills, executors,
and validators for a specific domain (e.g., coding, media production).

## Setting Up a New Project

### 1. Initialize the saga

```bash
cd ~/my-project
agentrail init --name my-project --plan "Build feature X. Phase 1: scaffold. Phase 2: implement. Phase 3: test."
```

This creates `.agentrail/` with `saga.toml`, `plan.md`, and empty
`steps/`, `trajectories/`, `sessions/` directories.

### 2. Register a domain (optional)

If you have a domain repo with skills, create `.agentrail/domains.toml`:

```toml
[[domain]]
name = "coding"
path = "/path/to/agentrail-domain-coding"
```

### 3. Create the first step

```bash
agentrail complete --summary "Project initialized" \
  --next-slug scaffold \
  --next-prompt "Set up the project structure, create initial files" \
  --next-role production \
  --next-task-type rust-project-init
```

### 4. Plan ahead with --planned

```bash
agentrail complete --summary "Initialized" \
  --next-slug scaffold \
  --next-prompt "Set up project structure" \
  --planned "implement: Build the core logic" \
  --planned "test: Write integration tests" \
  --planned "docs: Write documentation"
```

This creates step 1 (scaffold) plus placeholder steps 2-4. When step 1
completes, `agentrail complete` automatically advances to the existing
step 2 instead of creating a duplicate.

### 5. Add CLAUDE.md to your project

Create a `CLAUDE.md` in your project root with the AgentRail session
protocol. This tells Claude Code to follow the workflow automatically.

```markdown
## CRITICAL: AgentRail Session Protocol (MUST follow exactly)

### 1. START (do this FIRST, before anything else)
agentrail next

### 2. BEGIN (immediately after reading the next output)
agentrail begin

### 3. WORK (do what the step prompt says)
Do NOT ask "want me to proceed?". The step prompt IS your instruction.

### 4. COMMIT (after the work is done)
Commit your code changes with git.

### 5. COMPLETE (LAST thing, after committing)
agentrail complete --summary "what you accomplished" \
  --reward 1 --actions "tools and approach used"

### 6. STOP (after complete, DO NOT continue working)
Do NOT make further code changes after complete.
Any changes after complete are untracked.
```

## Running a Session

### Starting Claude Code

```bash
cd ~/my-project
claude "go"
```

The positional argument `"go"` starts an interactive session with "go"
as the initial message. Claude reads CLAUDE.md, follows the protocol,
and executes the current step.

### What the agent does

1. Runs `agentrail next` -- sees plan, steps, prompt, skills, trajectories
2. Runs `agentrail begin` -- marks step as in-progress
3. Does the work described in the prompt
4. Commits code with git
5. Runs `agentrail complete` -- records trajectory, advances to next step
6. Stops (does not continue working)

### Multiple projects in parallel

Open separate terminals for each project:

```bash
# Terminal 1
cd ~/project-a && claude "go"

# Terminal 2
cd ~/project-b && claude "go"

# Terminal 3
cd ~/project-c && claude "go"
```

Each project has its own `.agentrail/` saga. They share the same domain
repo for skills.

## Recording Trajectories

### Successful completion

```bash
agentrail complete --summary "Implemented the parser" \
  --reward 1 \
  --actions "wrote recursive descent parser, used nom crate"
```

### Failed step

```bash
agentrail complete --summary "Parser failed on nested expressions" \
  --reward -1 \
  --actions "attempted recursive descent" \
  --failure-mode "stack_overflow"
```

### Trajectory defaults

- `--reward` defaults to `+1` when a step has a `task_type`
- `--actions` defaults to the summary text if not specified
- Trajectories are only recorded for steps that have a `task_type`

## Distilling Skills

After accumulating trajectories, distill them into a skill document:

```bash
agentrail distill tts
```

This analyzes all trajectories for the `tts` task type:
- Groups by success/failure
- Extracts common action patterns from successes
- Counts failure modes from failures
- Creates or updates `.agentrail/skills/tts.toml`

The next time `agentrail next` runs for a step with `task_type = "tts"`,
the distilled skill is injected into the prompt.

## After a Saga Completes

When all steps are done and the saga is marked complete, you have
several options for continued work:

### Option A: Start a new saga (recommended for multi-step work)

```bash
agentrail init --name "v2-features" --plan "Bug fixes and new features for v2"
agentrail complete --summary "Starting v2" \
  --next-slug fix-parser \
  --next-prompt "Fix the nested expression bug"
```

This gives you full trajectory tracking, skill injection, and structured
handoffs for the new work.

### Option B: Work directly (for quick one-off fixes)

For a single bug fix or small change, just open Claude Code normally
without the agentrail workflow:

```bash
cd ~/my-project
claude "fix the off-by-one error in parser.c line 42"
```

No saga overhead for trivial changes.

### Option C: Add steps with `agentrail add`

```bash
agentrail add --slug fix-bug-123 \
  --prompt "Fix the parser bug"
```

This adds a step and auto-advances if the current step is done. Works
on completed sagas too (reopens them). No need to manually edit
saga.toml.

### Option D: Maintenance mode (ongoing)

For open-ended work (bug fixes, features, improvements), use
maintenance mode. See `docs/maintenance-mode.md`.

```bash
# Just tell Claude what to do
claude "fix the login timeout bug in auth.rs"
```

The agent creates the step itself via `agentrail add`, does the work,
completes, and stops. No pre-planning needed.

## Adding Steps Ad-Hoc

The `agentrail add` command adds steps without going through `complete`:

```bash
# Add a single task
agentrail add --slug fix-css --prompt "Fix mobile layout breakpoint"

# Add with task type for skill injection
agentrail add --slug refactor-api \
  --prompt "Refactor API to async" \
  --task-type rust-clippy-fix

# Batch add, then process one at a time
agentrail add --slug task-1 --prompt "First thing"
agentrail add --slug task-2 --prompt "Second thing"
claude "go"   # does task-1
claude "go"   # does task-2
```

## Auto-Executing Deterministic Steps

For steps that don't need agent involvement (build scripts, file
transforms, validation checks), use `run-loop`:

```bash
agentrail run-loop
```

This walks through pending steps:
- **Deterministic** steps: executes the job spec via domain executor
- **Validation** steps: runs validators from the domain repo
- **Production/Meta** steps: pauses and tells you to run `agentrail next`

Trajectories are recorded automatically on success (+1) or failure (-1).

### Setting up deterministic steps

Steps need a `job_spec` in their `step.toml` and a registered domain
with a matching executor. See `docs/domain-repos.md` for the executor
interface.

## Command Reference

| Command | Purpose |
|---------|---------|
| `agentrail setup --name <n> --plan <p> [--domain <d>]` | Bootstrap: saga + CLAUDE.md + domain |
| `agentrail init --name <name> --plan <text>` | Create a new saga |
| `agentrail add --slug <s> --prompt <p> [--task-type <t>]` | Add a step (maintenance mode / ad-hoc) |
| `agentrail next` | Show current step with skills and trajectories |
| `agentrail begin` | Mark current step as in-progress |
| `agentrail complete --summary <text> [flags]` | Complete step, record trajectory, advance |
| `agentrail status` | Show saga state at a glance |
| `agentrail plan [--update <text>]` | View or update the saga plan |
| `agentrail history` | Show all step summaries |
| `agentrail distill <task_type>` | Generate skill doc from trajectories |
| `agentrail run-loop` | Auto-execute deterministic steps |
| `agentrail abort [--reason <text>]` | Mark current step as blocked |
| `agentrail insert --after <N> --slug <s> --prompt <p>` | Insert a new step at position N+1 |
| `agentrail reorder <N> --to <M>` | Move a pending/in-progress step to position M |
| `agentrail reopen <N>` | Reopen a completed or blocked step |
| `agentrail --version` | Show version and build info |

### Complete flags

| Flag | Purpose |
|------|---------|
| `--summary <text>` | What was accomplished |
| `--next-slug <slug>` | Slug for the next step |
| `--next-prompt <text>` | Instructions for the next step |
| `--next-role <role>` | Role: production, deterministic, validation, meta |
| `--next-task-type <type>` | Task type for skill/trajectory lookup |
| `--next-context <files>` | Comma-separated context file paths |
| `--planned <"slug: desc">` | Planned future step (repeatable) |
| `--reward <-1\|0\|1>` | Trajectory reward (default: +1) |
| `--actions <text>` | Actions taken (default: summary) |
| `--failure-mode <id>` | Failure mode identifier |
| `--done` | Mark saga as complete |

## Handling Surprises Mid-Saga

Sometimes an unplanned task appears — another agent reports a bug, a
regression turns up during testing, or the current step uncovers work
that has to happen first. Three commands let you adjust the saga
without abandoning it:

### `agentrail insert --after <N>`

Slot a new pending step in at position N+1. Every pending/in-progress
step with a higher number shifts up by one (both `step.toml` `number`
and the `NNN-slug` directory name are updated). The saga cursor follows
its step by identity, so if you are currently mid-saga you end up
pointing at the same step after the shift.

```bash
# Found a blocker before step 3. Insert a fix before it.
agentrail insert --after 2 --slug hotfix-crash \
  --prompt "Reproduce and fix the crash from issue #42"
```

**Completed steps never shift.** The operation refuses if any step in
the affected range is already `Completed`, because those steps anchor
git-tracked history.

### `agentrail reorder <N> --to <M>`

Renumber an existing pending/in-progress step to a new position.
Intervening steps shift by one in the opposite direction. Completed
steps in the swept range cause the move to be rejected.

```bash
# Decided step 5 should actually go earlier.
agentrail reorder 5 --to 3
```

### `agentrail reopen <N>`

Transition a completed or blocked step back to `in-progress`, clear
`completed_at`, and move the cursor to it. The step's `commits` array
is preserved so the git-history linkage from the original completion
stays intact — new work after reopen should be committed on top of
those commits.

```bash
# A bug was reported against the work from step 3.
agentrail reopen 3
# ... fix it, git commit ...
agentrail complete --summary "Followup fix for reported regression"
```

If the saga was already `Completed`, reopen flips it back to `Active`.

## Tips

- **Always use --planned** when creating the first step. Pre-planned
  steps prevent duplicates when the agent completes and advances.

- **Use task types** for any step that will recur across projects.
  This is what enables skill injection and trajectory learning.

- **Distill periodically** after accumulating 5+ trajectories for a
  task type. The distilled skill captures what works and what doesn't.

- **One step per session**. The agent should do one step, commit,
  complete, and stop. Start a new session for the next step.

- **Domain repos are shareable**. Create one domain repo per knowledge
  area and register it in multiple projects.

## Troubleshooting

### Agent doesn't run agentrail next

Make sure CLAUDE.md is in the project root with the protocol section
marked as CRITICAL/MUST. Claude Code reads CLAUDE.md at session start.

### Agent keeps working after complete

The `complete` command now prints a STOP message. Also ensure CLAUDE.md
has step 6 (STOP) in the protocol.

### Duplicate steps created

Fixed in the current version. `complete` now checks for existing planned
steps at the next number and advances to them instead of creating
duplicates.

### Skills not showing in next output

Check that:
1. The step has a `task_type` set
2. Either a local skill exists at `.agentrail/skills/{type}.toml`
   or a domain repo is registered in `.agentrail/domains.toml`
   with a matching skill file
