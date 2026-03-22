#!/bin/bash
# Echo executor: reads JSON params from stdin, echoes the "message" field
set -euo pipefail
PARAMS=$(cat)
MESSAGE=$(echo "$PARAMS" | python3 -c "import sys,json; print(json.load(sys.stdin).get('message',''))")
echo "{\"success\": true, \"outputs\": {\"message\": \"$MESSAGE\"}}"
