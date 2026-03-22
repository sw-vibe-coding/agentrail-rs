#!/bin/bash
# Content-matches validator: checks that file contains expected content
set -euo pipefail
CONTEXT=$(cat)
FILE=$(echo "$CONTEXT" | python3 -c "import sys,json; print(json.load(sys.stdin)['file'])")
EXPECTED=$(echo "$CONTEXT" | python3 -c "import sys,json; print(json.load(sys.stdin)['expected'])")

if [ ! -f "$FILE" ]; then
    echo "{\"valid\": false, \"error\": \"File not found: $FILE\"}"
    exit 1
fi

ACTUAL=$(cat "$FILE")
if echo "$ACTUAL" | grep -q "$EXPECTED"; then
    echo "{\"valid\": true, \"details\": {\"file\": \"$FILE\", \"matched\": \"$EXPECTED\"}}"
    exit 0
else
    echo "{\"valid\": false, \"error\": \"Content does not contain: $EXPECTED\"}"
    exit 1
fi
