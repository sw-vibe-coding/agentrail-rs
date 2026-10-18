## Recovery: detecting and repairing saga/git drift

The saga is a record of git history. When the two diverge — agent forgets
to commit `.agentrail/`, a step is reverted, files are renamed by hand —
agentrail provides two recovery tools.

### Work done where the cursor never goes

A step finished in another lane, branch or repository stays `pending`
here, and once the cursor has passed it nothing brings the cursor back
(`agentrail status` lists such steps as **stranded**). Correct the record
with commands, never by hand-editing step.toml or by `reopen`-ing a
neighbor (that rewrites the neighbor's history):

```bash
agentrail close N --summary "..." --commit REF        # done in this repo
agentrail close N --summary "..." --elsewhere WHERE   # done elsewhere
agentrail edit N --prompt "..."          # a step not yet done, plan changed
agentrail reserve N --for OWNER          # someone else's: the cursor skips it
agentrail unreserve N                    # take it back
```

Reserve a step when you hand it off, before work starts, so it is never
stranded; `close` it when the owner reports it done.

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
