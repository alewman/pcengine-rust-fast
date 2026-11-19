#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Frames per second of the reference on a card, nothing logged or digested (run on the
reference's interpreter). The counterpart of ``pce-bench``.

    bench.py IMAGE [--frames N] [--machine pce|tg16] [--press BUTTONS@FRAME+HOLD ...]

Prints the whole run and its second half alone (PyPy's JIT has warmed up by then).
"""

from __future__ import annotations

import argparse
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import pce_trace  # noqa: E402

from pcengine_python import pads  # noqa: E402
from pcengine_python.board import Board  # noqa: E402
from pcengine_python.cards import make_card  # noqa: E402
from pcengine_python.machine import PC_ENGINE, TURBOGRAFX_16  # noqa: E402
from pcengine_python.system import build  # noqa: E402


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("image")
    p.add_argument("--frames", type=int, default=300)
    p.add_argument("--machine", choices=("pce", "tg16"), default="pce")
    p.add_argument("--press", action="append", default=[])
    a = p.parse_args()
    presses = [pce_trace.parse_press(spec) for spec in a.press]
    pad = pads.Pad2()
    machine = TURBOGRAFX_16 if a.machine == "tg16" else PC_ENGINE
    system = build(Board(machine, card=make_card(Path(a.image).read_bytes()), pad=pad))
    began = time.perf_counter()
    half = None
    for n in range(a.frames):
        if n == a.frames // 2:
            half = time.perf_counter()
        pad.pressed = pce_trace.held(presses, system.frame)
        system.run_frame()
        system.board.take_audio()
    end = time.perf_counter()
    print(
        f"{a.frames} frames in {end - began:.3f} s: {a.frames / (end - began):.2f} frames per second; "
        f"the second half {(a.frames - a.frames // 2) / (end - half):.2f} (mclk {system.board.mclk})"
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
