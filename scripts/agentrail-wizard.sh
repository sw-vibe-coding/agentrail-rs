#!/bin/bash
# agentrail-wizard.sh -- Interactive setup wizard for new projects
# Usage: bash /path/to/agentrail-wizard.sh
set -euo pipefail

echo "=== AgentRail Setup Wizard ==="
echo

# 1. Project directory
PROJ_DIR="${1:-.}"
PROJ_DIR=$(cd "$PROJ_DIR" && pwd)
echo "Project directory: $PROJ_DIR"
echo

# 2. Project name
DEFAULT_NAME=$(basename "$PROJ_DIR")
read -rp "Project name [$DEFAULT_NAME]: " NAME
NAME="${NAME:-$DEFAULT_NAME}"

# 3. Plan
echo
echo "Describe your project plan. What needs to be done?"
echo "  (Multi-line: type your plan, then press Enter on an empty line to finish)"
echo "  (Or enter a path to a plan file)"
echo
PLAN=""
while IFS= read -rp "> " LINE; do
    [ -z "$LINE" ] && break
    PLAN="${PLAN}${LINE}\n"
done

if [ -z "$PLAN" ]; then
    echo "No plan entered. Using default."
    PLAN="Build $NAME"
fi

# Check if plan is a file path
if [ -f "$PLAN" ]; then
    echo "  (Reading plan from file: $PLAN)"
fi

# 4. Domain repo
echo
echo "Do you have a domain repo with skills? (e.g., agentrail-domain-coding)"
read -rp "Domain repo path (or Enter to skip): " DOMAIN_PATH

# 5. First step
echo
echo "What should the first step be?"
read -rp "Step slug (short name, e.g., 'scaffold', 'research', 'setup'): " FIRST_SLUG
read -rp "Step description/prompt: " FIRST_PROMPT
echo
echo "Step role:"
echo "  1. production  (agent does semantic work)"
echo "  2. deterministic  (auto-executable, no agent needed)"
echo "  3. meta  (prepares handoff for next step)"
echo "  4. validation  (checks outputs)"
read -rp "Role [1]: " ROLE_NUM
case "${ROLE_NUM:-1}" in
    2) FIRST_ROLE="deterministic" ;;
    3) FIRST_ROLE="meta" ;;
    4) FIRST_ROLE="validation" ;;
    *) FIRST_ROLE="production" ;;
esac

read -rp "Task type (e.g., 'rust-project-init', 'c-compile-fix', or Enter to skip): " TASK_TYPE

# 6. Planned steps
echo
echo "Plan additional steps? (Enter 'slug: description' per line, empty to finish)"
PLANNED_ARGS=""
while IFS= read -rp "planned> " PLANNED_LINE; do
    [ -z "$PLANNED_LINE" ] && break
    PLANNED_ARGS="$PLANNED_ARGS --planned \"$PLANNED_LINE\""
done

# 7. Execute
echo
echo "=== Setting up AgentRail ==="
echo

# Build the setup command
DOMAIN_ARG=""
if [ -n "$DOMAIN_PATH" ]; then
    DOMAIN_ARG="--domain $DOMAIN_PATH"
fi

echo "$ agentrail setup --name \"$NAME\" --plan \"...\" $DOMAIN_ARG"
cd "$PROJ_DIR"
# shellcheck disable=SC2086
agentrail setup --name "$NAME" --plan "$(echo -e "$PLAN")" $DOMAIN_ARG

# Create first step
TASK_TYPE_ARG=""
if [ -n "$TASK_TYPE" ]; then
    TASK_TYPE_ARG="--next-task-type $TASK_TYPE"
fi

echo "$ agentrail complete --summary \"Project initialized\" --next-slug \"$FIRST_SLUG\" ..."
# shellcheck disable=SC2086
eval agentrail complete \
    --summary \"Project initialized\" \
    --next-slug \"$FIRST_SLUG\" \
    --next-prompt \"$FIRST_PROMPT\" \
    --next-role "$FIRST_ROLE" \
    $TASK_TYPE_ARG \
    $PLANNED_ARGS

echo
echo "=== Setup complete! ==="
echo
echo "To start working:"
echo "  cd $PROJ_DIR"
echo "  claude \"go\""
echo
echo "To verify the setup:"
echo "  agentrail next"
