#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""The lockstep gate's reference side: pcengine-python, traced frame by frame.

Run on the reference's interpreter (external/pcengine-python/.venv-pypy). Each line is one
``System.run_frame()``: the frame counter, the master clock, the count and SHA-1 of every CPU
bus cycle of the frame (the tap of the reference's ``scripts/state_roundtrip.py`` ``observe``:
``address << 8 | value`` then ``mclk << 2 | kind`` for a read or a write, ``mclk << 2 | 2`` for
an idle cycle, little-endian u64), the SHA-1 of the picture (``capture.last_frame``: rows as
little-endian u16, then the dividers), the count and SHA-1 of the mixed audio
(``board.take_audio()``, little-endian i16 pairs) and every field of the CPU's ``CPUState``.
``target/release/pce-trace`` writes the same lines; ``scripts/lockstep.py`` compares them.

    pce_trace.py IMAGE --frames N [--machine pce|tg16] [--press BUTTONS@FRAME+HOLD ...]
                 [--seed N] [--start-line N] [--start-offset N] [--vblank-dots N|none]
                 [--no-dma-resume] [--psg NAME=VALUE ...]
                 [--psg-variant NAME] [--timer-window N] [--no-bit2-lines] [--device NAME]
                 [--detail FRAME] --out FILE

``--device``: what is plugged into the sockets. ``pad2`` (default), ``pad6``, ``multitap``,
``multitap-clr-low``, ``none``, ``fake-controller``, ``tennokoe`` (the card built as a
``TennokoeCard``), and three test devices defined here and, line for line, in the Rust
board's ``src/fake_devices.rs``: ``lockstep-card``, ``lockstep-port``, ``lockstep-path``.
``--detail FRAME``: write only that frame's bus cycles, one a line.
"""

from __future__ import annotations

import argparse
import dataclasses
import hashlib
import struct
import sys
from array import array

from pcengine_python import pads
from pcengine_python.board import Board
from pcengine_python.cards import RomCard, TennokoeCard, make_card
from pcengine_python.cpu_io.timer import Timer
from pcengine_python.machine import HARDWARE_FIRST_PAGE, MASTER_HZ, PC_ENGINE, TURBOGRAFX_16
from pcengine_python.psg import VARIANTS, Psg
from pcengine_python.system import build

BUTTONS = {
    "up": pads.UP, "down": pads.DOWN, "left": pads.LEFT, "right": pads.RIGHT,
    "i": pads.I, "ii": pads.II, "select": pads.SELECT, "run": pads.RUN,
    "iii": pads.III, "iv": pads.IV, "v": pads.V, "vi": pads.VI,
}  # fmt: skip


def parse_press(spec: str) -> tuple[int, int, int]:
    buttons, _, when = spec.partition("@")
    start, _, hold = when.partition("+")
    mask = 0
    for name in buttons.lower().split("-"):
        mask |= BUTTONS[name]
    return mask, int(start), int(hold or 1)


def held(presses: list[tuple[int, int, int]], frame: int) -> int:
    mask = 0
    for buttons, start, hold in presses:
        if start <= frame < start + hold:
            mask |= buttons
    return mask


# --- the lockstep devices (src/fake_devices.rs has the same three) ---------------------------


class LockstepCard(RomCard):
    """A ROM card that also claims banks $F8-$FB, so the work RAM is the card's, and that holds
    the CPU for 3 master clocks on every 4096th write to it."""

    def __init__(self, data: bytes) -> None:
        super().__init__(data)
        self.ram = bytearray(0x2000)
        self.writes = 0
        self.pins = None

    def bind(self, pins) -> None:
        self.pins = pins

    def claims(self):
        return [(0x0000, 0x0FFF), (0x1F00, 0x1F7F)]

    def read(self, address: int, mclk: int) -> int:
        if address >= 0x1F0000:
            return self.ram[address & 0x1FFF]
        return super().read(address, mclk)

    def write(self, address: int, value: int, mclk: int) -> None:
        if address >= 0x1F0000:
            self.ram[address & 0x1FFF] = value
            self.writes += 1
            if self.writes % 4096 == 0:
                self.pins.hold(3)

    def reset(self, kind) -> None:
        self.writes = 0


class LockstepTone:
    """A source at 32 kHz with a gain the mixer applies, a fixed pattern."""

    rate_hz = 32000.0
    gain = 0.25

    def __init__(self) -> None:
        self.produced = 0

    def take_samples(self, mclk: int):
        total = mclk * 32000 // MASTER_HZ
        out = [((i * 37) % 2001 - 1000, (i * 53) % 1501 - 750) for i in range(self.produced, total)]
        self.produced = max(self.produced, total)
        return out


class LockstepPort:
    """A port device that claims $FF:1800-18FF (one register), is present, raises IRQ2 by
    itself for WIDTH master clocks every PERIOD, and owns an audio source."""

    PERIOD = 300_000
    WIDTH = 700
    present = True
    grounds_cd = False

    def __init__(self) -> None:
        self.register = 0
        self.tone = LockstepTone()
        self.pins = None

    def bind(self, pins) -> None:
        self.pins = pins

    def claims(self):
        return [(0x1FF8, 0x1FF8)]

    def read(self, address: int, mclk: int) -> int:
        return self.register

    def write(self, address: int, value: int, mclk: int) -> None:
        self.register = value

    def catch_up(self, mclk: int) -> None:
        level = mclk >= self.PERIOD and mclk % self.PERIOD < self.WIDTH
        self.pins.irq2.set("lockstep-port", level)

    def reset(self, kind) -> None:
        self.register = 0


ST_OPCODES = (0x03, 0x13, 0x23)


class LockstepPath:
    """On a machine row with two VDCs: the path claims the VDC pages; a write made by ST0-ST2
    goes to both VDCs, any other write to the first only; a read is the first VDC's, and a
    status read also reads the second's. The second VDC's pixel wins where it is not zero."""

    def __init__(self) -> None:
        self.lines_seen = 0
        self.board = None

    def mix(self, timing, rows):
        self.lines_seen += 1
        if len(rows) == 1:
            return rows[0]
        return [b if b else a for a, b in zip(rows[0], rows[1], strict=True)]

    def read(self, address: int, mclk: int) -> int:
        port = address & 3
        value = self.board.vdcs[0].read(port, mclk)
        if port == 0:
            self.board.vdcs[1].read(port, mclk)
        return value

    def write(self, address: int, value: int, mclk: int) -> None:
        port = address & 3
        self.board.vdcs[0].write(port, value, mclk)
        cpu = self.board.cpu
        if cpu is not None and cpu.opcode in ST_OPCODES:
            self.board.vdcs[1].write(port, value, mclk)


# --- building ----------------------------------------------------------------------------------


def psg_value(name: str, text: str):
    current = getattr(Psg, name)
    if isinstance(current, bool):
        return text.lower() in ("true", "1")
    if isinstance(current, int):
        return int(text)
    return text


def make(a: argparse.Namespace):
    """(system, the pads the presses go to)."""
    for name, value in VARIANTS[a.psg_variant].items() if a.psg_variant else ():
        setattr(Psg, name, value)
    for item in a.psg:
        name, _, text = item.partition("=")
        setattr(Psg, name, psg_value(name, text))
    Timer.underflow_window_mclk = a.timer_window
    machine = TURBOGRAFX_16 if a.machine == "tg16" else PC_ENGINE
    if not a.bit2_lines:
        machine = dataclasses.replace(machine, lines_follow_vce_bit2=False)
    with open(a.image, "rb") as handle:
        image = handle.read()
    device = a.device
    if device == "lockstep-card":
        card = LockstepCard(image)
    elif device == "tennokoe":
        card = TennokoeCard(image)
    else:
        card = make_card(image)
    pressed = []
    if device == "none":
        pad = None
    elif device == "pad6":
        pad = pads.Pad6()
        pressed = [pad]
    elif device == "fake-controller":

        class FakeController:
            def write(self, sel: bool, clr: bool) -> None:
                pass

            def read(self, sel: bool, clr: bool) -> int:
                return 0x05

        pad = FakeController()
    elif device in ("multitap", "multitap-clr-low"):
        ports = [pads.Pad2(), None, pads.Pad6(), None, pads.Pad2()]
        pad = pads.MultiTap(ports, clr_high_reads_pad=device == "multitap")
        pressed = [ports[0], ports[2]]
    else:
        pad = pads.Pad2()
        pressed = [pad]
    port = LockstepPort() if device == "lockstep-port" else None
    path = LockstepPath() if device == "lockstep-path" else None
    if path is not None:
        machine = dataclasses.replace(machine, vdc_count=2)
    board = Board(
        machine,
        card=card,
        port=port,
        pad=pad,
        pixel_path=path,
        power_on_seed=a.seed,
        start_line=a.start_line,
        start_offset=a.start_offset,
        vblank_dots=None if a.vblank_dots == "none" else int(a.vblank_dots),
        dma_resumes=a.dma_resume,
    )
    if port is not None:
        board.audio_sources.append(port.tone)
    if path is not None:
        path.board = board
        board.pages.claim(HARDWARE_FIRST_PAGE, HARDWARE_FIRST_PAGE + 3, path)
    return board, pressed


def observe(board, detail: list | None):
    """The reference's own tap (scripts/state_roundtrip.py), plus the per-cycle list."""
    log = array("Q")
    append = log.append
    read, write, idle = board.read_byte, board.write_byte, board.idle

    def read_byte(address: int) -> int:
        value = read(address)
        append(address << 8 | value)
        append(board.mclk << 2)
        if detail is not None:
            detail.append(f"r {address:06x} {value:02x} {board.mclk}")
        return value

    def write_byte(address: int, value: int) -> None:
        write(address, value)
        append(address << 8 | value)
        append(board.mclk << 2 | 1)
        if detail is not None:
            detail.append(f"w {address:06x} {value:02x} {board.mclk}")

    def idle_cycle(*args):
        out = idle(*args)
        append(board.mclk << 2 | 2)
        if detail is not None:
            detail.append(f"i {board.mclk}")
        return out

    board.read_byte, board.write_byte, board.idle = read_byte, write_byte, idle_cycle
    return log


def option(value) -> str:
    return "-" if value is None else str(int(value))


def cpu_text(s) -> str:
    mpr = "".join(f"{b:02x}" for b in s.mpr)
    return (
        f"{s.a:02x},{s.x:02x},{s.y:02x},{s.s:02x},{s.pc:04x},{s.p:02x},{mpr},"
        f"{int(s.high_speed)},{s.mpr_latch:02x},{int(s.irq1)}{int(s.irq2)}{int(s.tiq)},"
        f"{s.irq_disable},{int(s.halted)}{int(s.reset_pending)}"
        f"{int(s.maskable_interrupt_pending)}{int(s.non_maskable_interrupt_pending)},"
        f"{option(s.polled_i)},{option(s.polled_maskable_interrupt)},"
        f"{option(s.polled_non_maskable_interrupt)},{int(s.skipped_poll)},"
        f"{'-' if s.polled_irq_lines is None else s.polled_irq_lines}"
    )


def frame_line(system, log: array) -> str:
    board = system.board
    cpu = cpu_text(system.cpu.capture_state())
    bus = hashlib.sha1(log.tobytes()).hexdigest()
    cycles = len(log)
    del log[:]
    frame = board.capture.last_frame
    if frame is None:
        picture = "-"
    else:
        out = hashlib.sha1()
        for row in frame.rows:
            out.update(struct.pack(f"<{len(row)}H", *row))
        out.update(bytes(frame.dividers))
        picture = f"{frame.number}:{out.hexdigest()}"
    samples = board.take_audio()
    audio = hashlib.sha1()
    for left, right in samples:
        audio.update(struct.pack("<hh", left, right))
    return (
        f"{system.frame} mclk={board.mclk} bus={cycles}:{bus} pic={picture} "
        f"audio={len(samples)}:{audio.hexdigest()} cpu={cpu}"
    )


def main() -> int:
    p = argparse.ArgumentParser(description=__doc__.splitlines()[0])
    p.add_argument("image")
    p.add_argument("--frames", type=int, default=60)
    p.add_argument("--machine", choices=("pce", "tg16"), default="pce")
    p.add_argument("--press", action="append", default=[])
    p.add_argument("--seed", type=int)
    p.add_argument("--start-line", type=int, default=0)
    p.add_argument("--start-offset", type=int, default=0)
    p.add_argument("--vblank-dots", default="-16")
    p.add_argument("--no-dma-resume", dest="dma_resume", action="store_false")
    p.add_argument("--psg", action="append", default=[], metavar="NAME=VALUE")
    p.add_argument("--psg-variant", choices=sorted(VARIANTS))
    p.add_argument("--timer-window", type=int, default=3)
    p.add_argument("--no-bit2-lines", dest="bit2_lines", action="store_false")
    p.add_argument("--device", default="pad2")
    p.add_argument("--detail", type=int)
    p.add_argument("--out", required=True)
    a = p.parse_args()
    presses = [parse_press(spec) for spec in a.press]
    board, pressed = make(a)
    cycles: list[str] = []
    log = observe(board, cycles if a.detail is not None else None)
    system = build(board)
    with open(a.out, "w") as out:
        for _ in range(a.frames):
            mask = held(presses, system.frame)
            for pad in pressed:
                pad.pressed = mask
            wanted = a.detail is not None and a.detail == system.frame + 1
            del cycles[:]
            system.run_frame()
            line = frame_line(system, log)
            if a.detail is None:
                out.write(line + "\n")
            elif wanted:
                out.write("\n".join(cycles) + "\n")
                break
    return 0


if __name__ == "__main__":
    sys.exit(main())
