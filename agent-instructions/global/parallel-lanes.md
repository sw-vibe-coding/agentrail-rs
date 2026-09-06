## Working a branch in parallel with another agent

If another agent is working this same repo on its own branch at the same
time, you are both writing into `.agentrail/`. Step numbers are
per-branch and *will* collide — you will both have a `003-` — so your
step directories land on identical paths and the merge conflicts. The
**slug** is the only part that can keep the two lanes apart.

Pick a lane prefix naming your slice of the work (`rtx5060`, `rtx3060`,
`api`, `ui`) and namespace your saga with it:

```bash
agentrail rename prefix <lane> --dry-run   # preview, changes nothing
agentrail rename prefix <lane>             # apply
git add -A .agentrail/                     # -A: a dir move is a delete + an add
git commit -m "saga: rename into <lane> lane"
```

`001-setup` becomes `001-<lane>-setup`, and the saga name is prefixed
too. Your directories no longer collide with the other agent's, so both
lanes survive the merge.

**Run it retroactively — that is what it is for.** You do not need to
decide on a lane before starting work. Renaming is not renumbering: step
numbers, statuses, completion timestamps, and recorded commits are all
preserved, so steps you have already completed keep their git-history
linkage and `agentrail audit` still matches them. Unlike `insert` and
`reorder`, which refuse to touch completed steps, `rename` is safe on
work that has already landed.

It is also **idempotent** — steps already carrying the prefix are
skipped — so just re-run it whenever you have added steps.

**Archive your lane before the merge.** Slug prefixing separates the step
directories, but both branches still have a single `saga.toml` and
overlapping step numbers, so merging two *live* lanes still conflicts.
When your lane is done:

```bash
agentrail complete --summary "..." --done
agentrail archive --reason "<lane> lane merged upstream"
```

Because the saga name was prefixed too, the archive directories are
distinct and the merge is completely conflict-free, with both lanes'
histories landing side by side.

Finer-grained forms: `agentrail rename step <N> <new-slug>` renames one
step, `agentrail rename saga <new-name>` renames only the saga.
