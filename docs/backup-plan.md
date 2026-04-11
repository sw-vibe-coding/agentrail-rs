# Backup & restore plan for `.agentrail/`

**Status**: design only. Not implemented. Opened this file to stake the
design before the next incident so there is something to point at.

## Problem

`.agentrail/` should always be tracked in git, and `agentrail audit` can
reconstruct sagas from commit history when that discipline is followed. But
agents sometimes delete untracked files outright, and if `.agentrail/` is
not yet staged when that happens, git reflog cannot recover them — the
blobs never entered the object store. The user has hit this at least once
and lost saga history across multiple repos.

Three layers of defense, in increasing order of cost and independence:

1. **Discipline**: `.agentrail/` is tracked + committed as steps complete.
   Enforced by `CLAUDE.md` / `AGENTS.md`. Already in place.
2. **`agentrail audit`**: detect gaps against git history and emit
   `agentrail add` commands to rebuild. Already in place; limited to what
   git history preserves.
3. **Out-of-tree mirror**: periodic snapshot of `.agentrail/` to a location
   independent of the repo, so that even a `rm -rf .agentrail/` on an
   untracked state is recoverable. **This doc describes layer 3.**

## Proposed design

### Storage location

```
~/.config/agentrail/repos/<repo-fingerprint>/snapshots/<timestamp>/
```

- `<repo-fingerprint>` is a stable hash derived from:
  - The repo's first `git remote get-url origin` output, if any.
  - Otherwise the absolute path of the repo root.
  - Fingerprint = first 12 chars of SHA-256 of the chosen string. Also
    store a `meta.toml` file alongside the snapshots with the
    fingerprint's provenance (origin URL, last-seen path, last-seen host)
    so the user can identify the repo later.
- `<timestamp>` is `YYYYMMDDTHHMMSS` local time, matching
  `agentrail_core::timestamp()`.
- Each snapshot is a full recursive copy of `.agentrail/` — not a diff.
  Optimizations (hardlink dedup, zstd tar) are a follow-up; start simple.

### When snapshots are taken

A new `agentrail backup` subcommand, invoked as a side effect of every
mutating command:

- `init`, `add`, `begin`, `complete`, `abort`, `archive`, `plan --update`.
- Read-only commands (`status`, `next`, `history`, `audit`) do nothing.
- Snapshot runs *after* the primary mutation has succeeded, so a corrupt
  state is not captured.
- Snapshots are best-effort: a failed copy prints a warning but does not
  abort the primary command.

Alternative trigger surface, for power users: a filesystem watcher or
a git `post-commit` hook. Out of scope for v1 — the in-process hook on
mutating commands is simple and deterministic.

### Pruning

The mirror is not a museum. Default policy:

- Keep all snapshots taken in the last 24 hours.
- Keep one snapshot per hour for the last 7 days.
- Keep one snapshot per day for the last 30 days.
- Prune everything older.

Pruning runs at the same moment a snapshot is written, so the mirror
steady-state size is bounded by the number of retained snapshots, not by
how long the repo has existed. A `--no-prune` flag on `agentrail backup`
(or a config toggle) lets the user disable pruning for a specific repo.

### Restore UX

```
agentrail backup list              # show all snapshots for the current repo
agentrail backup restore <ts>      # restore .agentrail/ from the named snapshot
agentrail backup restore latest    # most recent snapshot
agentrail backup diff <ts>         # show what restore would change
```

Restore never overwrites an existing `.agentrail/` silently — it requires
either (a) a missing `.agentrail/`, (b) `--force`, or (c) restoring into an
alternate path with `--into <dir>`.

### Cross-repo recovery

If the user is on a different machine or the repo has moved:

```
agentrail backup list --all
```

…lists every fingerprint under `~/.config/agentrail/repos/`, with the
stored `meta.toml` provenance so the user can identify the right one
before restoring.

## Out of scope for v1

- Encryption of snapshots (the source repo isn't encrypted either; parity
  is acceptable).
- Remote sync (rsync / S3 / cloud drive). Users who want this can point
  `~/.config/agentrail` at a synced directory and pruning will still work.
- Filesystem watching — adds a long-running process that the user didn't
  ask for. In-process hooks cover 95% of the use case without that cost.

## Open questions

- Do we want a `pre-snapshot` hook the user can drop in
  `~/.config/agentrail/hooks/` for custom behavior (e.g., mirror to
  a second location)? Maybe. Defer.
- Should `.agentrail-archive/` also be mirrored? Yes — it's part of the
  durable record. Snapshots should capture both `.agentrail/` and
  `.agentrail-archive/` as a single unit.
- What about the deferred `agentrail snapshot` helper (dangling-blob
  trick)? Keep it as an independent, opt-in command. It's cheap and
  orthogonal — it writes into the repo's own `.git/objects/` store, which
  gives you in-repo recovery without the out-of-tree mirror. Belt and
  suspenders.

## Relationship to `agentrail audit`

`audit` is diagnostic: "what does git history say versus what does
`.agentrail/` say?" The backup mirror is preventative: "hold onto a copy
so audit's job is easier." Audit remains useful even with the mirror, and
the mirror only helps if snapshots were taken before the incident.
