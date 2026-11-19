#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Chris Covell's PC-Engine CPU Timing Test into external/covell/ (never committed), by the
# URL and SHA-256 pcengine-python pins (its scripts/pinned_files.json).
# Usage: scripts/fetch_covell.sh [DEST]
set -euo pipefail
URL=http://www.chrismcovell.com/data/CPU_Test.zip
SHA256=db65a32d8832663f606afbcedcba3c62e84a28e53001a38bfb4e9bd6c937b625
ROOT=$(cd "$(dirname "$0")/.." && pwd)
DEST=${1:-"$ROOT/external/covell"}
mkdir -p "$DEST"
if [ ! -f "$DEST/CPU_Test.zip" ]; then
    curl -fsSL -o "$DEST/CPU_Test.zip" "$URL"
fi
echo "$SHA256  $DEST/CPU_Test.zip" | sha256sum -c - >/dev/null
unzip -o -j -q "$DEST/CPU_Test.zip" '*CPU_Test_10.pce' -d "$DEST"
echo "Covell's CPU test in $DEST/CPU_Test_10.pce"
