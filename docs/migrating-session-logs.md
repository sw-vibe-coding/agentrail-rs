# Moving a repository off in-repo session logs

Older versions of agentrail kept each Claude Code session log in
`.agentrail/sessions/` and committed it with every saga record. A log only
grows, so history holds a full copy per step, and the log quotes every file
the agent read. This guide moves such a repository to a clean successor:
the old repository stays as it is, deprecated, and work continues in a new,
small one.

The method is **clone, fix, push to a new empty repository**. Nothing is
force-pushed and the old repository's history is not touched.

## Why not GitHub's Fork button

- A GitHub fork shares object storage with its parent. Rewriting and
  force-pushing the fork does not remove the logs from the fork network;
  they stay reachable by commit hash through the fork's own URL.
- Pull requests from a fork default to the parent as base, issues are off
  by default, and a fork of a public repository cannot be made private.
- A new repository never receives the large objects at all.

## Before you start

- Install the current `agentrail` (`sw-install`) and
  [`git filter-repo`](https://github.com/newren/git-filter-repo).
  `agentrail shadow` also needs `xz`.
- Create the new, **empty** repository (no README, no licence) in the
  organisation where the project will live.
- Finish or park work in the old checkout. Push every branch you want to
  keep: the steps below start from a fresh clone.

## Steps

Run all of these in a fresh clone, in the directory you will keep working
in. The session store is keyed by the checkout's path, so logs migrated
here belong to this directory.

```bash
git clone git@github.com:OLD-ORG/project.git project
cd project
```

### 1. Move the logs out of the working tree

```bash
agentrail sessions migrate
git commit -m "chore: move session logs out of the repository"
```

`migrate` moves the logs from `.agentrail/sessions/` and
`.agentrail-archive/*/sessions/` to `~/.agentrail/sessions/<repo-key>/`
and untracks them. It does not rewrite history. If the logs were already
gitignored there is nothing to commit.

Remove any `.gitignore` entry for `.agentrail/sessions/` in the same
commit; `.agentrail/` must stay tracked and nothing writes logs there now.

### 2. Remove the logs from history

```bash
git filter-repo --invert-paths \
  --path .agentrail/sessions \
  --path-glob '.agentrail-archive/*/sessions/*'
```

This rewrites every commit, drops the `origin` remote, and writes the
old-to-new hash pairs to `.git/filter-repo/commit-map`. Check `git branch`
afterwards: every branch you want in the new repository should be listed.

### 3. Repair the commit hashes the steps recorded

Each step records the commits made for it, and the rewrite changed them
all.

```bash
agentrail remap .git/filter-repo/commit-map --dry-run   # preview
agentrail remap .git/filter-repo/commit-map
agentrail remap                                         # expect: nothing dangling
git add -A .agentrail .agentrail-archive
git commit -m "saga: remap recorded commits after history rewrite"
```

Keep a copy of `commit-map` outside the clone (the shadow store is a good
place). It is the only link between old and new hashes.

Commits that were already dangling before the rewrite (an earlier squash
merge or rebase) are not in the map. Fix those one at a time with
`agentrail remap --from OLD --to NEW`, or leave them: `agentrail audit`
falls back to commit times.

### 4. Refresh the briefing

```bash
agentrail instructions apply
git commit -am "docs: refresh agentrail briefing"
```

### 5. Check, then push to the new repository

```bash
agentrail check-size          # exit 0: nothing oversize under .agentrail/
agentrail status              # no in-repo session log warning
du -sh .git

git remote add origin git@github.com:NEW-ORG/project.git
git push -u origin --all
git push origin --tags
```

### 6. Optional: keep the logs in a private shadow store

```bash
git clone git@github.com:NEW-ORG/project-agentrail.git ../project-agentrail   # private
agentrail shadow set ../project-agentrail
agentrail shadow push
```

`shadow push` writes each log xz-compressed with its SHA-256 beside it,
commits and pushes. On another machine, `agentrail shadow set` the same
clone and `agentrail shadow pull` to restore them.

### 7. Deprecate the old repository

In the old repository, replace the top of `README.md` with a notice that
links to the new one, then archive it (Settings → Archive this
repository) so it is read-only:

```markdown
> **This repository is deprecated.** Development continues at
> https://github.com/NEW-ORG/project, which has the same history without
> the session logs.
```

The old repository still contains every committed session log. **If it is
public, whatever those logs quoted stays published**: files that must not
be redistributed, or a secret read during a session. If that applies, make
the old repository private (or delete it) instead of only archiving it, and
rotate any secret that appeared in a log.

## A repository whose history was already rewritten

If the logs were removed earlier with `git filter-repo` (for example
before agentrail could do it), skip step 2 and use the `commit-map` saved
from that rewrite in step 3. Run step 1 anyway: it moves logs that are
still on disk but gitignored.

## What does not carry over

- Old steps have no `transcript.md`. `agentrail complete` writes one from
  now on; none are generated for past steps.
- The session store and the shadow target are per machine and per checkout
  path. A second dev system starts empty until `shadow pull`.
