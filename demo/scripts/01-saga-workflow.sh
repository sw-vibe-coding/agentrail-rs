#!/bin/bash
# Demo 1: Full saga workflow lifecycle
# Shows: init, next, complete, begin, status, history
set -euo pipefail

DEMO_DIR=$(mktemp -d)
cd "$DEMO_DIR"

echo "=== Demo: Saga Workflow ==="
echo "Working in: $DEMO_DIR"
echo

# Init a saga
echo "$ agentrail init --name rust-calculator --plan 'Build a Rust calculator CLI'"
agentrail init --name rust-calculator --plan "Build a Rust calculator CLI with add, subtract, multiply, divide"
echo

# Check status
echo "$ agentrail status"
agentrail status
echo

# Next shows initial state
echo "$ agentrail next"
agentrail next
echo

# Complete step 0, create step 1 with planned future steps
echo "$ agentrail complete --summary 'Project initialized' --next-slug scaffold --next-prompt 'Create Cargo project with edition 2024' --next-role production --next-task-type rust-project-init --planned 'implement: Add calculator operations' --planned 'test: Write unit tests'"
agentrail complete \
    --summary "Project initialized" \
    --next-slug scaffold \
    --next-prompt "Create Cargo project with edition 2024, add clap dependency, set up main.rs with CLI argument parsing" \
    --next-role production \
    --next-task-type rust-project-init \
    --planned "implement: Add calculator operations" \
    --planned "test: Write unit tests"
echo

# Next now shows step 1 with task type
echo "$ agentrail next"
agentrail next
echo

# Begin the step
echo "$ agentrail begin"
agentrail begin
echo

# Complete step 1, advance to step 2
echo "$ agentrail complete --summary 'Created Cargo project, edition 2024, clippy clean' --next-slug implement --next-prompt 'Implement calculator operations'"
agentrail complete \
    --summary "Created Cargo project with edition 2024, added clap, verified clippy clean" \
    --next-slug implement \
    --next-prompt "Implement add, subtract, multiply, divide operations" \
    --next-role production
echo

# Show history
echo "$ agentrail history"
agentrail history
echo

# Complete and finish
echo "$ agentrail complete --summary 'All operations implemented and tested' --done"
agentrail complete --summary "All calculator operations implemented and tested" --done
echo

# Final status
echo "$ agentrail status"
agentrail status
echo

echo "=== Saga complete! ==="
echo "Cleaning up: $DEMO_DIR"
rm -rf "$DEMO_DIR"
