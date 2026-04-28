## Push discipline

A commit that hasn't been pushed is invisible to the next session, to
collaborators, and to CI. After running `agentrail complete`, push the
branch:

```bash
git push
```

If you cannot push (no remote, no network, branch protection, etc.):

- Note the unpushed branch state explicitly in your handoff or summary
  so the next session knows to push it.
- Do NOT silently leave commits stranded on a local branch — the next
  agent will look at the remote and assume your work was lost.

If the remote rejects the push:

- For non-fast-forward errors on a personal branch, prefer
  `git pull --rebase` over `--force` unless the user has explicitly
  authorized force-push.
- Never force-push to a shared branch (`main`, release branches) without
  an explicit instruction.
