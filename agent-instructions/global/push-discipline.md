## Push discipline

When you push (typically after `branch-handoff` has renamed `feat/X`
to `pr/X` and you're publishing the PR branch), do it safely.

### Verify push capability before assuming it works

- `git remote get-url origin` — `git@…` (SSH) or `https://…` plus a
  configured credential helper is push-capable. A bare `https://…`
  with no helper usually is not (read-only mirror or fresh clone).
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
- If this project has a project-specific handoff flow (release
  engineer scans for `pr/*` branches, queue files, CI hooks
  documented in CLAUDE.md / AGENTS.md outside the briefing markers),
  follow that instead of pushing — the branch rename from
  `branch-handoff.md` may already be sufficient.

### If the remote rejects a push

- For non-fast-forward errors on a personal branch, prefer
  `git pull --rebase` over `--force` unless the user has explicitly
  authorized force-push.
- Never force-push to a shared branch (`main`, release branches)
  without an explicit instruction.
- Never amend a commit that has already been pushed to a shared
  branch.
