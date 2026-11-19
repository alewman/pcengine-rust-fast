#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Rung 3: picture and sound. The 18 picture-gate titles (600 frames with pad input) and the
# five PSG games (900 frames) in lockstep: every frame's picture, every mixed sample.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
python3 "$ROOT/scripts/lockstep.py" --only '^(pic|psg)-' --jobs "${JOBS:-16}" \
    --reference-cache "$ROOT/external/refcache" "$@"
echo "rung 3: passed"
