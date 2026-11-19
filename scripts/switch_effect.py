#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Which rung 5 case does its switch or device actually change?

The lockstep gate proves a switch is wired only on a case where the switch changes what the
board does: if the reference's trace with the switch is the trace without it, both boards
would agree whether or not the Rust board reads the switch at all. This compares, for every
``sock-``, ``mach-`` and ``sw-`` case, the reference's trace (in the reference cache a gate run
left) with the trace of the same card with every default: ``base-probe`` for the probe card,
the ``lib-`` or ``map-`` case of the same image otherwise. It prints the first frame that
differs and in what (the bus log, the picture, the audio, the CPU state), and fails if a
switch or device has no case it changes.

    switch_effect.py [--reference-cache DIR]
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

HERE = Path(__file__).resolve().parent
ROOT = HERE.parent
sys.path.insert(0, str(HERE))
import lockstep  # noqa: E402


def lines(cache: Path, case) -> list[str] | None:
    path = cache / lockstep.cache_name(case)
    return path.read_text().splitlines() if path.is_file() else None


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--reference-cache", type=Path, default=ROOT / "external" / "refcache")
    args = parser.parse_args()
    cases = lockstep.cases(HERE / "cases.tsv")
    by_image: dict[str, tuple] = {}
    for case in cases:
        if re.match(r"(lib|map|base)-", case[0]):
            by_image[case[2]] = case
    groups: dict[str, list[tuple[str, str]]] = {}
    missing = 0
    for case in cases:
        name = case[0]
        if not re.match(r"(sock|mach|sw)-", name):
            continue
        group = name.rsplit("-", 1)[0] if not name.endswith("battle-royale") else name[: -len("-battle-royale")]
        base = by_image.get(case[2])
        ours, theirs = lines(args.reference_cache, case), base and lines(args.reference_cache, base)
        if ours is None or theirs is None:
            print(f"{name}: no reference trace (run the gate with --reference-cache first)")
            missing += 1
            continue
        effect = "no effect"
        for number, (a, b) in enumerate(zip(ours, theirs)):
            if a != b:
                x, y = lockstep.fields(a), lockstep.fields(b)
                what = [key for key in x if x[key] != y.get(key)]
                effect = f"differs from {base[0]} at line {number} in {', '.join(what) or 'the frame counter'}"
                break
        groups.setdefault(group, []).append((name, effect))
    dead = 0
    for group, found in groups.items():
        live = [name for name, effect in found if effect != "no effect"]
        for name, effect in found:
            print(f"{name}: {effect}")
        if not live:
            print(f"  ** {group}: no case on which it changes anything")
            dead += 1
    print(f"{len(groups) - dead} of {len(groups)} switches and devices change at least one of their cases")
    return 1 if dead or missing else 0


if __name__ == "__main__":
    sys.exit(main())
