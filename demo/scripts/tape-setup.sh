#!/bin/bash
# Setup script for VHS tape -- creates a pre-populated demo environment
set -euo pipefail

DEMO_DIR=$(mktemp -d)
cd "$DEMO_DIR"

SCRIPT_DIR="${1:-.}"

agentrail init --name my-greeter --plan "Create a Rust greeter CLI" >/dev/null 2>&1

# Load skill
mkdir -p .agentrail/skills
cp "$SCRIPT_DIR/demo/skills/rust-project-init.toml" .agentrail/skills/

# Pre-populate trajectories
mkdir -p .agentrail/trajectories/rust-project-init
cat > .agentrail/trajectories/rust-project-init/run_001.json << 'EOF'
{"task_type":"rust-project-init","state":{},"action":"cargo init + edition 2024 + clippy","result":"clean project, zero warnings","reward":1,"timestamp":"2026-03-19T14:30:00"}
EOF
cat > .agentrail/trajectories/rust-project-init/run_002.json << 'EOF'
{"task_type":"rust-project-init","state":{},"action":"cargo init + edition 2024 + clippy","result":"edition 2024, all gates pass","reward":1,"timestamp":"2026-03-20T09:15:00"}
EOF

# Create step with task_type
agentrail complete --summary "Initialized" \
    --next-slug create-project \
    --next-prompt "Create the greeter Rust project with edition 2024" \
    --next-role production \
    --next-task-type rust-project-init >/dev/null 2>&1

echo "$DEMO_DIR"
