#!/usr/bin/env bash
# SPDX-License-Identifier: MIT
# The reference, pcengine-python at 3318027 (the clean rewrite of 2026-11-19,
# after v0.2.0; local, not on GitHub yet), into external/ with a PyPy venv
# (PyPy runs it several times faster than CPython). Its CPU core,
# huc6280-python 6ce348e (also local), comes with it (pinned in its pyproject).
# Until both are pushed: SOURCE=a local pcengine-python clone and
# CORE_SOURCE=a local huc6280-python clone.
# Usage: scripts/fetch_pcenginepython.sh [DEST] [SOURCE]
#   SOURCE: a git URL or a local clone (default: GitHub); the commit is fetched
#   from it, so a local clone that has the commit works before GitHub does.
#   CORE_SOURCE: a local clone of huc6280-python that has the core's commit, to
#   install the core from while the pyproject's GitHub pin is not pushed yet.
set -euo pipefail
COMMIT=3318027e5c34f1e211515f95233a0853938a5f53  # the clean rewrite (after v0.2.0)
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
    # --force-reinstall: the core's version number may not change between commits
    "$DEST/.venv-pypy/bin/python" -m pip install -q --force-reinstall --no-deps "git+file://$CORE@$CORE_COMMIT"
    "$DEST/.venv-pypy/bin/python" -m pip install -q --no-deps -e "$DEST"
else
    "$DEST/.venv-pypy/bin/python" -m pip install -q -e "$DEST"
fi
echo "pcengine-python $(git -C "$DEST" rev-parse --short HEAD) in $DEST"
