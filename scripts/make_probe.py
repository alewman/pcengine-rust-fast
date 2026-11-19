#!/usr/bin/env python3
# SPDX-License-Identifier: MIT
"""Build scratch/probe.pce: a synthetic 8 KiB HuCard that reaches what the library's first
seconds do not, so the lockstep gate has a case on which each named switch and each socket
changes what the board does.

Everything here is this repository's own: a few hundred bytes of HuC6280 code assembled by the
helpers below and data from a fixed generator. It is not a ROM of anything and is not
committed (scratch/ is ignored); the gate rebuilds it, byte for byte the same, when a case
names ``$PROBE``.

What it does, by socket and switch:

- **VDC**: display registers through ST0-ST2, tiles, a BAT and a sprite table through block
  transfers; 18 sprites on one line (overflow), sprite 0 over sprite 1 (collision), raster and
  vblank interrupts whose status reads are in the bus log; both scroll registers written all
  the time from the main loop (so a write lands on every dot of a line sooner or later, the
  latch dots among them); a VRAM-to-VRAM DMA read back while it runs, and another left running
  into the display period (``dma_resumes``); a display height of more than 8 bits.
- **VCE**: the three dot clocks and bit 2 (262 or 263 lines: ``lines_follow_vce_bit2``),
  palette writes and reads.
- **Timer**: latch 0, enabled, its interrupt acknowledged, and `$0C00` polled in the main
  loop (``underflow_window_mclk``).
- **PSG**: every disputed point of pcengine-python's docs/m4-record.md in turn (key-on, R6
  while playing, the DDA latch with the channel off, an attenuation of exactly 30 units, the
  R1/R5 nibble 8, noise frequency 0, noise selected late, the LFO with frequency 0, a
  modulation that leaves 12 bits, channel 2 off under the LFO, LF TRG with LF CTL 0).
- **Pad port**: a five-port multitap scan with a read while CLR is high, twice a frame (the
  6-button pad's alternating scans).
- **Card**: writes and reads in banks $40 and $7F (the Tennokoe card's RAM and lock), work
  RAM and stack traffic (a card that claims the RAM).
- **Expansion port**: `$FF:1800` read and written, IRQ2 enabled with a handler.
"""

from __future__ import annotations

import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
ORG = 0xE000


class Asm:
    def __init__(self) -> None:
        self.code = bytearray()

    @property
    def here(self) -> int:
        return ORG + len(self.code)

    def b(self, *values: int) -> None:
        self.code.extend(values)

    def w(self, opcode: int, address: int) -> None:
        self.b(opcode, address & 0xFF, address >> 8)

    # the instructions the probe uses
    def lda(self, value: int) -> None:
        self.b(0xA9, value)

    def sta(self, address: int) -> None:
        self.w(0x8D, address)

    def stz(self, address: int) -> None:
        self.w(0x9C, address)

    def lda_abs(self, address: int) -> None:
        self.w(0xAD, address)

    def store(self, address: int, value: int) -> None:
        if value == 0:
            self.stz(address)
        else:
            self.lda(value)
            self.sta(address)

    def tam(self, mask: int, bank: int) -> None:
        self.lda(bank)
        self.b(0x53, mask)

    def jsr(self, address: int) -> None:
        self.w(0x20, address)

    def vdc(self, register: int, value: int) -> None:
        """A VDC register through ST0, ST1, ST2."""
        self.b(0x03, register, 0x13, value & 0xFF, 0x23, value >> 8)

    def vdc_sta(self, register: int, value: int) -> None:
        """A VDC register through STA (not the ST instructions)."""
        self.store(0x0000, register)
        self.store(0x0002, value & 0xFF)
        self.store(0x0003, value >> 8)

    def tia(self, source: int, destination: int, length: int) -> None:
        self.b(0xE3, source & 0xFF, source >> 8, destination & 0xFF, destination >> 8,
               length & 0xFF, length >> 8)  # fmt: skip

    def psg(self, register: int, value: int) -> None:
        self.store(0x0800 + register, value)


def generator(seed: int):
    state = seed
    while True:
        state = (state * 1103515245 + 12345) & 0x7FFFFFFF
        yield (state >> 16) & 0xFF


def words(values: list[int]) -> bytes:
    return b"".join(bytes((v & 0xFF, v >> 8)) for v in values)


def build() -> bytes:
    a = Asm()
    rnd = generator(6280)

    # -- data ---------------------------------------------------------------------------------
    tiles = a.here
    a.code.extend(next(rnd) for _ in range(8 * 32))  # 8 tiles of 16 words
    bat = a.here
    a.code.extend(words([((i * 5 // 7) % 16) << 12 | 0x100 | (i * 3 + i // 64) % 8 for i in range(64 * 6)]))
    pattern = a.here
    a.code.extend(next(rnd) | 0x11 for _ in range(128))  # one 16x16 sprite pattern, four planes
    sat = a.here
    entries = []
    for i in range(64):
        if i < 18:  # eighteen on one line: the 17th overflows; sprite 0 overlaps sprite 1
            y, x = 64 + 60, 32 + (8 if i == 1 else i * 13)
            flags = (i & 0xF) | (0x80 if i % 3 == 0 else 0) | (0x800 if i % 4 == 1 else 0)
        elif i < 24:  # larger shapes further down
            y, x = 64 + 100 + (i - 18) * 4, 32 + (i - 18) * 40
            flags = (i & 0xF) | 0x100 | ((i - 18) % 4) << 12 | (0x8000 if i % 2 else 0)
        else:
            y, x, flags = 0, 0, 0
        entries += [y, x, 0x80 << 1, flags]
    a.code.extend(words(entries))
    palette = a.here
    a.code.extend(words([(i * 73 + 21) & 0x1FF for i in range(48)]))

    # -- subroutines and handlers -------------------------------------------------------------
    pads = a.here
    a.store(0x1000, 1)
    a.store(0x1000, 3)  # CLR high with SEL high: the multitap's counter resets
    a.lda_abs(0x1000)  # C10: what D3-D0 show while CLR is high
    a.store(0x1000, 2)  # CLR stays high: not another pulse for a 6-button pad
    a.store(0x1000, 3)  # SEL rises under CLR: the multitap must not count it
    a.lda_abs(0x1000)
    for _ in range(5):
        a.store(0x1000, 1)
        a.lda_abs(0x1000)
        a.stz(0x1000)
        a.lda_abs(0x1000)
    a.b(0x60)  # RTS

    wait = a.here  # until the vblank handler has counted a frame; the timer is polled meanwhile
    a.lda_abs(0x2000)
    loop = a.here
    a.w(0xAE, 0x0C00)  # LDX $0C00
    a.w(0xAE, 0x1403)  # LDX $1403
    a.w(0xEE, 0x2004)  # INC $2004: a changing value into both scroll registers, at any dot
    a.w(0xAC, 0x2004)  # LDY $2004
    a.b(0x03, 0x08)  # ST0 #8 (BYR)
    a.w(0x8C, 0x0002)  # STY $0002
    a.b(0x03, 0x07)  # ST0 #7 (BXR)
    a.w(0x8C, 0x0002)  # STY $0002
    a.w(0xCD, 0x2000)  # CMP $2000
    a.b(0xF0, (loop - (a.here + 2)) & 0xFF)  # BEQ loop
    a.jsr(pads)
    a.jsr(pads)
    a.b(0x60)

    irq1 = a.here
    a.b(0x48)  # PHA
    a.lda_abs(0x0000)  # the status: acknowledges
    a.sta(0x2001)
    a.b(0x29, 0x20)  # AND #$20: the vertical blank
    a.b(0xF0, 3)  # BEQ +3
    a.w(0xEE, 0x2000)  # INC $2000
    a.b(0x68, 0x40)  # PLA, RTI

    tiq = a.here
    a.b(0x48)
    a.sta(0x1403)  # any write acknowledges the timer
    a.w(0xEE, 0x2002)
    a.b(0x68, 0x40)

    irq2 = a.here
    a.b(0x48)
    a.lda_abs(0x1800)
    a.b(0x1A)  # INC A
    a.sta(0x1800)
    a.w(0xEE, 0x2003)
    a.b(0x68, 0x40)

    nmi = a.here
    a.b(0x40)

    # -- reset --------------------------------------------------------------------------------
    reset = a.here
    a.b(0x78, 0xD4, 0xD8)  # SEI, CSH, CLD
    a.b(0xA2, 0xFF, 0x9A)  # LDX #$FF, TXS
    a.tam(0x01, 0xFF)  # MPR0: the hardware page at $0000
    a.tam(0x02, 0xF8)  # MPR1: work RAM at $2000 (zero page, stack)
    a.tam(0x04, 0x40)  # MPR2: bank $40 at $4000
    a.tam(0x08, 0x7F)  # MPR3: bank $7F at $6000
    for address in (0x2000, 0x2001, 0x2002, 0x2003, 0x2004):
        a.stz(address)
    a.stz(0x0400)  # VCE: 5 MHz, bit 2 clear
    a.stz(0x0402)
    a.stz(0x0403)
    a.tia(palette, 0x0404, 64)  # background palettes 0 and 1
    a.stz(0x0402)
    a.store(0x0403, 1)
    a.tia(palette + 32, 0x0404, 64)  # sprite palettes
    for register, value in (
        (0x05, 0x0000), (0x06, 0x0040 + 100), (0x07, 3), (0x08, 5), (0x09, 0x0010),
        (0x0A, 0x0202), (0x0B, 0x041F), (0x0C, 0x0F02), (0x0D, 0x00EF), (0x0E, 0x0003),
        (0x0F, 0x0013), (0x13, 0x0800),
    ):  # fmt: skip
        a.vdc(register, value)
    a.vdc(0x00, 0x1000)
    a.b(0x03, 0x02)  # ST0 #2: the data register
    a.tia(tiles, 0x0002, 8 * 32)
    a.vdc(0x00, 0x0000)
    a.b(0x03, 0x02)
    a.tia(bat, 0x0002, 64 * 6 * 2)
    a.vdc(0x00, 0x2000)
    a.b(0x03, 0x02)
    a.tia(pattern, 0x0002, 128)
    a.vdc(0x00, 0x0800)
    a.b(0x03, 0x02)
    a.tia(sat, 0x0002, 512)
    a.vdc(0x00, 0x1040)  # a few words through ST1/ST2 as well
    a.b(0x03, 0x02)
    for value in (0x1234, 0xFFFF, 0x0F0F, 0x8001):
        a.b(0x13, value & 0xFF, 0x23, value >> 8)
    a.stz(0x1402)  # all three interrupts enabled
    a.stz(0x0C00)  # timer latch 0
    a.store(0x0C01, 1)
    a.vdc(0x05, 0x00CF)  # both layers, all four interrupt enables
    a.b(0x58)  # CLI

    # -- the script: a group of writes every few frames, then again ----------------------------
    main = a.here

    def frames(count: int) -> None:
        for _ in range(count):
            a.jsr(wait)

    frames(2)
    # the PSG: six channels, key-on (C22)
    a.psg(1, 0xFF)
    for channel in range(6):
        a.psg(0, channel)
        a.psg(4, 0x00)
        for i in range(32):
            sample = (i, 31 - i, (i * 7) % 32, 31 if i < 16 else 0, (i * 11 + 5) % 32, (i * 3) % 32)[channel]
            a.psg(6, sample)
        frequency = (0x120, 0x1C0, 0x0F0, 0x200, 0x060, 0x090)[channel]
        a.psg(2, frequency & 0xFF)
        a.psg(3, frequency >> 8)
        a.psg(5, 0xFF)
        a.psg(4, 0x9F if channel < 3 else 0x1F)
    # a VRAM-to-VRAM DMA longer than the blanking, and VRAM read back behind it
    a.vdc(0x10, 0x1000)
    a.vdc(0x11, 0x3000)
    a.vdc(0x12, 0x4000)
    a.vdc_sta(0x01, 0x1002)  # MARR: the read waits for the transfer
    a.lda_abs(0x0002)
    a.lda_abs(0x0003)
    a.lda_abs(0x0002)
    a.lda_abs(0x0003)
    frames(6)
    # a second DMA left alone until the display period has started: paused, or cancelled
    # (`dma_resumes`); the sprite pattern's words reach $7000 only if it goes on afterwards
    a.vdc(0x10, 0x1000)
    a.vdc(0x11, 0x6000)
    a.vdc(0x12, 0x1FFF)
    frames(1)
    a.vdc_sta(0x01, 0x7004)
    a.lda_abs(0x0002)
    a.lda_abs(0x0003)
    frames(2)
    a.vdc_sta(0x01, 0x3004)
    a.lda_abs(0x0002)
    a.lda_abs(0x0003)
    a.vdc_sta(0x00, 0x3100)
    a.store(0x0000, 0x02)
    a.store(0x0002, 0x55)
    a.store(0x0003, 0xAA)
    a.store(0x0002, 0x33)
    a.store(0x0003, 0xCC)  # a second write while the first waits for its slot
    # the card: banks $7F and $40 (Tennokoe's lock and RAM), the port: $FF:1800
    a.store(0x6000, 0x01)
    a.store(0x4005, 0x5A)
    a.lda_abs(0x4005)
    a.stz(0x6000)
    a.lda_abs(0x4005)
    a.store(0x6000, 0x7E)
    a.lda_abs(0x4005)
    a.lda_abs(0x1800)
    a.store(0x1800, 0x21)
    a.lda_abs(0x1800)
    frames(4)
    # C13, C16: R6 while a channel plays
    a.psg(0, 0)
    a.psg(6, 7)
    a.psg(6, 21)
    frames(6)
    # C16: the DDA latch written with the channel off, then DDA on
    a.psg(0, 2)
    a.psg(4, 0x40)
    a.psg(6, 25)
    a.psg(4, 0xDF)
    frames(6)
    a.psg(4, 0x9F)
    # C14: exactly 30 units of attenuation on channel 4
    a.psg(0, 3)
    a.psg(4, 0x81)
    frames(6)
    # C15: nibble 8 in R1 and R5
    a.psg(4, 0x9F)
    a.psg(5, 0x8F)
    a.psg(1, 0x88)
    frames(6)
    a.psg(1, 0xFF)
    # the VCE: 7 MHz and bit 2 (263 lines)
    a.store(0x0400, 0x05)
    a.stz(0x0402)
    a.stz(0x0403)
    a.lda_abs(0x0404)
    a.lda_abs(0x0405)
    a.store(0x0404, 0x9C)
    a.store(0x0405, 0x01)
    # C17, C20: noise frequency 0 on channel 5; channel 6 playing its wave, noise selected later
    a.psg(0, 4)
    a.psg(7, 0x9F)
    a.psg(4, 0x9F)
    a.psg(0, 5)
    a.psg(7, 0x00)
    a.psg(4, 0x9F)
    frames(8)
    a.psg(7, 0x85)
    frames(6)
    # C19: the LFO with frequency 0
    a.psg(8, 0x00)
    a.psg(9, 0x01)
    frames(8)
    # U6: a modulation that leaves 12 bits (channel 1 at frequency 8, shift 4)
    a.psg(0, 0)
    a.psg(2, 0x08)
    a.psg(3, 0x00)
    a.psg(8, 0x03)
    a.psg(9, 0x03)
    frames(4)
    a.psg(4, 0x1F)  # channel 1 keyed off and on under the LFO: its first period is modulated
    a.psg(4, 0x9F)
    frames(4)
    # C18: channel 2 off under the LFO; its wave rewritten while it is off, so the sample it
    # holds is not 16 (no modulation) whatever its index was when the key-off came, and it still
    # changes when it plays (C23 below)
    a.psg(0, 1)
    a.psg(4, 0x1F)
    for i in range(32):
        a.psg(6, 31 if i % 2 else 0)
    frames(6)
    # C23: LF TRG with LF CTL 0, channel 2 on
    a.psg(4, 0x9F)
    a.psg(9, 0x80)
    frames(6)
    a.psg(9, 0x00)
    a.psg(0, 0)
    a.psg(2, 0x20)
    a.psg(3, 0x01)
    # the VCE: 10 MHz, bit 2 clear again; the display narrower and moved
    a.store(0x0400, 0x02)
    a.vdc(0x0A, 0x0502)
    a.vdc(0x0B, 0x0627)
    a.vdc(0x07, 0x0041)
    a.vdc(0x08, 0x0023)
    a.vdc(0x0D, 0x01EF)  # a display height past 8 bits: the sync cuts it short
    frames(8)
    a.vdc(0x0D, 0x00EF)
    a.stz(0x0400)
    a.vdc(0x0A, 0x0202)
    a.vdc(0x0B, 0x041F)
    a.vdc(0x09, 0x0050)
    frames(4)
    a.vdc(0x09, 0x0010)
    a.w(0x4C, main)  # JMP main

    code = bytes(a.code)
    if len(code) > 0x2000 - 10:
        raise SystemExit(f"the probe is {len(code)} bytes: more than a bank")
    image = bytearray(code + bytes(0x2000 - len(code)))
    for offset, address in ((0x1FF6, irq2), (0x1FF8, irq1), (0x1FFA, tiq), (0x1FFC, nmi), (0x1FFE, reset)):
        image[offset] = address & 0xFF
        image[offset + 1] = address >> 8
    return bytes(image)


def main() -> int:
    out = Path(sys.argv[1]) if len(sys.argv) > 1 else ROOT / "scratch" / "probe.pce"
    out.parent.mkdir(parents=True, exist_ok=True)
    image = build()
    if not out.is_file() or out.read_bytes() != image:
        out.write_bytes(image)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
