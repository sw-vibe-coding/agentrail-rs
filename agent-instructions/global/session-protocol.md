## CRITICAL: Agentrail session protocol (follow exactly)

Every session follows this sequence. Do not skip steps, do not reorder.

### 1. START — `agentrail next`
First thing in the session. Read the output carefully: it tells you the
current step number, prompt, skill docs, and past trajectories. If `next`
exits non-zero, the saga is complete or absent — do not invent work.

### 2. BEGIN — `agentrail begin`
Run immediately after reading `next` output. This transitions the step from
Pending to InProgress so the recorded history matches what actually
happened.

### 3. WORK
Do what the step prompt says. Do NOT ask the user "want me to proceed?" or
"shall I start?" — the step prompt IS your instruction. Execute it.

### 4. COMMIT
Commit your code changes with git BEFORE running `agentrail complete`.
The commit recorded into the step's `commits` field comes from `HEAD` at
the moment of `complete`, so the commit must land first.

### 5. COMPLETE — `agentrail complete`
```
agentrail complete --summary "what you accomplished" \
  --reward 1 \
  --actions "tools and approach used"
```
On failure: `--reward -1 --failure-mode "what went wrong"`.
On final step: add `--done`.

### 6. STOP
After `complete`, do NOT make further code changes. Anything after
`complete` is untracked and invisible to the next session. If you see more
work to do, it belongs in the NEXT step, not this one.
