## Push discipline

A commit that has not been pushed is invisible to the next session, to
collaborators, and to CI. After `agentrail complete`, push the branch:

```bash
git push
```

### Verify push capability before assuming it works

Before retrying or panicking on a failed push, confirm whether this
checkout is allowed to push at all:

- `git remote get-url origin` — `git@…` (SSH) or `https://…+credential
  helper` is push-capable; a bare `https://…` with no helper usually
  is not (read-only mirror).
- `gh auth status` — if the project uses GitHub PRs and `gh` is not
  authenticated, `gh pr create` will fail.
- `git push --dry-run` — surfaces auth or branch-protection problems
  without changing remote state.

### If you cannot push (or push fails)

- Note the unpushed branch state explicitly in your `agentrail
  complete --summary` so the next session knows the work is local.
- Do NOT silently leave commits stranded on a local branch — the next
  agent will look at the remote and assume your work was lost.
- Do NOT retry the same `git push` in a loop. If the failure is a
  permission issue (no SSH key, no `gh` auth, sandboxed user), retries
  will not fix it. Surface the failure to the user.
- This project may have a project-specific handoff convention for
  sandboxed users (e.g. branch renaming, queue files, CI hooks) — if
  CLAUDE.md / AGENTS.md describes one outside the briefing markers,
  follow it instead of pushing.

### If the remote rejects a push

- For non-fast-forward errors on a personal branch, prefer
  `git pull --rebase` over `--force` unless the user has explicitly
  authorized force-push.
- Never force-push to a shared branch (`main`, release branches)
  without an explicit instruction.
- Never amend a commit that has already been pushed to a shared
  branch.
