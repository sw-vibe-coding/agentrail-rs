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

Never edit or delete files under `.agentrail/` by hand. Always go through
`agentrail` commands. A direct `rm` on untracked step files is unrecoverable.
