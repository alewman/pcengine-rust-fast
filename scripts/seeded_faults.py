#!/usr/bin/env python3
"""A gate that cannot fail proves nothing: build the board with one fault at a time and check
that some rung catches each.

Each fault is one exact text replacement in a copy of ``src/`` (with ``tests/``), built into a
shared target directory under ``external/faults/``; then the checks run, cheapest first, until
one fails:

    rung 2      lockstep: Covell's CPU test and the probe card with every default
    rung 5      lockstep: the socket, machine and switch cases
    rung 4s     lockstep: the smoke list
    tests       cargo test (tests/*.rs and the unit tests)
    rung 3      lockstep: the picture and PSG titles
    rung 4      lockstep: the whole library
    rung 6      the save-state round trip and the run-ahead pass on the 18 titles

The reference's traces come from the cache a full gate run left (``--reference-cache``), so a
fault costs a build and the Rust side of the cases. A fault no check catches is reported as
MISSED.

    seeded_faults.py [--cache DIR] [--only REGEX] [--jobs N] [--list] [--check]
"""

from __future__ import annotations

import argparse
import re
import shutil
import subprocess
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]

# name: (file, exact text, replacement, what it changes)
FAULTS = {
    # board.rs
    "vce-access-not-stretched": (
        "board.rs",
        "if (HARDWARE_FIRST_PAGE..HARDWARE_FIRST_PAGE + 8).contains(&page) {",
        "if (HARDWARE_FIRST_PAGE..HARDWARE_FIRST_PAGE + 4).contains(&page) {",
        "only a VDC access takes the extra cycle, not a VCE one (C4, ares' reading)",
    ),
    "row-ends-a-dot-late": (
        "board.rs",
        "self.row_end = Some(at + HSYNC_CLOCKS);",
        "self.row_end = Some(at + HSYNC_CLOCKS + 4);",
        "a line's picture row is handed to the capture 4 master clocks late",
    ),
    "irq1-from-the-first-vdc-only": (
        "board.rs",
        "self.vdcs.iter().any(|vdc| vdc.irq1()),",
        "self.vdcs[0].irq1(),",
        "a second VDC's interrupt is not ORed into IRQ1 (the reference's issue 0001)",
    ),
    "port-device-not-caught-up": (
        "board.rs",
        "            port.catch_up(target, &mut pins);\n",
        "            let _ = (&port, &mut pins);\n",
        "the board never runs the port device up to its clock (the reference's issue 0002)",
    ),
    "disable-mask-not-delivered": (
        "board.rs",
        "            pins.set_irq_disable(self.cpu_io.irq.disable);\n",
        "",
        "a write to $1402 never reaches the CPU",
    ),
    "lines-delivered-only-on-advance": (
        "board.rs",
        "        // the access itself (a status read) may have changed a line\n        self.deliver_lines(Some(pins), false);\n",
        "",
        "an interrupt line an access changed is seen by the CPU a cycle later",
    ),
    # cpu_io
    "timer-prescaler-not-reset": (
        "cpu_io/timer.rs",
        "self.next_tick = mclk + TICK_CLOCKS;",
        "self.next_tick = mclk + TICK_CLOCKS - 3;",
        "the first tick after an enable comes a CPU cycle early (U24)",
    ),
    "timer-window-a-clock-long": (
        "cpu_io/timer.rs",
        "if mclk < self.window_end {",
        "if mclk <= self.window_end {",
        "the count reads $7F one master clock longer after an underflow (C25)",
    ),
    "irq-status-buffer-bits": (
        "cpu_io/irq_ctl.rs",
        "low | (buffer & 0xF8)",
        "low | (buffer & 0xF0)",
        "$1402/$1403 return only four bits of the I/O buffer",
    ),
    "io-port-d4-low": (
        "cpu_io/io_port.rs",
        "let mut value = 0x30 | nibble;",
        "let mut value = 0x20 | nibble;",
        "the I/O port's D4 reads 0",
    ),
    "read-does-not-load-the-buffer": (
        "cpu_io/mod.rs",
        "        self.buffer.value = value;\n        value\n",
        "        value\n",
        "a read of the timer, the port or the interrupt registers leaves the I/O buffer",
    ),
    # pads.rs
    "pad6-counts-clr-level": (
        "pads.rs",
        "if clr && !self.clr {\n            self.second_scan = !self.second_scan;",
        "if clr {\n            self.second_scan = !self.second_scan;",
        "the 6-button pad changes scan on every write with CLR high, not on its rise",
    ),
    "multitap-counts-under-clr": (
        "pads.rs",
        "} else if sel && !self.sel && !clr && self.counter < Self::PORTS {",
        "} else if sel && !self.sel && self.counter < Self::PORTS {",
        "the multitap's counter steps on a SEL rise while CLR is high",
    ),
    # cards.rs
    "card-384k-upper-mirror": (
        "cards.rs",
        ".map(|b| if b < 64 { b % 32 } else { 32 + (b % 16) })",
        ".map(|b| if b < 64 { b % 32 } else { 16 + (b % 32) })",
        "a 384 KiB card's banks $40-$7F show the wrong part of the image",
    ),
    "tennokoe-lock-inverted": (
        "cards.rs",
        "self.locked = value == 0;",
        "self.locked = value != 0;",
        "the Tennokoe card's RAM unlocks on a zero",
    ),
    "populous-ram-two-banks": (
        "cards.rs",
        "        if (0x40..=0x43).contains(&(address >> 13)) {\n            self.ram[(address & 0x7FFF) as usize] = value;",
        "        if (0x40..=0x41).contains(&(address >> 13)) {\n            self.ram[(address & 0x7FFF) as usize] = value;",
        "the Populous card's RAM takes writes in two banks of its four",
    ),
    # vce.rs, video.rs
    "palette-lsb-clears-bit-8": (
        "vce.rs",
        "4 => self.set_entry((self.palette[self.address as usize] & 0x100) | value, mclk),",
        "4 => self.set_entry(value, mclk),",
        "a write of a colour's low byte clears its ninth bit",
    ),
    "palette-write-a-dot-late": (
        "video.rs",
        "while pending < used && log[pending].0 <= when {",
        "while pending < used && log[pending].0 < when {",
        "a palette write made exactly at a dot's start shows from the next dot",
    ),
    "line-count-ignores-the-machine-row": (
        "video.rs",
        "if self.lines_follow_vce_bit2 && !vce.bit2() {",
        "if !vce.bit2() {",
        "`lines_follow_vce_bit2 = false` still follows VCE bit 2 (C3)",
    ),
    # vdc
    "cpu-slot-every-fourth-dot": (
        "vdc/chip.rs",
        "if slots.contains(&dot.rem_euclid(8)) {",
        "if slots.contains(&dot.rem_euclid(4)) {",
        "the CPU's VRAM slots repeat every 4 dots, not 8 (U1)",
    ),
    "raster-compare-a-line-early": (
        "vdc/chip.rs",
        "if count == self.regs.rcr() - 0x40 && self.regs.cr() & r::CR_RASTER_IRQ != 0 {",
        "if count == self.regs.rcr() - 0x41 && self.regs.cr() & r::CR_RASTER_IRQ != 0 {",
        "the raster compare fires a line early",
    ),
    "byr-set-on-the-msb-only": (
        "vdc/chip.rs",
        "} else if register == r::BYR {",
        "} else if register == r::BYR && msb {",
        "a write of BYR's low byte alone does not move the line counter (Mednafen's note)",
    ),
    "byr-latched-a-dot-late": (
        "vdc/sequencer.rs",
        "let to_byr = if hds <= 2 { 1 } else { (hds + 1) * 8 - 21 };",
        "let to_byr = if hds <= 2 { 2 } else { (hds + 1) * 8 - 21 };",
        "BYR is latched a dot later in the line (U3)",
    ),
    "vds-a-line-short": (
        "vdc/sequencer.rs",
        "VPhase::Vds => self.seq.vphase_counter = self.seq.vds + 2,",
        "VPhase::Vds => self.seq.vphase_counter = self.seq.vds + 1,",
        "the display period starts a line early",
    ),
    "burst-from-the-background-bit": (
        "vdc/sequencer.rs",
        "self.seq.burst_mode = self.regs.cr() & 0xC0 == 0;",
        "self.seq.burst_mode = self.regs.cr() & 0x80 == 0;",
        "burst mode is decided from the background enable alone",
    ),
    "sat-transfer-time-fixed": (
        "vdc/dma.rs",
        "self.dma.sat_end = Some(now + SAT_DOTS * self.seq.divider);",
        "self.dma.sat_end = Some(now + SAT_DOTS * 4);",
        "the SAT transfer takes 1024 dots of the 5 MHz clock whatever the dot clock",
    ),
    "dma-cancelled-by-the-display": (
        "vdc/dma.rs",
        "if self.dma.running && !self.seq.burst_mode && !self.dma.resume {",
        "if self.dma.running && !self.seq.burst_mode {",
        "a VRAM DMA running when the display starts is cancelled whatever `dma_resumes` says (C7)",
    ),
    "seventeen-sprites": (
        "vdc/sprites.rs",
        "            if entries.len() == SPRITE_LIMIT {",
        "            if entries.len() == SPRITE_LIMIT + 1 {",
        "a line holds 17 sprite entries before the overflow",
    ),
    "sprites-always-in-front": (
        "vdc/sprites.rs",
        "if row[target] & 0x0F == 0 || value & 0x200 != 0 {",
        "if value & 0x0F != 0 {",
        "a sprite pixel hides the background whatever its priority bit",
    ),
    "bat-height-from-the-width-bit": (
        "vdc/background.rs",
        "BAT_HEIGHTS[((mwr >> 6) & 1) as usize] - 1",
        "BAT_HEIGHTS[((mwr >> 5) & 1) as usize] - 1",
        "the BAT's height follows MWR D5",
    ),
    "vdw-eight-bits": (
        "vdc/regs.rs",
        "(self.value[VDW] & 0x1FF) as i64",
        "(self.value[VDW] & 0xFF) as i64",
        "the display height register is 8 bits wide",
    ),
    # psg
    "noise-frequency-not-complemented": (
        "psg/chip.rs",
        "let nf = 0x1F - (self.channels[ch].noise_control & 0x1F) as i64; // [mfr 2.3.3] NF is the complement",
        "let nf = (self.channels[ch].noise_control & 0x1F) as i64;",
        "the noise period is taken from NF, not its complement",
    ),
    "noise-feedback-bit": (
        "psg/chip.rs",
        "channel.lfsr = (channel.lfsr >> 1) | (bit << 17);",
        "channel.lfsr = (channel.lfsr >> 1) | (bit << 16);",
        "the noise shift register is 17 bits long",
    ),
    "lfo-shift": (
        "psg/chip.rs",
        "let shift = ((self.lfo_control & 3) as u32 - 1) * 2; // 0, 2, 4 [mfr 2.1.10]",
        "let shift = (self.lfo_control & 3) as u32 - 1;",
        "the LFO's modulation is shifted by 0, 1, 2 where a period is first loaded",
    ),
    "key-on-steps-the-address": (
        "psg/chip.rs",
        "if self.switches.keyon_mode == KeyOnMode::Reload {",
        "if self.switches.keyon_mode != KeyOnMode::Reload {",
        "key-on does what the other `keyon_mode` says (C22)",
    ),
    "dda-latch-needs-on": (
        "psg/chip.rs",
        "if channel.on() || !switches.dda_latch_needs_on {",
        "if channel.on() {",
        "R6 in DDA mode loads the latch only with the channel on (C16, Mednafen's way)",
    ),
    # mixer.rs
    "mixer-rounds-half-away": (
        "mixer.rs",
        "round_ties_even(value).clamp(-32768, 32767) as i16",
        "(value.round() as i64).clamp(-32768, 32767) as i16",
        "the mixer rounds a half away from zero (Rust's `round`), not to even (Python's)",
    ),
    "mixer-hold-rounds": (
        "mixer.rs",
        "let repeats = lane.carry as i64; // Python's int(): toward zero",
        "let repeats = lane.carry.round() as i64;",
        "a source at another rate is held by a rounded count",
    ),
    "mixer-gain-twice": (
        "mixer.rs",
        "            let gain = if source.applies_gain() {\n                1.0\n            } else {\n                source.gain()\n            };",
        "            let gain = source.gain() * if source.applies_gain() { 1.0 } else { 2.0 };",
        "a source that does not apply its own gain gets it doubled",
    ),
    # machine.rs, sockets.rs, pagetable.rs, hardware_page.rs
    "hsync-a-clock-short": (
        "machine.rs",
        "pub const HSYNC_CLOCKS: i64 = 237;",
        "pub const HSYNC_CLOCKS: i64 = 236;",
        "the horizontal sync is 236 master clocks",
    ),
    "irq-line-is-a-single-flag": (
        "sockets.rs",
        "            self.sources.remove(source);",
        "            self.sources.clear();",
        "a source letting go of an interrupt line lowers it for every source",
    ),
    "work-ram-half-size": (
        "pagetable.rs",
        "            mask: size - 1,",
        "            mask: (size - 1) >> 1,",
        "work RAM mirrors every 4 KiB",
    ),
    "interrupt-registers-unmapped": (
        "hardware_page.rs",
        "pages.set_base(first + 0x08, first + 0x17, Handler::CpuIo);",
        "pages.set_base(first + 0x08, first + 0x13, Handler::CpuIo);",
        "$FF:1400-17FF answers $FF",
    ),
    # state.rs, system.rs, mt.rs
    "state-forgets-the-capture": (
        "state.rs",
        "    board.capture = body.capture.into_owned();\n",
        "",
        "a loaded state keeps the picture rows of the system it is loaded into",
    ),
    "state-forgets-the-pad-latch": (
        "state.rs",
        "    board.cpu_io.port.latch = body.latch;\n",
        "",
        "a loaded state keeps the SEL/CLR latch of the system it is loaded into",
    ),
    "state-takes-the-timer-switch": (
        "state.rs",
        "    board.cpu_io.timer.underflow_window_mclk = window;\n",
        "",
        "a loaded state overwrites the system's own `underflow_window_mclk`",
    ),
    "no-reset-requested": (
        "system.rs",
        "        cpu.request_reset();\n",
        "",
        "the CPU is put on the board without a reset: no start sequence, no reset vector",
    ),
    "seed-low-bits": (
        "mt.rs",
        "self.next_u32() >> (32 - k)",
        "self.next_u32() & (u32::MAX >> (32 - k))",
        "a seeded power-on takes the low bits of each random word",
    ),
    # the fork's own shortcuts (the base board has none of these lines)
    "horizon-waits-for-the-underflow": (
        "board.rs",
        "if let Some(tick_at) = self.cpu_io.timer.next_tick() {",
        "if let Some(tick_at) = self.cpu_io.timer.next_event() {",
        "the event horizon stops for the timer's underflow only, so the count is behind the clock between",
    ),
    "horizon-sleeps-through-a-dma": (
        "board.rs",
        "            if vdc.dma.running {\n                return i64::MIN;\n            }\n",
        "",
        "the event horizon does not stop for a running VRAM DMA: its words move late",
    ),
    "access-keeps-the-old-horizon": (
        "board.rs",
        "        self.wake = i64::MIN;\n        if self.held != 0 {",
        "        if self.held != 0 {",
        "what an access schedules (a VRAM commit, the timer's start) is not in the event horizon",
    ),
    "next-event-forgets-the-commit": (
        "vdc/chip.rs",
        "            earlier(self.seq_next_event(), self.commit_event()),\n",
        "            self.seq_next_event(),\n",
        "a VDC's next event leaves out a waiting VRAM access's slot",
    ),
    "any-card-is-plain": (
        "board.rs",
        "            card.is::<RomCard>()\n",
        "            true\n",
        "a card that is not this crate's own (it may hold the CPU, raise IRQ2) has its accesses cut short too",
    ),
    "quiet-access-with-a-hold-waiting": (
        "board.rs",
        "if self.unsettled || (address >> PAGE_BITS) as usize >= HARDWARE_FIRST_PAGE {",
        "if (address >> PAGE_BITS) as usize >= HARDWARE_FIRST_PAGE {",
        "a work RAM or card access is cut short with a hold from outside waiting",
    ),
    "palette-write-waits-for-the-row-end": (
        "video.rs",
        "        let split = if used == 0 { &row[..0] } else { row };\n        if used == 0 {\n",
        "        let split = &row[..0];\n        {\n",
        "the capture's quick row is taken with palette writes inside the row: they show from the next row",
    ),
}

LOCKSTEP = [
    ("rung 2", ["--only", "^(covell|base-probe)$"]),
    ("rung 5", ["--only", "^(sock|mach|sw)-"]),
    ("rung 4 (smoke)", ["--smoke"]),
]
LOCKSTEP_LATE = [
    ("rung 3", ["--only", "^(pic|psg)-"]),
    ("rung 4", ["--only", "^(lib|map)-"]),
]


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    parser.add_argument("--cache", type=Path, default=ROOT / "external" / "refcache")
    parser.add_argument("--work", type=Path, default=ROOT / "external" / "faults")
    parser.add_argument("--only")
    parser.add_argument("--jobs", type=int, default=12)
    parser.add_argument("--list", action="store_true")
    parser.add_argument("--check", action="store_true", help="only check that every fault's text is there once")
    args = parser.parse_args()
    if args.check:
        bad = 0
        for name, (file, old, _, _) in FAULTS.items():
            count = (ROOT / "src" / file).read_text().count(old)
            if count != 1:
                print(f"{name}: the fault's text is in {file} {count} times, not once")
                bad += 1
        print(f"{len(FAULTS) - bad} of {len(FAULTS)} faults apply")
        return 1 if bad else 0
    if args.list:
        for name, (file, _, _, what) in FAULTS.items():
            print(f"{name}\t{file}\t{what}")
        return 0
    missed = ran = 0
    tree = args.work / "tree"
    target = args.work / "target"
    for name, (file, old, new, _) in FAULTS.items():
        if args.only and not re.fullmatch(args.only, name):
            continue
        ran += 1
        for part in ("src", "tests"):
            if (tree / part).exists():
                shutil.rmtree(tree / part)
            shutil.copytree(ROOT / part, tree / part)
        manifest = (ROOT / "Cargo.toml").read_text().replace(
            'path = "../huc6280-rust"', f'path = "{(ROOT.parent / "huc6280-rust").as_posix()}"'
        )
        (tree / "Cargo.toml").write_text(manifest)
        shutil.copy(ROOT / "Cargo.lock", tree / "Cargo.lock")
        source = tree / "src" / file
        text = (ROOT / "src" / file).read_text()
        if text.count(old) != 1:
            print(f"{name}: the fault's text is in {file} {text.count(old)} times, not once")
            return 2
        source.write_text(text.replace(old, new))
        cargo = ["nice", "cargo", "--quiet", "--manifest-path", str(tree / "Cargo.toml")]
        build = subprocess.run(
            [*cargo[:2], "build", "--release", *cargo[2:], "--target-dir", str(target)],
            capture_output=True, text=True,
        )  # fmt: skip
        if build.returncode:
            print(f"{name}: does not build\n{build.stderr[-800:]}")
            return 2
        binaries = target / "release"

        def lockstep(extra: list[str]) -> str | None:
            done = subprocess.run(
                [sys.executable, str(ROOT / "scripts" / "lockstep.py"), "--rust", str(binaries / "pce-trace"),
                 "--reference-cache", str(args.cache), "--traces", str(args.work / "traces"),
                 "--jobs", str(args.jobs), "--no-narrow", "--quiet", *extra],
                capture_output=True, text=True,
            )  # fmt: skip
            if done.returncode == 0:
                return None
            return next((line.split(":")[0] for line in done.stdout.splitlines()
                         if line and not line.startswith(" ") and ": " in line), "?")  # fmt: skip

        caught = None
        for label, extra in LOCKSTEP:
            first = lockstep(extra)
            if first is not None:
                caught = f"{label}: {first}"
                break
        if caught is None:
            tests = subprocess.run(
                [*cargo[:2], "test", "--release", *cargo[2:], "--target-dir", str(target)],
                capture_output=True, text=True,
            )  # fmt: skip
            if tests.returncode:
                # `cargo test --quiet` names a failed test under "failures:", indented
                out = tests.stdout.splitlines()
                failed = [line.strip() for n, line in enumerate(out)
                          if line.startswith("    ") and " " not in line.strip()
                          and "failures:" in out[:n]]  # fmt: skip
                caught = f"tests: {failed[0] if failed else '?'}"
        if caught is None:
            for label, extra in LOCKSTEP_LATE:
                first = lockstep(extra)
                if first is not None:
                    caught = f"{label}: {first}"
                    break
        if caught is None:
            states = subprocess.run(
                [sys.executable, str(ROOT / "scripts" / "state_roundtrip.py"), "--binary",
                 str(binaries / "pce-state-roundtrip"), "--jobs", str(args.jobs)],
                capture_output=True, text=True,
            )  # fmt: skip
            if states.returncode:
                first = next((line.split(":")[0] for line in states.stdout.splitlines() if "FAIL" in line), "?")
                caught = f"rung 6: {first}"
        print(f"{name}: {'caught by ' + caught if caught else 'MISSED'}", flush=True)
        missed += caught is None
    print(f"{ran - missed} caught, {missed} missed")
    return 1 if missed else 0


if __name__ == "__main__":
    raise SystemExit(main())
