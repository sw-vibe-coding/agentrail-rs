## Agentrail metadata discipline

When using agentrail, changes to `.agentrail/` files are first-class project
changes — the same as application source code. They must be committed, and
should be pushed before you hand off.

Before running `agentrail complete`:

1. `git status` — confirm `.agentrail/` files are tracked, not just present.
   Untracked files in `.agentrail/` mean the next session will not see them.
2. Stage and commit source changes and `.agentrail/` metadata together. The
   commit recorded into the step's `commits` field comes from `HEAD` at the
   moment of `agentrail complete`, so the commit must happen *first*.
3. Run `agentrail complete` after the commit lands.
4. Push the branch (or note the unpushed branch state in your handoff).

This is the most common agentrail failure mode: agents do the work, commit
the source, and forget the `.agentrail/` files. The result is a saga whose
recorded history disagrees with git history, which `agentrail audit` will
flag — but only after the damage is done.

### What's hand-editable in `.agentrail/`

Three categories of files live here, and the rules differ:

- **Append-only saga state** — `saga.toml`, `step.toml`, `plan.md`, session
  JSONLs under `sessions/`, trajectory JSONs under `trajectories/`, and
  anything under `.agentrail-archive/`. **Never hand-edit or delete these.**
  They are written by agentrail commands and represent the durable saga
  record; hand-editing desyncs the saga from git history and from prior
  session memory. A direct `rm` on untracked files is unrecoverable.
- **Regenerated state** — `instruction-lock.toml`. Don't hand-edit; it is
  rewritten on every `agentrail instructions apply`, so any local changes
  are overwritten silently.
- **User config** — `instruction-profile.toml`. This is normal user-edited
  config, like `.gitconfig` or a `package.json`. Hand-edit it freely, or
  use `agentrail instructions profile *` subcommands to mutate via
  commands (which validate field values and surface typos).
