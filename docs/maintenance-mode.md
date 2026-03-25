# Maintenance Mode

Maintenance mode is an open-ended saga for ongoing bug fixes, feature
requests, and ad-hoc tasks. Unlike a normal saga with a fixed set of
planned steps, a maintenance saga runs indefinitely. Each session
processes one task, then the user decides what's next.

## Setup

```bash
cd ~/my-project
agentrail setup --name "my-project-maint" \
  --plan "Ongoing maintenance: bug fixes, features, improvements"
```

Then replace the generated CLAUDE.md with the maintenance-mode template
below, or add a maintenance section to your existing CLAUDE.md.

## How It Works

1. User adds tasks via `agentrail add`:
   ```bash
   agentrail add --slug fix-parser-bug \
     --prompt "Fix the off-by-one error in parser.c line 42"
   agentrail add --slug add-dark-mode \
     --prompt "Add dark mode toggle to the settings page"
   ```

2. User starts Claude:
   ```bash
   claude "go"
   ```

3. Agent runs `agentrail next`, sees the next pending task, does it,
   runs `agentrail complete`, stops.

4. If no pending tasks, agent asks the user what to do.

5. User adds more tasks and runs `claude "go"` again. Repeat.

## Adding Tasks

### From the command line (before starting Claude)

```bash
# Simple task
agentrail add --slug fix-login \
  --prompt "Fix the login timeout issue reported in issue #42"

# With task type for skill injection
agentrail add --slug refactor-api \
  --prompt "Refactor the API module to use async/await" \
  --task-type rust-clippy-fix

# Multiple tasks at once
agentrail add --slug fix-css --prompt "Fix mobile layout breakpoint"
agentrail add --slug add-tests --prompt "Add tests for user service"
agentrail add --slug update-deps --prompt "Update all dependencies"
```

### From inside a Claude session

The agent can also add tasks during a session if it discovers more
work while doing the current task:

```bash
agentrail add --slug fix-related-bug \
  --prompt "Found related bug in auth module while fixing login"
```

This adds it to the backlog without disrupting the current step.

## The `agentrail add` Command

```
agentrail add --slug <slug> --prompt <text> [--role <role>] [--task-type <type>]
```

- Adds a step at the end of the current step list
- If the saga's current step is completed/blocked, auto-advances to
  the new step
- If the saga is marked complete, reopens it as active
- Default role is `production`

## CLAUDE.md Template for Maintenance Mode

```markdown
# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when
working with code in this repository.

## CRITICAL: AgentRail Maintenance Protocol (MUST follow exactly)

This project uses AgentRail in maintenance mode (ongoing tasks).

### 1. START (do this FIRST)
```bash
agentrail next
```

If there is a pending step: proceed to step 2.
If there are no pending steps: ask the user what to work on.

### 2. BEGIN
```bash
agentrail begin
```

### 3. WORK
Do the task described in the step prompt. Do NOT ask permission.

### 4. COMMIT
Commit code changes with git.

### 5. COMPLETE
```bash
agentrail complete --summary "what you accomplished" --reward 1
```

### 6. STOP
Do NOT continue working after complete. The user will add more
tasks and start a new session when ready.
```

## Example Workflow

```bash
# Monday: user adds tasks from issue tracker
agentrail add --slug fix-123 --prompt "Fix login timeout (issue #123)"
agentrail add --slug fix-124 --prompt "Fix mobile layout (issue #124)"
agentrail add --slug feat-125 --prompt "Add export CSV button (issue #125)"

# Process them one at a time
claude "go"   # does fix-123, stops
claude "go"   # does fix-124, stops
claude "go"   # does feat-125, stops

# Wednesday: more tasks
agentrail add --slug fix-130 --prompt "Fix email notifications"
claude "go"   # does fix-130, stops
```

## Transitioning Between Modes

### From roadmap saga to maintenance
When a planned saga completes (agent runs `--done`), start a new
maintenance saga:

```bash
agentrail init --name "my-project-maint" \
  --plan "Ongoing maintenance"
```

### From maintenance to roadmap
If a big feature needs multi-step planning, start a new saga:

```bash
agentrail init --name "v2-rewrite" \
  --plan "Rewrite the backend. Phase 1: ..."
```

The `.agentrail/` directory keeps history from all sagas (trajectories
and skills persist across saga reinitializations).

## Tips

- **Keep tasks small**: one task = one session = one commit. Break
  large tasks into multiple `agentrail add` calls.
- **Use task types**: even in maintenance mode, setting `--task-type`
  enables skill injection and trajectory learning.
- **Batch add**: add several tasks at once, then process them with
  sequential `claude "go"` sessions.
- **Trajectories accumulate**: every completed maintenance task adds
  to the trajectory history. Run `agentrail distill` periodically
  to update skills.
