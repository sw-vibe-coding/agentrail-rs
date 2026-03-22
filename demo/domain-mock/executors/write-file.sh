#!/bin/bash
# Write-file executor: reads JSON params, writes content to output_path
set -euo pipefail
PARAMS=$(cat)
OUTPUT_PATH=$(echo "$PARAMS" | python3 -c "import sys,json; print(json.load(sys.stdin)['output_path'])")
CONTENT=$(echo "$PARAMS" | python3 -c "import sys,json; print(json.load(sys.stdin)['content'])")
echo "$CONTENT" > "$OUTPUT_PATH"
echo "{\"success\": true, \"outputs\": {\"file\": \"$OUTPUT_PATH\"}}"
