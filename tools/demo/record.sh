#!/usr/bin/env bash
# Record the demo videos: ./record.sh [develop|cull|merge|sky ...] (all four by default).
# The site must be up at $DEMO_URL (http://localhost:8790 by default).
set -euo pipefail
cd "$(dirname "$0")"
for name in "${@:-develop cull merge sky}"; do
  for video in $name; do
    tts/venv/bin/python tts/say.py "narration/$video.json" "out/$video"
    node "$video.mjs"
    python3 assemble.py "$video"
    python3 web.py "$video"
  done
done
