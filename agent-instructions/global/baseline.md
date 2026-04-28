## Baseline expectations for agentrail-driven sessions

You are working with the agentrail saga/steps process. The saga record (the
`.agentrail/` directory) is the durable handoff between sessions — if your
work is not reflected there, the next session cannot see it.

Before reporting work as complete:

- Confirm the active step is reflected in `.agentrail/` and committed.
- Run the project's pre-commit gate (tests, lint, format) before
  `agentrail complete`, not after.
- If you discover unplanned work, prefer `agentrail insert` / `agentrail add`
  over silently doing it inside another step.
