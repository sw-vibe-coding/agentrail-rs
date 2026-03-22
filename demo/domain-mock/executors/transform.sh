#!/bin/bash
# Transform executor: reads input_path, applies transformation, writes to output_path
set -euo pipefail
PARAMS=$(cat)
INPUT_PATH=$(echo "$PARAMS" | python3 -c "import sys,json; print(json.load(sys.stdin)['input_path'])")
OUTPUT_PATH=$(echo "$PARAMS" | python3 -c "import sys,json; print(json.load(sys.stdin)['output_path'])")
TRANSFORM=$(echo "$PARAMS" | python3 -c "import sys,json; print(json.load(sys.stdin).get('transform','uppercase'))")

if [ "$TRANSFORM" = "uppercase" ]; then
    tr '[:lower:]' '[:upper:]' < "$INPUT_PATH" > "$OUTPUT_PATH"
elif [ "$TRANSFORM" = "reverse" ]; then
    rev < "$INPUT_PATH" > "$OUTPUT_PATH"
else
    cp "$INPUT_PATH" "$OUTPUT_PATH"
fi

echo "{\"success\": true, \"outputs\": {\"file\": \"$OUTPUT_PATH\", \"transform\": \"$TRANSFORM\"}}"
