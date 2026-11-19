#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# Rung 5: machines, sockets and switches. A test device in every socket (src/fake_devices.rs,
# the same devices in scripts/pce_trace.py), the TurboGrafx-16 row, a power-on seed and each
# named switch away from its default, in lockstep; then scripts/switch_effect.py, which says
# which of those cases the switch or device actually changes (a case it does not change proves
# nothing about the wiring). The socket unit tests are tests/sockets.rs.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
cargo test --release -q --manifest-path "$ROOT/Cargo.toml" --test sockets --test pads
python3 "$ROOT/scripts/lockstep.py" --only '^(base|sock|mach|sw)-' --jobs "${JOBS:-16}" \
    --reference-cache "$ROOT/external/refcache" "$@"
python3 "$ROOT/scripts/switch_effect.py"
echo "rung 5: passed"
