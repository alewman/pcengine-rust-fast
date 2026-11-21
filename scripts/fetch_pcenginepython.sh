#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The reference, pcengine-python at 03ac699 (the clean rewrite of 2026-11-19,
# 3318027, and the cropped early display start; on GitHub since 2026-11-20), into external/
# with a PyPy venv (PyPy runs it several times faster than CPython). Its CPU core,
# huc6280-python 6ce348e, and the core's frame, 6502-python 2146eba, come with it (each pinned
# in the pyproject before it).
# Usage: scripts/fetch_pcenginepython.sh [DEST] [SOURCE]
#   SOURCE: a git URL or a local clone (default: GitHub); the commit is fetched
#   from it, so a local clone that has the commit works too.
#   CORE_SOURCE: a local clone of huc6280-python that has the core's commit, to
#   install the core from instead of the pyproject's GitHub pin (its own dependency,
#   6502-python, still comes by the core's pin).
set -euo pipefail
COMMIT=03ac6998c812a291e5f91882552df5ae712ba870  # the clean rewrite + the early display start crop
CORE_COMMIT=6ce348e13c9ef3009bd07e7938c4b48956eeadc6  # huc6280-python, the clean rewrite
ROOT=$(cd "$(dirname "$0")/.." && pwd)
DEST=${1:-"$ROOT/external/pcengine-python"}
SOURCE=${2:-https://github.com/alewman/pcengine-python.git}
PYTHON=${PYTHON:-pypy3}
if [ ! -d "$DEST/.git" ]; then
    git clone -q "$SOURCE" "$DEST"
fi
git -C "$DEST" fetch -q --tags "$SOURCE" "$COMMIT" 2>/dev/null || true
git -C "$DEST" checkout -q "$COMMIT"
[ "$(git -C "$DEST" rev-parse HEAD)" = "$COMMIT" ] || { echo "not at $COMMIT" >&2; exit 1; }
if [ ! -x "$DEST/.venv-pypy/bin/python" ]; then
    "$PYTHON" -m venv "$DEST/.venv-pypy"
fi
if [ -n "${CORE_SOURCE:-}" ]; then
    CORE=$(cd "$CORE_SOURCE" && pwd)
    # --force-reinstall: the core's version number may not change between commits. With its
    # dependencies: the core needs 6502-python (a --no-deps here left it out, 2026-11-20).
    "$DEST/.venv-pypy/bin/python" -m pip install -q --force-reinstall "git+file://$CORE@$CORE_COMMIT"
    # The board without its dependencies: its one dependency is the core just installed.
    "$DEST/.venv-pypy/bin/python" -m pip install -q --force-reinstall --no-deps -e "$DEST"
else
    "$DEST/.venv-pypy/bin/python" -m pip install -q --force-reinstall -e "$DEST"
fi
echo "pcengine-python $(git -C "$DEST" rev-parse --short HEAD) in $DEST"
