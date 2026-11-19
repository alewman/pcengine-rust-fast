#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Rung 1: the skeleton. The board, huc6280-rust and a plain ROM card run Covell's CPU test
# for 60 frames (the card is read in place, never committed).
# Usage: scripts/rung1.sh [CPU_Test_10.pce]
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
CARD=${1:-${COVELL:-$ROOT/external/covell/CPU_Test_10.pce}}
cargo build --release -q --manifest-path "$ROOT/Cargo.toml"
OUT=$("$ROOT/target/release/pce-trace" "$CARD" --frames 60)
LAST=$(printf '%s\n' "$OUT" | tail -n 1)
LINES=$(printf '%s\n' "$OUT" | wc -l)
case "$LAST" in
    "60 mclk="*" pic=59:"*) ;;
    *) echo "rung 1: FAILED: the 60th line is: $LAST" >&2; exit 1 ;;
esac
[ "$LINES" -eq 60 ] || { echo "rung 1: FAILED: $LINES lines" >&2; exit 1; }
echo "rung 1: passed: Covell's CPU test ran 60 frames (${LAST%% bus=*}, a picture every frame)"
