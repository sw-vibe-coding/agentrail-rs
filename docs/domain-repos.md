# Domain Repository Specification

This document defines the contract that domain-specific knowledge repos
must follow to be usable with agentrail-rs (Layer 1).

## Purpose

Domain repos contain all task-specific knowledge: how to execute tasks,
how to validate outputs, what good execution looks like, and what common
failures to avoid. They are independent, versionable, and shareable.

Separating domain knowledge from the orchestration engine means:
- agentrail-rs stays generic and task-agnostic
- Domain experts maintain their own knowledge repos
- Multiple projects can share the same domain repo
- Domains can evolve independently of the orchestration engine

## Repository Structure

```
agentrail-domain-{name}/
  domain.toml
  skills/
    {task_type}.toml
  experiences/
    {task_type}/
      run_NNN.json
  executors/
    {kind}.sh          (or .rs, .py, or a Cargo crate)
  validators/
    {check_name}.sh    (or .rs, .py)
  graphs/
    {task_type}.toml   (optional)
  README.md
```

## domain.toml

The manifest file that Layer 1 reads to discover what the domain provides.

```toml
[domain]
name = "media"
description = "Audio/video production: TTS, ffmpeg, compositing, probing"
version = "0.1.0"

[[task_types]]
name = "tts"
description = "Text-to-speech audio generation"
executor = "tts-voxcpm"
validators = ["file-exists", "duration-positive"]

[[task_types]]
name = "ffmpeg-concat"
description = "Concatenate video/audio files with ffmpeg"
executor = "ffmpeg-concat"
validators = ["file-exists", "duration-matches"]

[[task_types]]
name = "video-composite"
description = "Composite image + audio into video"
executor = "ffmpeg-composite"
validators = ["file-exists", "resolution-check"]

[[task_types]]
name = "probe"
description = "Probe media file for duration and format info"
executor = "ffprobe"
validators = []
```

## Executors

Executors are invoked by Layer 1 when running deterministic steps. Layer 1
passes the `JobSpec` (kind + params as JSON) to the executor.

### Shell executor interface

```bash
#!/bin/bash
# executors/tts-voxcpm.sh
# Receives job params as JSON on stdin
# Exit 0 on success, non-zero on failure
# Write result JSON to stdout

set -euo pipefail
PARAMS=$(cat)

SCRIPT_PATH=$(echo "$PARAMS" | jq -r '.script_path')
OUTPUT_WAV=$(echo "$PARAMS" | jq -r '.output_wav')
SERVICE_URL=$(echo "$PARAMS" | jq -r '.service_url')
REFERENCE=$(echo "$PARAMS" | jq -r '.reference_voice')

# Execute the task
python3 tts/client.py \
    --script "$SCRIPT_PATH" \
    --output "$OUTPUT_WAV" \
    --service "$SERVICE_URL" \
    --reference "$REFERENCE"

# Output result
echo "{\"status\": \"success\", \"output_file\": \"$OUTPUT_WAV\"}"
```

### Rust executor interface (future)

For performance-critical executors, a domain repo can include a Cargo crate
that implements the `Executor` trait from `agentrail-core`:

```rust
pub trait Executor: Send + Sync {
    fn kind(&self) -> &str;
    fn execute(&self, params: &serde_json::Value) -> Result<ExecutionResult>;
}

pub struct ExecutionResult {
    pub status: ExecutionStatus,
    pub outputs: serde_json::Value,
    pub error: Option<String>,
}
```

## Validators

Validators check outputs against acceptance criteria. Layer 1 invokes them
by name, passing context as JSON.

### Shell validator interface

```bash
#!/bin/bash
# validators/duration-positive.sh
# Receives context JSON on stdin (contains file paths, expected values)
# Exit 0 if valid, exit 1 if invalid
# Write result JSON to stdout

set -euo pipefail
CONTEXT=$(cat)

FILE=$(echo "$CONTEXT" | jq -r '.file')
DURATION=$(ffprobe -v error -show_entries format=duration \
    -of default=noprint_wrappers=1:nokey=1 "$FILE")

if (( $(echo "$DURATION > 0" | bc -l) )); then
    echo "{\"valid\": true, \"duration\": $DURATION}"
    exit 0
else
    echo "{\"valid\": false, \"error\": \"duration is zero or negative\"}"
    exit 1
fi
```

## Knowledge Graphs (Optional)

Domain repos can optionally define expected tool chains as directed graphs.
These enable structured reward signals (MLF-03a: KG as implicit reward model).

```toml
# graphs/tts.toml

[graph]
task_type = "tts"

[[graph.edges]]
from = "read_script"
to = "call_tts_api"
required = true

[[graph.edges]]
from = "call_tts_api"
to = "save_output"
required = true

[[graph.edges]]
from = "save_output"
to = "validate_duration"
required = true

[[graph.edges]]
from = "validate_duration"
to = "validate_format"
required = false
```

When recording an experience, Layer 1 can compare the actual action sequence
against the expected graph to compute a structured reward:
- All required edges followed: reward = +1
- Required edge skipped: reward = -1, failure_mode = "skipped_{edge}"
- Optional edge skipped: no penalty

## Discovery and Registration

Layer 1 discovers domain repos through `.agentrail/domains.toml` in the
project directory:

```toml
[[domain]]
name = "media"
path = "/Users/mike/github/sw-vibe-coding/agentrail-domain-media"

[[domain]]
name = "rust"
path = "/Users/mike/github/sw-vibe-coding/agentrail-domain-rust"
```

On `agentrail next`, Layer 1:
1. Reads `domains.toml`
2. For each domain, reads `domain.toml` to discover task types
3. Matches the current step's `task_type` to a domain
4. Loads skill doc and experiences from that domain
5. Merges with local project skills/experiences (local takes precedence)

## Contributing Experiences Back

Experiences recorded in the local saga can be contributed to the domain repo:

```bash
agentrail contribute <task_type> --to media
```

This copies local experiences to the domain repo's `experiences/` directory.
Domain repo maintainers curate (accept/reject/edit) contributed experiences
before committing them. This is a manual process by design -- experience
quality matters more than quantity.

## Creating a New Domain Repo

```bash
mkdir agentrail-domain-mytools
cd agentrail-domain-mytools
mkdir skills experiences executors validators graphs

cat > domain.toml << 'EOF'
[domain]
name = "mytools"
description = "Description of this domain"
version = "0.1.0"

[[task_types]]
name = "my-task"
description = "What this task does"
executor = "my-executor"
validators = ["my-check"]
EOF
```

Then write skill docs, executors, and validators following the interfaces
above.
