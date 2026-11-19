#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# A/B two source trees of this board under several code layouts.
#
# Two builds of one source can differ by several per cent from where the linker puts the hot
# blocks alone (README, "Measuring"), so one build a side measures layout luck as well as the
# change. Each tree's pce-bench is built under each layout (RUSTFLAGS; fat LTO and one codegen
# unit, the crate's release profile) and the pair is timed by scripts/ab.py (alternating on one
# pinned core, best of ROUNDS a side). A change is kept when it wins on every layout.
#
#   scripts/ab.sh TREE_A TREE_B [ab.py arguments, e.g. --behaviour-changed]
#   env: ROUNDS (default 12), CPU (31), LAYOUTS ("default nf5 nf6"), OUT (scratch/ab-layouts)
#
# A tree is a checkout of a crate with this fork's src/bin/pce-bench.rs (a worktree of an older
# revision, or of pcengine-rust with the fork's bench copied in) and a release profile with
# lto = "fat" and codegen-units = 1; a relative `path =` dependency must resolve from it.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
A=$(cd "$1" && pwd)
B=$(cd "$2" && pwd)
shift 2
ROUNDS=${ROUNDS:-12}
CPU=${CPU:-31}
LAYOUTS=${LAYOUTS:-"default nf5 nf6"}
OUT=${OUT:-$ROOT/scratch/ab-layouts}
declare -A FLAGS=(
    [default]=""
    [nf5]="-C llvm-args=-align-all-nofallthru-blocks=5"
    [nf6]="-C llvm-args=-align-all-nofallthru-blocks=6"
)
mkdir -p "$OUT"
build() { # tree side layout
    local target="$OUT/target-$2-$3"
    RUSTFLAGS="${FLAGS[$3]}" nice cargo build --release -q --bin pce-bench \
        --manifest-path "$1/Cargo.toml" --target-dir "$target" -j "${CARGO_BUILD_JOBS:-8}"
    cp "$target/release/pce-bench" "$OUT/pce-bench-$2-$3"
}
for layout in $LAYOUTS; do
    build "$A" a "$layout"
    build "$B" b "$layout"
done
for layout in $LAYOUTS; do
    echo "== layout $layout"
    nice python3 "$ROOT/scripts/ab.py" "$OUT/pce-bench-a-$layout" "$OUT/pce-bench-b-$layout" \
        --rounds "$ROUNDS" --cpu "$CPU" "$@"
done
