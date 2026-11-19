#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Rung 2: the lockstep gate itself, on Covell's CPU test: both boards, every frame's bus log,
# picture, audio and CPU state. The gate is scripts/lockstep.py; the later rungs are its cases.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
python3 "$ROOT/scripts/lockstep.py" --only '^covell$' --reference-cache "$ROOT/external/refcache" "$@"
echo "rung 2: passed"
