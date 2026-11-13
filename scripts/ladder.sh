#!/usr/bin/env bash
# The fork's gate: every change must leave this board identical to pcengine-rust.
# cargo test, every lockstep case against the reference traces (external/refcache:
# pcengine-rust's, which are pcengine-python's at v0.1.0 on all 275 cases), then the save-state
# round trip and the run-ahead pass on the 18 picture-gate titles (rung 6). `--faults` adds the
# seeded faults (rung 7, several minutes). At most 12 processes, niced.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
JOBS=${JOBS:-12}
nice cargo build --release -q --manifest-path "$ROOT/Cargo.toml"
nice cargo test --release -q --manifest-path "$ROOT/Cargo.toml" 2>&1 \
    | grep -E "test result|FAILED|panicked|error" | grep -v " 0 passed" || true
nice python3 "$ROOT/scripts/lockstep.py" --jobs "$JOBS" --quiet \
    --reference-cache "$ROOT/external/refcache" | tail -5
nice python3 "$ROOT/scripts/state_roundtrip.py" --jobs "$JOBS" | tail -1
if [ "${1:-}" = "--faults" ]; then
    nice python3 "$ROOT/scripts/seeded_faults.py" --jobs "$JOBS" | tail -3
fi
