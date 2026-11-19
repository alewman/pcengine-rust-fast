#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Rung 4: the library. Every HuCard of $LIB and the mapper cards outside it, 300 frames from
# power-on with a RUN press, in lockstep by frame digest. `--smoke` first: the curated 20 of
# scripts/smoke.txt; the full library only when the smoke is clean.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
if [ "${1:-}" = "--smoke" ]; then
    shift
    python3 "$ROOT/scripts/lockstep.py" --smoke --jobs "${JOBS:-16}" \
        --reference-cache "$ROOT/external/refcache" "$@"
    echo "rung 4: the smoke list passed"
    exit 0
fi
python3 "$ROOT/scripts/lockstep.py" --smoke --quiet --jobs "${JOBS:-16}" \
    --reference-cache "$ROOT/external/refcache"
python3 "$ROOT/scripts/lockstep.py" --only '^(lib|map)-' --jobs "${JOBS:-16}" \
    --reference-cache "$ROOT/external/refcache" "$@"
echo "rung 4: passed"
