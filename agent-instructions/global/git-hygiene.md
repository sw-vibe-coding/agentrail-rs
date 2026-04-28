## Git hygiene

- Never amend a commit that has already been pushed to a shared branch.
- Never use `--no-verify` to skip pre-commit hooks unless explicitly told to.
  If a hook fails, fix the underlying issue.
- Prefer naming files explicitly (`git add path/to/file`) over `git add -A`,
  to avoid sweeping in unintended files (secrets, local scratch, etc.).
- Resolve merge conflicts; do not discard changes by default.
