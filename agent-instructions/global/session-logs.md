## Session logs stay out of the repository

`agentrail complete` copies your session log (the JSONL transcript) to a
store **outside** the repository: `~/.agentrail/sessions/<repo-key>/`
(`agentrail sessions path` prints it). Never commit a session log, and
never copy one into the working tree.

- **Privacy.** A session log quotes every file you read, every command's
  output, and your reasoning. Committed to a public repository it publishes
  all of that: licensed or third-party files the repository must not
  redistribute, and any secret that crossed your screen.
- **Size.** A log only grows, so each commit of it stores the whole log
  again. One long session passes GitHub's 100 MiB file limit and the push
  is refused — after the commits holding it already exist.

If `agentrail next`, `status` or `complete` reports session logs inside the
repository (older agentrail versions wrote them to `.agentrail/sessions/`),
run `agentrail sessions migrate` and commit the staged removal. If it
reports a file over the size limit under `.agentrail/`, do not stage that
file; use the `git add` line agentrail prints, which leaves it out. Tell
the user either way: logs already committed stay in history until the
repository's owner decides to rewrite it.
