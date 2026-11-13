#!/usr/bin/env python3
"""A/B two builds of pce-bench on this shared machine: alternate them, title by title, on one
pinned CPU, and compare each side's best run.

    ab.py BIN_A BIN_B [--rounds N] [--frames N] [--cpu N] [--only REGEX]
    ab.py --fps BIN [...]          # one build: frames per second a title

Each round runs A then B (then B then A the next round) on each title; the result is B's best
speed over A's best, per title and overall (the geometric mean). The best, not the median: on
this shared machine other work only ever slows a run down (a busy SMT sibling, shared cache),
so each side's fastest of many short runs is the closest to its own speed. pce-bench times by
the thread's CPU time, not the wall clock. The checksums (every picture, every audio sample,
the master clock) must agree, or the builds do not compute the same thing and the comparison
is refused. An A/A run (the same binary twice) shows the noise.

The five titles are extracted once into scratch/bench/ (the library is only read).
"""

from __future__ import annotations

import argparse
import re
import statistics
import subprocess
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
sys.path.insert(0, str(HERE))
from make_cases import DEFAULTS  # noqa: E402

LIB = DEFAULTS["LIB"]
PLAY = ["--press", "run@250+10", "--press", "i@400+6", "--press", "right-ii@480+40"]
# name, image, arguments: the picture gate's input (RUN at 250, I at 400, right with II from 480)
TITLES = [
    ("Blazing Lazers", f"{LIB}/Blazing Lazers (USA).7z", ["--machine", "tg16", *PLAY]),
    ("Bonk's Adventure", f"{LIB}/Bonk's Adventure (USA).7z", ["--machine", "tg16", *PLAY]),
    ("R-Type", f"{LIB}/R-Type (USA).7z", ["--machine", "tg16", *PLAY]),
    ("After Burner II", f"{LIB}/After Burner II (Japan) (En).7z", PLAY),
    ("Street Fighter II'", DEFAULTS["SF2"], PLAY),
]
LINE = re.compile(r"([\d.]+) frames/s, check ([0-9A-F]{8})")


def image(name: str, archive: str) -> Path:
    out = ROOT / "scratch" / "bench" / (re.sub(r"[^A-Za-z0-9]+", "_", name) + ".pce")
    if not out.is_file():
        out.parent.mkdir(parents=True, exist_ok=True)
        with out.open("wb") as handle:
            subprocess.run(["7z", "e", "-so", archive], stdout=handle, stderr=subprocess.DEVNULL, check=True)
    return out


def bench(binary: str, card: Path, args: list[str], frames: int, cpu: int) -> tuple[float, str]:
    out = subprocess.run(
        ["nice", "taskset", "-c", str(cpu), binary, str(card), "--frames", str(frames), *args],
        capture_output=True, text=True, check=True,
    ).stdout  # fmt: skip
    fps, check = LINE.search(out).groups()
    return float(fps), check


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("a")
    parser.add_argument("b", nargs="?")
    parser.add_argument("--fps", action="store_true", help="one build: its best frames per second")
    parser.add_argument("--rounds", type=int, default=12)
    parser.add_argument("--frames", type=int, default=600)
    parser.add_argument("--cpu", type=int, default=31)
    parser.add_argument("--only")
    parser.add_argument("--extra", default="", help="more pce-bench arguments for every title (--device lockstep-port)")
    args = parser.parse_args()
    titles = [(n, a, [*x, *args.extra.split()]) for n, a, x in TITLES if not args.only or re.search(args.only, n)]
    if args.fps or args.b is None:
        for name, archive, extra in titles:
            card = image(name, archive)
            runs = [bench(args.a, card, extra, args.frames, args.cpu) for _ in range(args.rounds)]
            print(f"{name}: {max(f for f, _ in runs):.1f} fps (check {runs[0][1]})", flush=True)
        return 0
    ratios = []
    for name, archive, extra in titles:
        card = image(name, archive)
        a_runs, b_runs = [], []
        for r in range(args.rounds):
            order = [("a", args.a), ("b", args.b)] if r % 2 == 0 else [("b", args.b), ("a", args.a)]
            for side, binary in order:
                (a_runs if side == "a" else b_runs).append(bench(binary, card, extra, args.frames, args.cpu))
        checks = {c for _, c in a_runs} | {c for _, c in b_runs}
        if len(checks) != 1:
            print(f"{name}: checksums differ {sorted(checks)}: the builds are not equivalent")
            return 1
        a = max(f for f, _ in a_runs)
        b = max(f for f, _ in b_runs)
        ratios.append(b / a)
        print(f"{name}: A {a:.1f} fps, B {b:.1f} fps, x{b / a:.3f}", flush=True)
    geo = statistics.geometric_mean(ratios)
    print(f"overall x{geo:.3f} (geometric mean of {len(ratios)} titles, {args.rounds} rounds each)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
