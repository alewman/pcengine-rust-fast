#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Rung 6: save states. The refusals and the round trip on a synthetic card as unit tests
# (tests/state.rs), then the round trip and the run-ahead pass on the 18 picture-gate titles
# (scripts/state_roundtrip.py).
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cargo test --release -q --manifest-path "$ROOT/Cargo.toml" --test state
python3 "$ROOT/scripts/state_roundtrip.py" --jobs "${JOBS:-12}" "$@"
echo "rung 6: passed"
