## Recovery: detecting and repairing saga/git drift

The saga is a record of git history. When the two diverge — agent forgets
to commit `.agentrail/`, a step is reverted, files are renamed by hand —
agentrail provides two recovery tools.

### `agentrail audit`

Compares first-parent git history against saga steps and reports gaps.
Default output is a markdown report; `--emit-commands` prints a shell
script of `agentrail add` lines pre-seeded from commit subjects:

```bash
agentrail audit                    # human-readable report
agentrail audit --emit-commands    # draft script (review before running)
agentrail audit --since HEAD~50    # limit scope
```

The emitted script is a draft — slugs and prompts come from commit
subjects and usually need rewording. Review and edit before executing.

### `agentrail snapshot` (belt-and-suspenders)

Captures `.agentrail/` (and `.agentrail-archive/` if present) into a real
git commit under `refs/agentrail/snapshots/<timestamp>`. The blobs are
reachable, survive `git gc`, and can be restored with a normal git
command:

```bash
agentrail snapshot                 # take a snapshot now
agentrail snapshot --list          # list existing refs
git restore --source=<ref> -- .agentrail .agentrail-archive
```

Run a snapshot before risky agent operations or after creating files
you haven't yet staged. It does NOT replace normal git tracking — it is
a safety net for what is not yet committed.
