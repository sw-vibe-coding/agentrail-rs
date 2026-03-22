#!/bin/bash
# File-exists validator: checks that a file exists at the given path
set -euo pipefail
CONTEXT=$(cat)
FILE=$(echo "$CONTEXT" | python3 -c "import sys,json; print(json.load(sys.stdin)['file'])")

if [ -f "$FILE" ]; then
    SIZE=$(wc -c < "$FILE" | tr -d ' ')
    echo "{\"valid\": true, \"details\": {\"file\": \"$FILE\", \"size\": $SIZE}}"
    exit 0
else
    echo "{\"valid\": false, \"error\": \"File not found: $FILE\"}"
    exit 1
fi
