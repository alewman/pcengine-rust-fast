#!/usr/bin/env bash
# The board under wasm: build wasm-bench/ for wasm32-unknown-unknown (the rustup 1.93.1
# toolchain: /usr/bin/cargo has no wasm std), run the five titles under Node, and compare the
# PSG's level table (the platform's powf) with the native build's.
#
#   scripts/wasm-bench.sh                 # this tree
#   scripts/wasm-bench.sh --base DIR      # the same bench on another checkout of the board
#                                         # (the fork point: /data/emu/pcengine-rust)
#   FRAMES=600 ROUNDS=5 CPU=31
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
TOOLCHAIN=${TOOLCHAIN:-"$HOME/.rustup/toolchains/1.93.1-x86_64-unknown-linux-gnu"}
export RUSTC="$TOOLCHAIN/bin/rustc"
CARGO="$TOOLCHAIN/bin/cargo"
CRATE="$ROOT/wasm-bench"
OUT="$ROOT/external/wasm"
if [ "${1:-}" = "--base" ]; then
    # a copy of the bench crate that depends on the other checkout; nothing is written there
    BASE=$(cd "$2" && pwd)
    NAME=$(grep -m1 '^name = ' "$BASE/Cargo.toml" | cut -d'"' -f2)
    CRATE="$ROOT/external/wasm-base/crate"
    OUT="$ROOT/external/wasm-base"
    mkdir -p "$CRATE/src"
    cp "$ROOT/wasm-bench/src/lib.rs" "$ROOT/wasm-bench/src/main.rs" "$CRATE/src/"
    python3 - "$ROOT/wasm-bench/Cargo.toml" "$CRATE/Cargo.toml" "$NAME" "$BASE" <<'PY'
import sys
source, out, name, base = sys.argv[1:]
text = open(source).read().replace(
    'package = "pcengine-rust-fast", path = ".."', f'package = "{name}", path = "{base}"')
open(out, "w").write(text)
PY
fi
nice "$CARGO" build --release -q --manifest-path "$CRATE/Cargo.toml" --target-dir "$OUT/target" \
    --target wasm32-unknown-unknown --lib
nice "$CARGO" build --release -q --manifest-path "$CRATE/Cargo.toml" --target-dir "$OUT/target" --bin pce-wasm-bench
WASM="$OUT/target/wasm32-unknown-unknown/release/pce_wasm_bench.wasm"
if node "$ROOT/wasm-bench/bench.mjs" "$WASM" --levels | cmp -s - <("$OUT/target/release/pce-wasm-bench"); then
    echo "the PSG level table: the 128 values under wasm are the native build's, bit for bit"
else
    echo "THE PSG LEVEL TABLE DIFFERS UNDER WASM (powf), in these entries (native, wasm):"
    paste <("$OUT/target/release/pce-wasm-bench") <(node "$ROOT/wasm-bench/bench.mjs" "$WASM" --levels) \
        | awk '$1 != $2 { print "    " NR - 1 ": " $1 " " $2 }'
    echo "so a snapshot's bytes differ (check); 'output' below says whether a sample or a pixel did"
fi
python3 "$ROOT/scripts/ab.py" --fps true --rounds 0 >/dev/null 2>&1 || true   # extracts scratch/bench
B="$ROOT/scratch/bench"
run() {
    nice taskset -c "${CPU:-31}" node "$ROOT/wasm-bench/bench.mjs" "$WASM" "$@" --frames "${FRAMES:-600}" --rounds "${ROUNDS:-5}"
    echo "    native: $(nice "$OUT/target/release/pce-wasm-bench" "$@" "${FRAMES:-600}")"
}
run "$B/Blazing_Lazers.pce" tg16
run "$B/Bonk_s_Adventure.pce" tg16
run "$B/R_Type.pce" tg16
run "$B/After_Burner_II.pce"
run "$B/Street_Fighter_II_.pce"
