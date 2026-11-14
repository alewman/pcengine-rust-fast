#!/usr/bin/env bash
# The reference, pcengine-python at v0.2.0, into external/ with a PyPy venv
# (PyPy runs it several times faster than CPython). Its CPU core,
# huc6280-python v0.4.0, comes with it (pinned in its pyproject).
# Usage: scripts/fetch_pcenginepython.sh [DEST] [SOURCE]
#   SOURCE: a git URL or a local clone (default: GitHub); the commit is fetched
#   from it, so a local clone that has the commit works before GitHub does.
#   CORE_SOURCE: a local clone of huc6280-python that has the core's commit, to
#   install the core from while the pyproject's GitHub pin is not pushed yet.
set -euo pipefail
COMMIT=4c32f26730eb2a95041ec62547f977f97d2ab696  # v0.2.0
CORE_COMMIT=265df883bbe1b3fe27077525786c57d580a1edfa  # huc6280-python v0.4.0
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
    "$DEST/.venv-pypy/bin/python" -m pip install -q "git+file://$CORE@$CORE_COMMIT"
    "$DEST/.venv-pypy/bin/python" -m pip install -q --no-deps -e "$DEST"
else
    "$DEST/.venv-pypy/bin/python" -m pip install -q -e "$DEST"
fi
echo "pcengine-python $(git -C "$DEST" rev-parse --short HEAD) in $DEST"
