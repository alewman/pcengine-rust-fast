#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Write scripts/cases.tsv, the lockstep gate's cases: name, frames, image, arguments.

The image column uses ``$LIB`` (the HuCard library, 179 ``.7z``), ``$COVELL`` (Covell's CPU
test), ``$SF2`` (the Street Fighter II' card, outside the library) and ``$PROBE`` (the
synthetic card scripts/make_probe.py builds); scripts/lockstep.py fills them from the
environment, with the defaults below. Nothing here is a ROM: the cases name files the user
brings.

    make_cases.py            # rewrite scripts/cases.tsv from the library's file names

Groups, by the name's prefix:

    covell      Covell's CPU test (rung 1's program, in lockstep)
    pic-        rung 3: the 18 picture-gate titles of pcengine-python docs/m3-record.md, 600
                frames with pad input
    psg-        rung 3: the five PSG games of docs/m4-record.md, 900 frames
    lib-        rung 4: every HuCard of the library, 300 frames from power-on with a RUN press
    map-        rung 4: the mapper cards outside the library
    base-       rung 5: the synthetic probe card with every default (scripts/make_probe.py)
    sock-       rung 5: a test device in each socket
    mach-       rung 5: the machine rows
    sw-         rung 5: each named switch away from its default
"""

from __future__ import annotations

import os
import re
from pathlib import Path

HERE = Path(__file__).resolve().parent

# No machine path here (the family's rule: romsets by environment variable). LIB (the directory
# of the 179 HuCard .7z images, No-Intro names) has no default; SF2 defaults to the card's No-Intro
# name under $NO_INTRO_DIR's PC Engine set; COVELL to where scripts/fetch_covell.sh puts the test.
_NO_INTRO_PCE = Path(os.environ.get("NO_INTRO_DIR", "NO_INTRO_DIR-unset")) / "NEC - PC Engine - TurboGrafx-16"
DEFAULTS = {
    "LIB": "LIB-unset",
    "COVELL": str(HERE.parent / "external" / "covell" / "CPU_Test_10.pce"),
    "SF2": str(_NO_INTRO_PCE / "Street Fighter II' - Champion Edition (Japan).zip"),
    "PROBE": str(HERE.parent / "scratch" / "probe.pce"),
}

# pcengine-python docs/m3-record.md: the first ten library titles, the four sprite-heavy games,
# the four U15 outliers that are plain PC Engine cards.
PICTURE = [
    "Aero Blasters (USA)",
    "After Burner II (Japan) (En)",
    "Air Zonk (USA)",
    "Alien Crush (USA)",
    "Andre Panza Kick Boxing (USA)",
    "Armed F (Japan) (En)",
    "Atomic Robo-Kid Special (Japan) (En)",
    "Ballistix (USA)",
    "Barbarian (World) (Amiga 500) (Aftermarket) (Unl)",
    "Barbarian (World) (Amstrad CPC) (Aftermarket) (Unl)",
    "Blazing Lazers (USA)",
    "R-Type (USA)",
    "Bonk's Adventure (USA)",
    "Samurai-Ghost (USA)",
    "Barbarian (World) (ZX Spectrum) (Aftermarket) (Unl)",
    "Death Sword (World) (PC) (Aftermarket) (Unl)",
    "Mai Nurse (World) (Aftermarket) (Unl)",
    "Gunboat (USA)",
]
# docs/m4-record.md
PSG = [
    "Aero Blasters (USA)",
    "Alien Crush (USA)",
    "Blazing Lazers (USA)",
    "Bonk's Adventure (USA)",
    "Samurai-Ghost (USA)",
]
# The smoke list (scripts/smoke.txt is written from it): the nine cards docs/m5-record.md lists
# as different from Mesen2, the mapper cards, multitap and 6-button users, classics.
SMOKE = [
    "lib-Santatlantean (World) (Aftermarket) (Unl)",
    "lib-Tiger Road (USA)",
    "lib-Bomberman (USA)",
    "lib-After Burner II (Japan) (En)",
    "lib-Davis Cup Tennis (USA)",
    "lib-Galaga '90 (USA)",
    "lib-Battle Royale (USA)",
    "lib-Super Metal Crusher (Japan) (En)",
    "lib-Night Creatures (USA)",
    "lib-Populous (Japan) (En)",
    "map-sf2",
    "sock-pad6-sf2",
    "sock-multitap-bomberman94",
    "lib-Dungeon Explorer (USA)",
    "lib-Blazing Lazers (USA)",
    "lib-Bonk's Adventure (USA)",
    "lib-R-Type (USA)",
    "lib-Gradius (Japan) (En)",
    "lib-Devil's Crush (USA)",
    "lib-Splatterhouse (USA)",
]

PSG_SWITCHES = [
    "write_wave_while_playing=true",
    "dda_latch_needs_on=true",
    "r6_overrides_sample_while_on=true",
    "keyon_mode=resume_step",
    "lfo_requires_ch2_on=false",
    "lfo_zero_multiplier=1",
    "lfo_mask12=false",
    "lf_trg_halts_always=false",
    "scale_table_quirk=true",
    "silent_units=31",
    "render_audio=false",
    "noise_zero_period=1",
    "noise_free_running=false",
]


def machine(title: str) -> str:
    return "--machine tg16" if "(USA)" in title else ""


def main() -> int:
    library = Path(os.environ.get("LIB", DEFAULTS["LIB"]))
    titles = sorted(p.stem for p in library.glob("*.7z"))
    rows: list[tuple[str, int, str, str]] = [("covell", 60, "$COVELL", "")]
    for title in PICTURE:
        press = "--press run@250+10 --press i@400+6 --press right-ii@480+40"
        rows.append((f"pic-{title}", 600, f"$LIB/{title}.7z", f"{machine(title)} {press}".strip()))
    for title in PSG:
        press = "--press run@200+10 --press run@420+10 --press i-right@600+120"
        rows.append((f"psg-{title}", 900, f"$LIB/{title}.7z", f"{machine(title)} {press}".strip()))
    for title in titles:
        rows.append(
            (f"lib-{title}", 300, f"$LIB/{title}.7z", f"{machine(title)} --press run@200+10".strip())
        )
    rows.append(("map-sf2", 300, "$SF2", "--press run@200+10"))
    # the probe card with everything at its default: what the rung 5 cases are measured against
    rows.append(("base-probe", 120, "$PROBE", ""))

    bonk = "$LIB/Bonk's Adventure (USA).7z"
    lazers = "$LIB/Blazing Lazers (USA).7z"
    rtype = "$LIB/R-Type (USA).7z"
    aero = "$LIB/Aero Blasters (USA).7z"
    run = "--press run@200+10"
    tg = "--machine tg16"
    # rung 5: the sockets
    rows += [
        ("sock-card-bonk", 300, bonk, f"{tg} --device lockstep-card {run}"),
        ("sock-card-probe", 120, "$PROBE", "--device lockstep-card"),
        ("sock-port-lazers", 300, lazers, f"{tg} --device lockstep-port {run}"),
        ("sock-port-probe", 120, "$PROBE", "--device lockstep-port"),
        ("sock-path-rtype", 300, rtype, f"{tg} --device lockstep-path --seed 7 {run}"),
        ("sock-path-probe", 120, "$PROBE", "--device lockstep-path --seed 7"),
        ("sock-pad6-sf2", 400, "$SF2",
         "--device pad6 --press run@200+10 --press iii-vi@260+30 --press run@300+10 --press iv-v-up@330+40"),
        ("sock-pad6-probe", 120, "$PROBE", "--device pad6 --press iii-v-up@20+60 --press run-vi@50+30"),
        ("sock-multitap-bomberman94", 400, "$LIB/Bomberman '94 (Japan).7z",
         "--device multitap --press run@200+10 --press run@280+10 --press i-left@330+40"),
        ("sock-multitap-probe", 120, "$PROBE", "--device multitap --press iii-v-up@20+60 --press run-ii@50+30"),
        ("sock-multitap-clr-low-probe", 120, "$PROBE",
         "--device multitap-clr-low --press iii-v-up@20+60 --press run-ii@50+30"),
        ("sock-no-pad-probe", 120, "$PROBE", "--device none"),
        ("sock-fake-controller-probe", 120, "$PROBE", "--device fake-controller"),
        ("sock-tennokoe-probe", 120, "$PROBE", "--device tennokoe"),
    ]
    # rung 5: the machine rows (a US card on the Japanese row and the other way)
    rows += [
        ("mach-pce-lazers", 300, lazers, run),
        ("mach-tg16-gradius", 300, "$LIB/Gradius (Japan) (En).7z", f"{tg} {run}"),
        ("mach-tg16-probe", 120, "$PROBE", tg),
    ]
    # rung 5: the named switches, each away from its default, on the probe card (which is
    # written to reach each one) and on a game
    board = [
        ("seed", "--seed 1234"),
        ("start-line", "--start-line 260"),
        ("start-offset", "--start-offset 500"),
        ("vblank-none", "--vblank-dots none"),
        ("vblank-0", "--vblank-dots 0"),
        ("vblank-18", "--vblank-dots -18"),  # the best fit to Covell's photographs
        ("no-dma-resume", "--no-dma-resume"),
        ("timer-window-0", "--timer-window 0"),
        ("timer-window-1", "--timer-window 1"),
        ("timer-window-15", "--timer-window 15"),  # the withdrawn default
        ("no-bit2-lines", "--no-bit2-lines"),
        ("psg-mame", "--psg-variant mame"),
        ("psg-mednafen", "--psg-variant mednafen"),
    ]
    board += [(f"psg-{s.split('=')[0]}", f"--psg {s}") for s in PSG_SWITCHES]
    for name, args in board:
        rows.append((f"sw-{name}-probe", 120, "$PROBE", args))
    on_bonk = [name for name, _ in board].index("psg-mame")
    for name, args in board[:on_bonk]:
        rows.append((f"sw-{name}-bonk", 300, bonk, f"{tg} {args} {run}"))
    for name, args in board[on_bonk:]:
        rows.append((f"sw-{name}-aero", 300, aero, f"{tg} {args} {run}"))
    rows.append(("sw-timer-window-0-battle-royale", 300, "$LIB/Battle Royale (USA).7z",
                 f"{tg} --timer-window 0 {run}"))  # fmt: skip

    names = [row[0] for row in rows]
    assert len(names) == len(set(names)), "case names must be unique"
    missing = [name for name in SMOKE if name not in names]
    assert not missing, missing
    out = ["# name\tframes\timage\targuments (written by scripts/make_cases.py)"]
    out += ["\t".join((name, str(frames), image, args)) for name, frames, image, args in rows]
    (HERE / "cases.tsv").write_text("\n".join(out) + "\n")
    (HERE / "smoke.txt").write_text("\n".join(SMOKE) + "\n")
    groups: dict[str, int] = {}
    for name in names:
        key = re.match(r"[a-z]+", name).group(0)
        groups[key] = groups.get(key, 0) + 1
    print(f"{len(rows)} cases: " + ", ".join(f"{k} {v}" for k, v in groups.items()))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
