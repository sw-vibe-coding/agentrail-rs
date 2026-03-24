# Getting Started

This guide walks you through setting up AgentRail on a project for the
first time, from installation through your first agent-driven session.

## Prerequisites

- Rust toolchain (for building agentrail)
- [Claude Code](https://claude.ai/code) (or another AI coding agent)
- [sw-install](https://github.com/softwarewrighter/sw-install) (optional,
  for installing to PATH)

## Step 1: Install AgentRail

```bash
# Clone the repo
git clone https://github.com/sw-vibe-coding/agentrail-rs
cd agentrail-rs

# Build release binary
cargo build --release

# Install to PATH (via sw-install)
sw-install -p .

# Verify
agentrail --version
```

## Step 2: Set Up Your Project

### Option A: Interactive Wizard (recommended)

The wizard walks you through project setup with prompts:

```bash
cd ~/my-project
bash /path/to/agentrail-rs/scripts/agentrail-wizard.sh
```

It asks for:
1. Project name
2. Plan (what needs to be done, multi-line)
3. Domain repo path (optional, for skill injection)
4. First step (slug, description, role, task type)
5. Planned future steps

Then it runs `agentrail setup` and creates the first step automatically.

### Option B: One Command

```bash
cd ~/my-project
agentrail setup --name my-project --plan "Build a web app with user auth"
```

This creates:
- `.agentrail/` directory with saga, steps, trajectories, sessions
- `CLAUDE.md` with the AgentRail session protocol (tells Claude how
  to use agentrail commands)

To also register a domain repo for skill injection:

```bash
agentrail setup --name my-project \
  --plan "Build a web app" \
  --domain /path/to/agentrail-domain-coding
```

### Option C: Step by Step

```bash
cd ~/my-project

# Initialize the saga
agentrail init --name my-project --plan "Build a web app with user auth"

# Create CLAUDE.md (copy the template from below)
# Register a domain repo (optional)
```

## Step 3: Create the First Step

After setup, create the first step that tells the agent what to do:

```bash
agentrail complete --summary "Project initialized" \
  --next-slug scaffold \
  --next-prompt "Set up the project structure: create directories, config files, and a hello-world main" \
  --next-role production \
  --next-task-type rust-project-init
```

To plan ahead (recommended):

```bash
agentrail complete --summary "Project initialized" \
  --next-slug scaffold \
  --next-prompt "Set up the project structure" \
  --next-role production \
  --planned "implement: Build the core logic" \
  --planned "test: Write tests" \
  --planned "docs: Write documentation"
```

Verify with:

```bash
agentrail next
```

You should see the plan, step list, current step prompt, and any
matching skill docs.

## Step 4: Start Claude Code

```bash
cd ~/my-project
claude "go"
```

The agent reads CLAUDE.md, follows the 6-step protocol:
1. `agentrail next` -- sees the plan, step, prompt, skills
2. `agentrail begin` -- marks step as in-progress
3. Does the work described in the prompt
4. Commits code with git
5. `agentrail complete` -- records trajectory, advances to next step
6. Stops

When the session ends, start a new one for the next step:

```bash
claude "go"
```

## Step 5: Repeat

Each `claude "go"` session handles one step. The agent picks up where
the previous one left off, seeing the full history and any accumulated
skill knowledge.

After several steps, you can distill trajectories into skill docs:

```bash
agentrail distill rust-project-init
```

## CLAUDE.md Template

If you used `agentrail setup`, this was created automatically. If not,
create `CLAUDE.md` in your project root with this content:

```markdown
# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when
working with code in this repository.

## CRITICAL: AgentRail Session Protocol (MUST follow exactly)

This project uses AgentRail. Every session follows this exact sequence:

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

Add project-specific sections below the protocol (build commands,
related projects, available task types, etc.).

## Using a Domain Repo

Domain repos provide reusable skills across projects. The
[agentrail-domain-coding](https://github.com/sw-vibe-coding/agentrail-domain-coding)
repo has skills for Rust, C, Lisp, and web development.

### Register in your project

```bash
# Clone the domain repo
git clone https://github.com/sw-vibe-coding/agentrail-domain-coding

# Register it (creates .agentrail/domains.toml)
agentrail setup --name my-project --plan "..." \
  --domain /path/to/agentrail-domain-coding
```

Or manually create `.agentrail/domains.toml`:

```toml
[[domain]]
name = "coding"
path = "/path/to/agentrail-domain-coding"
```

### How it works

When a step has a `task_type` (e.g., `rust-project-init`), `agentrail
next` looks for a matching skill doc:
1. First in `.agentrail/skills/` (local, project-specific)
2. Then in registered domain repos

The skill procedure, success patterns, and failure modes are injected
into the agent's prompt automatically.

## Working on Existing Projects

For an existing project that doesn't have a saga yet:

```bash
cd ~/existing-project

# The wizard works on existing projects too
bash /path/to/agentrail-rs/scripts/agentrail-wizard.sh

# Or one command
agentrail setup --name existing-project \
  --plan "Add feature X to the existing codebase"
```

AgentRail adds `.agentrail/` and `CLAUDE.md` without touching your
existing code.

## After a Saga Completes

When all steps are done:

- **More multi-step work**: start a new saga with `agentrail init`
- **Quick one-off fix**: just use `claude` normally without agentrail
- **Reopen**: edit `.agentrail/saga.toml`, set `status = "active"`,
  then create new steps with `agentrail complete`

See `docs/user-guide.md` for details.

## Example: Real Project Setup

Here is how the [tml24c](https://github.com/sw-vibe-coding/tml24c)
project (Tiny Macro Lisp for COR24) was bootstrapped:

```bash
cd ~/github/sw-vibe-coding/tml24c

# Setup with coding domain
agentrail setup --name tml24c \
  --plan "Build a Tiny Macro Lisp that compiles to COR24 assembly" \
  --domain ~/github/sw-vibe-coding/agentrail-domain-coding

# First step: research and planning
agentrail complete --summary "Project initialized" \
  --next-slug research-and-plan \
  --next-prompt "Read docs/research.txt, study COR24 toolchain, create architecture and design docs" \
  --next-role production \
  --planned "scaffold: Set up C source structure" \
  --planned "lexer: Implement tokenizer" \
  --planned "reader: Implement S-expression reader" \
  --planned "eval-core: Implement eval with core special forms" \
  --planned "gc: Implement mark-sweep GC" \
  --planned "compiler: Compile to COR24 assembly"

# Start working
claude "go"
```

## Troubleshooting

**Agent doesn't follow the protocol**: make sure CLAUDE.md is in the
project root and the protocol section says CRITICAL/MUST at the top.

**Agent asks "want me to proceed?"**: the CLAUDE.md should say "Do NOT
ask. The step prompt IS your instruction."

**Agent keeps working after complete**: the `complete` command prints
a STOP message. Ensure CLAUDE.md has step 6 (STOP).

**Skills not appearing**: check that the step has `--next-task-type`
set and that the domain repo is registered in `.agentrail/domains.toml`.

**Duplicate steps**: fixed in current version. `complete` advances to
existing planned steps instead of creating duplicates.
