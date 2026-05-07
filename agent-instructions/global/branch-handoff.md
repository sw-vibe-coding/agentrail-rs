## Branch handoff after `agentrail complete`

Once a feature branch is committed and `agentrail complete` has run,
signal that it is ready for review/merge by renaming it from
`feat/<slug>` to `pr/<slug>`:

```bash
git branch -m feat/<slug> pr/<slug>
```

The rename is a useful convention regardless of how this project
handles handoff:

- It distinguishes "in-progress work" from "awaiting review" in the
  output of `git branch`.
- Project-specific tooling (CI, release-engineer scans, automated PR
  creation) can key off the `pr/*` prefix.
- It keeps the branch name stable across the review cycle even if the
  feature slug evolves.

What happens after the rename depends on what push capability you
have. See `push-discipline.md` for how to detect capability before
trying.

- **You can push and use GitHub PRs.** Push the renamed branch and
  open the PR:
  ```bash
  git push -u origin pr/<slug>
  gh pr create
  ```
  (Or push and open the PR via the web UI.)
- **You cloned via HTTPS without push access to the upstream remote.**
  Typical when contributing to an open-source project you don't own:
  push the renamed branch to your fork instead, then open the PR
  pointing at upstream. `git remote -v` shows your remotes; add a
  fork remote with `git remote add fork <url>` if needed.
- **Your project has a project-specific handoff flow** (release
  engineer scanning for `pr/*` branches, automated PR creation, CI
  hooks). The rename IS the handoff signal. Look for site-specific
  guidance in CLAUDE.md / AGENTS.md outside the briefing markers — if
  present, follow that instead.

After the rename, do not continue committing to `pr/<slug>` — those
commits will not be in the PR. Start further work on a fresh branch
off the (eventually merged and pulled) base.
