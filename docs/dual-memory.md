# Dual Memory: Skills and Experiences

This document describes the XSkill-inspired dual-memory system that
replaces the single trajectory type in the original design.

## Motivation

The original agentrail-rs design had a single `Trajectory` type for all
learned knowledge. XSkill research (arXiv 2603.12056) demonstrated through
ablation studies that two distinct memory types are both necessary and
complementary:

- **Skills** capture strategic, reusable workflow knowledge
- **Experiences** capture tactical, situation-specific lessons

Neither alone is sufficient. Skills without experiences lack grounding in
real execution. Experiences without skills lack generalized procedure.

## Skills

A skill is a structured workflow document for an entire task category.
Skills are distilled from multiple experiences and represent the current
best-known procedure for a task type.

### Schema

```toml
# .agentrail/skills/tts.toml (or from domain repo: skills/tts.toml)

task_type = "tts"
version = 3
updated_at = "2026-03-20T10:00:00"
distilled_from = 12  # number of experiences analyzed

[procedure]
summary = "Generate TTS audio from a narration script using VoxCPM"
steps = [
    "Read the narration script from the specified path",
    "Call VoxCPM Gradio API at the configured service URL",
    "Pass the script text and reference voice file",
    "Save the output WAV to the specified path",
    "Validate: file exists and duration > 0",
]

[success_patterns]
patterns = [
    "Always use the Gradio client, not raw HTTP",
    "Reference voice file must be the 17-second sample",
    "Output WAV sample rate should be 24kHz",
]

[common_failures]
failures = [
    { mode = "wrong_api", description = "Calling HTTP endpoint instead of Gradio client", frequency = 4 },
    { mode = "missing_reference", description = "Forgetting to pass reference voice file", frequency = 2 },
    { mode = "wrong_sample_rate", description = "Not specifying 24kHz output", frequency = 1 },
]

[output_contract]
required_files = ["output.wav"]
acceptance_checks = ["file-exists", "duration-positive"]
```

### Lifecycle

1. **Seeded manually**: a human or meta agent writes the initial skill doc
2. **Refined by distillation**: `agentrail distill <task_type>` updates the
   skill based on accumulated experiences
3. **Versioned**: each distillation increments the version
4. **Shareable**: skill docs in domain repos can be used across projects

### Storage

Skills live in two places:
- **Local**: `.agentrail/skills/{task_type}.toml` (project-specific overrides)
- **Domain repo**: `skills/{task_type}.toml` (shared baseline)

Local takes precedence over domain repo. Distillation updates the local copy.

## Experiences

An experience is a single execution record with rich context. It extends
the original `Trajectory` type with trigger conditions and failure
documentation.

### Schema

```json
{
    "task_type": "tts",
    "run_id": "run_007",
    "timestamp": "2026-03-20T10:30:00",
    "trigger": {
        "step_role": "deterministic",
        "inputs": {
            "script_path": "work/scripts/03-1.txt",
            "service_url": "http://localhost:7860"
        }
    },
    "actions": [
        { "tool": "gradio_client", "params": { "endpoint": "/tts", "script": "..." } },
        { "tool": "file_write", "params": { "path": "work/audio/03-1.wav" } }
    ],
    "result": {
        "status": "success",
        "outputs": { "audio_file": "work/audio/03-1.wav", "duration_seconds": 4.2 }
    },
    "reward": 1,
    "failure_mode": null,
    "notes": null
}
```

On failure:

```json
{
    "task_type": "tts",
    "run_id": "run_008",
    "timestamp": "2026-03-20T11:00:00",
    "trigger": {
        "step_role": "deterministic",
        "inputs": { "script_path": "work/scripts/04-1.txt" }
    },
    "actions": [
        { "tool": "curl", "params": { "url": "http://localhost:7860/tts" } }
    ],
    "result": {
        "status": "failure",
        "error": "Connection refused"
    },
    "reward": -1,
    "failure_mode": "wrong_api",
    "notes": "Used curl instead of Gradio client; server only accepts Gradio protocol"
}
```

### Storage

Experiences live in two places:
- **Local**: `.agentrail/experiences/{task_type}/run_NNN.json`
- **Domain repo**: `experiences/{task_type}/run_NNN.json` (curated library)

Retrieval merges both sources. Local experiences are always saved.
Contributing to the domain repo is a deliberate curation step.

## Retrieval Strategy

When `agentrail next` runs for a step with a known `task_type`:

1. **Load skill**: find the skill doc (local override > domain repo baseline)
2. **Load experiences**: merge local + domain repo experiences for the task type
3. **Filter**: keep only reward > 0 (successes)
4. **Rank**: most recent first (recency bias captures tool/API changes)
5. **Limit**: top N experiences (default: 3)
6. **Format**: inject skill procedure + experience examples into prompt output

### Prompt injection format

```
=== SKILL: tts ===
Procedure:
  1. Read the narration script from the specified path
  2. Call VoxCPM Gradio API at the configured service URL
  ...

Known failure modes:
  - wrong_api: Calling HTTP endpoint instead of Gradio client (seen 4 times)
  ...

=== RECENT SUCCESSFUL EXPERIENCES ===

[run_007] 2026-03-20
  Actions: gradio_client(/tts) -> file_write(work/audio/03-1.wav)
  Result: success, duration=4.2s
  Reward: +1

[run_005] 2026-03-19
  Actions: gradio_client(/tts) -> file_write(work/audio/02-1.wav)
  Result: success, duration=3.8s
  Reward: +1
```

## Distillation

The `agentrail distill <task_type>` command implements XSkill's
accumulation phase:

1. Load all experiences for the task type (successes and failures)
2. Group by success/failure
3. Extract common action patterns from successes
4. Extract and count failure modes from failures
5. Generate or update the skill document:
   - Procedure steps from successful action sequences
   - Success patterns from commonalities
   - Failure modes with frequencies
   - Output contract from successful result patterns
6. Increment skill version, update timestamp

Distillation is run manually or on a schedule (e.g., after every N new
experiences). It is the "accumulation phase" of the XSkill two-phase loop.

## Backward Compatibility

The existing `Trajectory` type and `trajectories/` storage continue to work.
Migration path:

- Existing trajectories are readable as minimal experiences (reward field
  maps directly, other fields default to null/empty)
- New experiences written to `experiences/` directory
- Existing `retrieve_successes()` function continues to work against old data
- Skill documents are purely additive (no existing data to migrate)
