#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Rung 6's driver: the save-state round trip and the run-ahead pass on the picture-gate titles.

For each ``pic-`` case of scripts/cases.tsv (the 18 titles of pcengine-python's
docs/m3-record.md) the card is extracted into scratch/ and ``pce-state-roundtrip`` runs it with
the case's machine row and pad input: saved a title-dependent number of instructions into frame
300, loaded into a fresh system and into one that had run, 60 frames compared by the lockstep
gate's own lines (bus log, picture, audio, CPU state); the loaded system saved at once must
give the same bytes; then 120 frames with a snapshot taken, two frames run ahead and the
snapshot put back after every frame.

    state_roundtrip.py [--only REGEX] [--jobs N] [--binary PATH]
"""

from __future__ import annotations

import argparse
import os
import re
import shutil
import subprocess
import sys
from concurrent.futures import ThreadPoolExecutor
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
sys.path.insert(0, str(HERE))
import lockstep  # noqa: E402


def one(numbered, binary: Path) -> tuple[str, bool, str]:
    index, (name, _, image, args) = numbered
    work = lockstep.SCRATCH / f"state-{lockstep.safe_name(name)}-{os.getpid()}"
    try:
        if not Path(image).is_file():
            return name, False, f"no image at {image}"
        card = lockstep.extracted(image, work)
        extra = 148 + 137 * index  # 148 to 2,477 instructions into frame 300
        done = lockstep.run(
            [str(binary), str(card), *args, "--at", "300", "--to", "360", "--extra", str(extra),
             "--ahead", "2", "--ahead-frames", "120"]
        )  # fmt: skip
        text = (done.stdout + done.stderr).strip()
        return name, done.returncode == 0, f"saved {extra} instructions into frame 300: {text}"
    finally:
        if work.is_dir():
            shutil.rmtree(work)


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--only", default="^pic-")
    parser.add_argument("--jobs", type=int, default=12)
    parser.add_argument("--binary", type=Path)
    args = parser.parse_args()
    binary = args.binary
    if binary is None:
        build = subprocess.run(["cargo", "build", "--release", "-q", "--manifest-path", str(ROOT / "Cargo.toml")])
        if build.returncode:
            return 1
        binary = ROOT / "target" / "release" / "pce-state-roundtrip"
    todo = [c for c in lockstep.cases(HERE / "cases.tsv") if re.search(args.only, c[0])]
    failed = 0
    with ThreadPoolExecutor(max(1, min(args.jobs, lockstep.MAX_JOBS))) as pool:
        for name, ok, text in pool.map(lambda c: one(c, binary), enumerate(todo)):
            print(f"{name}: {text}", flush=True)
            failed += not ok
    print(f"{len(todo) - failed} of {len(todo)} titles: a loaded run continues the same")
    return 1 if failed else 0


if __name__ == "__main__":
    sys.exit(main())
