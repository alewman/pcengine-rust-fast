#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""The lockstep gate: run each case on both boards and compare the traces.

Each case of ``scripts/cases.tsv`` runs ``pce-trace`` (this board) and ``scripts/pce_trace.py``
on the reference's interpreter (PyPy, ``PYPY_GC_NURSERY=1M``), several at a time under ``nice``;
the two traces are compared line for line (a line is a frame: the CPU bus log, the picture, the
mixed audio and the CPU state, digested). A case passes when they are identical. Otherwise the
first differing frame is reported and both boards are run again with ``--detail`` on that frame,
which narrows it to the first differing bus cycle (when the bus logs agree, the line itself says
which of picture, audio and CPU state differs).

    lockstep.py [--only REGEX] [--smoke] [--jobs N] [--rust PATH] [--keep]
                [--reference-cache DIR] [--no-narrow]

The reference is slow (several seconds per emulated second), and its trace of a case never
changes while the pin does not: ``--reference-cache external/refcache`` keeps each one, named by
the case and a hash of its frames, image name and arguments, so a rerun costs the Rust side only.

An image in a ``.7z`` or ``.zip`` is extracted (``7z e -so``) into ``scratch/`` for the case and
removed afterwards; the library is only read. ``$LIB`` and the other case variables come from
the environment, with the defaults in scripts/make_cases.py. Stdlib only.
"""

from __future__ import annotations

import argparse
import hashlib
import os
import re
import shlex
import shutil
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
sys.path.insert(0, str(HERE))
from make_cases import DEFAULTS  # noqa: E402

PYTHON = ROOT / "external" / "pcengine-python" / ".venv-pypy" / "bin" / "python"
SCRATCH = ROOT / "scratch"
ENV = {**os.environ, "PYPY_GC_NURSERY": "1M"}
MAX_JOBS = 16

Case = tuple[str, int, str, list[str]]


def cases(path: Path) -> list[Case]:
    env = {**DEFAULTS, **{k: os.environ[k] for k in DEFAULTS if k in os.environ}}
    rows = []
    for line in path.read_text().splitlines():
        if not line or line.startswith("#"):
            continue
        name, frames, image, args = (line.split("\t") + [""])[:4]
        for key, value in env.items():
            image = image.replace(f"${key}", value)
        rows.append((name, int(frames), image, shlex.split(args)))
    return rows


def safe_name(name: str) -> str:
    return re.sub(r"[^A-Za-z0-9._-]+", "_", name)


def cache_name(case: Case) -> str:
    name, frames, image, args = case
    path = Path(image)
    what: tuple = (frames, path.name, args)
    if path.suffix.lower() not in (".7z", ".zip") and path.is_file():
        # a loose image (Covell's test, the probe card this repository builds): by content
        what += (hashlib.sha1(path.read_bytes()).hexdigest(),)
    key = hashlib.sha1(repr(what).encode()).hexdigest()[:10]
    return f"{safe_name(name)}-{key}.ref"


def extracted(image: str, work: Path) -> Path:
    """The card image as a file: itself, or the one member of its archive, in `work`."""
    path = Path(image)
    if path.suffix.lower() not in (".7z", ".zip"):
        return path
    work.mkdir(parents=True, exist_ok=True)
    out = work / (path.stem + ".pce")
    with out.open("wb") as handle:
        subprocess.run(["7z", "e", "-so", str(path)], stdout=handle, stderr=subprocess.DEVNULL, check=True)
    return out


def run(command: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run(["nice", *command], capture_output=True, text=True, env=ENV)


def first_difference(a: Path, b: Path) -> tuple[int, str, str] | None:
    with a.open() as left, b.open() as right:
        number = 0
        while True:
            x, y = left.readline(), right.readline()
            if x != y:
                return number, x.strip(), y.strip()
            if not x:
                return None
            number += 1


def fields(line: str) -> dict[str, str]:
    out = {}
    for item in line.split()[1:]:
        key, _, value = item.partition("=")
        out[key] = value
    return out


def narrow(case: Case, image: Path, rust: Path, traces: Path, frame: str) -> str:
    """The first differing bus cycle of `frame` (the frame counter a line starts with)."""
    name, frames, _, args = case
    ref, ours = traces / f"{safe_name(name)}.detail.ref", traces / f"{safe_name(name)}.detail.rust"
    common = [str(image), "--frames", str(frames), *args, "--detail", frame]
    a = run([str(PYTHON), str(HERE / "pce_trace.py"), *common, "--out", str(ref)])
    b = run([str(rust), *common, "--out", str(ours)])
    if a.returncode or b.returncode:
        return "narrowing failed: " + (a.stderr or b.stderr).strip()[-300:]
    found = first_difference(ref, ours)
    if found is None:
        return "every bus cycle of that frame is the same on both"
    number, left, right = found
    before = ref.read_text().splitlines()[max(0, number - 3) : number]
    context = "".join(f"\n        {line}" for line in before)
    return (
        f"bus cycle {number} of that frame (r/w ADDRESS VALUE MCLK, i MCLK){context}\n"
        f"      reference {left!r}\n      rust      {right!r}"
    )


def one(case: Case, rust: Path, traces: Path, keep: bool, cache: Path | None, narrowing: bool):
    name, frames, image, args = case
    safe = safe_name(name)
    ref, ours = traces / f"{safe}.ref", traces / f"{safe}.rust"
    if cache is not None:
        ref = cache / cache_name(case)  # kept: the reference's trace does not change
    work = SCRATCH / f"case-{safe}-{os.getpid()}"
    began = time.monotonic()
    try:
        if not Path(image).is_file():
            return name, False, f"no image at {image}"
        card = extracted(image, work)
        common = [str(card), "--frames", str(frames), *args]
        if cache is None or not ref.is_file():
            partial = ref.with_suffix(".partial")
            reference = run([str(PYTHON), str(HERE / "pce_trace.py"), *common, "--out", str(partial)])
            if reference.returncode:
                partial.unlink(missing_ok=True)
                return name, False, "reference failed: " + reference.stderr.strip()[-400:]
            partial.replace(ref)
        board = run([str(rust), *common, "--out", str(ours)])
        if board.returncode:
            return name, False, "rust failed: " + board.stderr.strip()[-400:]
        seconds = time.monotonic() - began
        found = first_difference(ref, ours)
        if found is None:
            if not keep:
                if cache is None:
                    ref.unlink()
                ours.unlink()
            return name, True, f"identical, {frames} frames ({seconds:.0f} s)"
        number, left, right = found
        if not left or not right:
            return name, False, f"line {number}: one trace is longer"
        x, y = fields(left), fields(right)
        differing = [key for key in x if x.get(key) != y.get(key)]
        frame = left.split()[0]
        text = f"frame {frame} (line {number}) differs in {', '.join(differing)}"
        text += f"\n      reference {left!r}\n      rust      {right!r}"
        if narrowing and ("bus" in differing or "mclk" in differing):
            text += "\n      " + narrow(case, card, rust, traces, frame)
        return name, False, text
    finally:
        if work.is_dir():
            shutil.rmtree(work)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--only", help="a regular expression on the case name")
    parser.add_argument("--smoke", action="store_true", help="only the cases of scripts/smoke.txt")
    parser.add_argument("--cases", type=Path, default=HERE / "cases.tsv")
    parser.add_argument("--jobs", type=int, default=12)
    parser.add_argument("--rust", type=Path, help="a trace binary to judge instead of this tree's")
    parser.add_argument("--traces", type=Path, default=ROOT / "external" / "traces")
    parser.add_argument("--keep", action="store_true", help="keep the traces of identical cases")
    parser.add_argument("--reference-cache", type=Path,
                        help="keep the reference's traces here and reuse them (the pin does not move)")
    parser.add_argument("--no-narrow", action="store_true", help="do not rerun a failing frame per cycle")
    parser.add_argument("--quiet", action="store_true", help="print only failures and the count")
    args = parser.parse_args()
    rust = args.rust
    if rust is None:
        build = subprocess.run(["cargo", "build", "--release", "-q", "--manifest-path", str(ROOT / "Cargo.toml")])
        if build.returncode:
            return 1
        rust = ROOT / "target" / "release" / "pce-trace"
    if not PYTHON.exists() and not args.reference_cache:
        print(f"no reference at {PYTHON}: run scripts/fetch_pcenginepython.sh")
        return 2
    args.traces.mkdir(parents=True, exist_ok=True)
    if args.reference_cache:
        args.reference_cache.mkdir(parents=True, exist_ok=True)
    todo = [c for c in cases(args.cases) if not args.only or re.search(args.only, c[0])]
    if args.smoke:
        smoke = (HERE / "smoke.txt").read_text().split("\n")
        todo = [c for c in todo if c[0] in smoke]
    if any("$PROBE" in line for line in args.cases.read_text().splitlines()) and any(
        Path(c[2]).name == "probe.pce" for c in todo
    ):
        subprocess.run([sys.executable, str(HERE / "make_probe.py")], check=True)
    failed = 0
    jobs = max(1, min(args.jobs, MAX_JOBS))
    with ThreadPoolExecutor(jobs) as pool:
        work = lambda c: one(c, rust, args.traces, args.keep, args.reference_cache, not args.no_narrow)  # noqa: E731
        for name, ok, text in pool.map(work, todo):
            if not ok or not args.quiet:
                print(f"{name}: {text}", flush=True)
            failed += not ok
    print(f"{len(todo) - failed} of {len(todo)} cases identical")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
