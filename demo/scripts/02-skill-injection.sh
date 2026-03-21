#!/bin/bash
# Demo 2: Skill injection -- shows how agentrail next injects skill docs
# and past trajectories into the agent's prompt
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
DEMO_DIR=$(mktemp -d)
cd "$DEMO_DIR"

echo "=== Demo: Skill + Trajectory Injection ==="
echo "Working in: $DEMO_DIR"
echo

# Init saga
agentrail init --name tts-demo --plan "Generate TTS audio for video segments"

# Copy skill into the saga's .agentrail/skills/ directory
SAGA_DIR="$DEMO_DIR/.agentrail"
mkdir -p "$SAGA_DIR/skills"
cp "$SCRIPT_DIR/skills/rust-project-init.toml" "$SAGA_DIR/skills/"
cp "$SCRIPT_DIR/skills/clippy-fix.toml" "$SAGA_DIR/skills/"

echo "--- Loaded skill docs into .agentrail/skills/ ---"
ls "$SAGA_DIR/skills/"
echo

# Pre-populate some trajectory data (simulating past successful runs)
mkdir -p "$SAGA_DIR/trajectories/rust-project-init"
cat > "$SAGA_DIR/trajectories/rust-project-init/run_001.json" << 'TRAJ'
{
    "task_type": "rust-project-init",
    "state": {"project": "video-editor"},
    "action": "cargo init + set edition 2024 + clippy check",
    "result": "project created, edition 2024, zero warnings",
    "reward": 1,
    "timestamp": "2026-03-19T14:30:00"
}
TRAJ
cat > "$SAGA_DIR/trajectories/rust-project-init/run_002.json" << 'TRAJ'
{
    "task_type": "rust-project-init",
    "state": {"project": "audio-tools"},
    "action": "cargo init + set edition 2024 + clippy check",
    "result": "project created, edition 2024, zero warnings",
    "reward": 1,
    "timestamp": "2026-03-20T09:15:00"
}
TRAJ

echo "--- Pre-populated 2 successful trajectories for rust-project-init ---"
echo

# Create step 1 with task_type that matches the skill
agentrail complete \
    --summary "Saga initialized" \
    --next-slug create-project \
    --next-prompt "Create a new Rust project for the TTS pipeline" \
    --next-role production \
    --next-task-type rust-project-init

echo
echo "=== Now run 'agentrail next' to see skill + trajectory injection ==="
echo
agentrail next
echo

echo "--- Notice: the agent now sees the procedure, success patterns,"
echo "    known failure modes, AND past successful runs. ---"
echo

# Now show the clippy-fix skill too
agentrail begin
agentrail complete \
    --summary "Rust project created with edition 2024" \
    --next-slug fix-warnings \
    --next-prompt "Fix all clippy warnings in the project" \
    --next-role production \
    --next-task-type clippy-fix

echo
echo "=== Step 2: clippy-fix skill injection ==="
echo
agentrail next

echo
echo "=== Demo complete ==="
rm -rf "$DEMO_DIR"
