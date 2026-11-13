#!/usr/bin/env python3
"""Fold pce-profile's samples into functions and source lines with addr2line.

    profile.py [external/profile.txt] [--lines N]

Each sample's address is resolved with its inline chain (``addr2line -i``): the innermost
frame is where the time went, the outer ones where it was called from. Prints the share of each
crate and module (self time), the functions by inclusive share, then the hottest lines.
"""

from __future__ import annotations

import collections
import re
import subprocess
import sys


def chains(exe: str, addresses: list[str]) -> dict[str, list[tuple[str, str]]]:
    """address -> [(function, file:line)], innermost first."""
    out = subprocess.run(
        ["addr2line", "-e", exe, "-f", "-i", "-C", "-a", *("0x" + a for a in addresses)],
        capture_output=True, text=True, check=True,
    ).stdout.splitlines()  # fmt: skip
    result: dict[str, list[tuple[str, str]]] = {}
    current: list[str] = []
    key = None
    for line in out:
        if line.startswith("0x"):
            if key is not None:
                result[key] = list(zip(current[0::2], current[1::2]))
            key, current = format(int(line, 16), "x"), []
        else:
            current.append(line)
    if key is not None:
        result[key] = list(zip(current[0::2], current[1::2]))
    return result


def module(where: str) -> str:
    where = where.split(" ")[0]
    if "/huc6280-rust/" in where:
        return "CPU core (huc6280-rust)"
    match = re.search(r"/pcengine-rust[^/]*/src/(.*?):", where)
    if match:
        return match.group(1)
    if "/rustc/" in where or "/library/" in where:
        return "std (" + where.rsplit("/", 1)[-1].split(":")[0] + ")"
    return where.rsplit("/", 1)[-1].split(":")[0] or "?"


def main() -> int:
    args = [a for a in sys.argv[1:] if not a.startswith("--")]
    lines_wanted = 40
    if "--lines" in sys.argv:
        lines_wanted = int(sys.argv[sys.argv.index("--lines") + 1])
        args = [a for a in args if a != str(lines_wanted)]
    path = args[0] if args else "external/profile.txt"
    exe, *addresses = open(path).read().split()
    counts = collections.Counter(addresses)
    resolved = chains(exe, list(counts))
    total = sum(counts.values())
    functions: collections.Counter = collections.Counter()
    where: collections.Counter = collections.Counter()
    modules: collections.Counter = collections.Counter()
    owners: collections.Counter = collections.Counter()
    for address, n in counts.items():
        pairs = resolved.get(address, [])
        seen = set()
        for function, _ in pairs:
            name = "::".join(function.split("::")[-2:]) if "::" in function else function
            if name not in seen:
                functions[name] += n
                seen.add(name)
        if pairs:
            where[pairs[0][1].split("/src/")[-1].split(" ")[0]] += n
            modules[module(pairs[0][1])] += n
            # the innermost frame that is this crate's or the core's: std time charged to its caller
            owner = next((module(w) for _, w in pairs if not module(w).startswith("std")), module(pairs[0][1]))
            owners[owner] += n
    print(f"{total} samples")
    print("\nmodules (self, std charged to the caller):")
    for name, n in owners.most_common(25):
        print(f"  {100 * n / total:5.1f}%  {name}")
    print("\nmodules (self):")
    for name, n in modules.most_common(25):
        print(f"  {100 * n / total:5.1f}%  {name}")
    print("\nfunctions (inclusive):")
    for name, n in functions.most_common(40):
        print(f"  {100 * n / total:5.1f}%  {name}")
    print("\nlines (self):")
    for line, n in where.most_common(lines_wanted):
        print(f"  {100 * n / total:5.1f}%  {line}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
