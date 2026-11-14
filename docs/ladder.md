# The ladder in detail

The README's "How it is proven" has the summary; this is the fuller account of
the gate and of rungs 3 to 6, as recorded by the session that built and
certified the board (2026-11-12). The whole gate was re-run from the
reference cache on 2026-11-13 with the same result.

## The gate

`scripts/lockstep.py` runs each case of `scripts/cases.tsv`
on both boards: `scripts/pce_trace.py` on the reference's interpreter and
`target/release/pce-trace` here write one line a frame with the frame
counter, the master clock, the count and SHA-1 of the CPU bus log (the tap of
the reference's own `scripts/state_roundtrip.py`: `address << 8 | value`,
`mclk << 2 | kind` for every cycle), the SHA-1 of the picture
(`capture.last_frame`: the rows at the 9-bit value, then the dividers), the
count and SHA-1 of the mixed audio (`board.take_audio()`) and every field of
the CPU's state. A case passes when the two files are identical. Otherwise
the first differing frame is named, and both boards are run again with
`--detail` on it, which narrows the difference to the first differing bus
cycle. The reference's traces are kept (`--reference-cache
external/refcache`, named by the case and a hash of its frames, image and
arguments), so a rerun costs the Rust side only: the whole gate of 275
cases then takes about half a minute on 16 cores (about 25 minutes of 16 PyPy processes without the cache). Archives are extracted one at a time
into `scratch/` and removed; the library is only read.

**In all, 84,200 frames in 275 cases, and no case has ever differed**: the
transcription was identical to the reference on the first run of each. (The
per-cycle narrowing has therefore only been exercised by the seeded faults.)

## Rungs 3 and 4

US cards run on the TurboGrafx-16 row, the rest on the PC
Engine row. Rung 3's titles are the reference's picture gate: the first ten
of the library (Aero Blasters, After Burner II, Air Zonk, Alien Crush, Andre
Panza Kick Boxing, Armed F, Atomic Robo-Kid Special, Ballistix, Barbarian
for the Amiga 500 and for the Amstrad CPC), the four sprite-heavy games
(Blazing Lazers, R-Type, Bonk's Adventure, Samurai-Ghost) and the four
outliers (Barbarian for the ZX Spectrum, Death Sword for the PC, Mai Nurse,
Gunboat), each 600 frames with RUN at frame 250, I at 400 and right with II
from 480; and the PSG games (Aero Blasters, Alien Crush, Blazing Lazers,
Bonk's Adventure, Samurai-Ghost), 900 frames with two RUN presses and two
seconds of I and right. Rung 4's smoke list (`scripts/smoke.txt`) is the
nine cards the reference's docs/m5-record.md lists as different from Mesen2
(Santatlantean, Tiger Road, Bomberman, After Burner II, Davis Cup Tennis,
Galaga '90, Battle Royale, Super Metal Crusher, Night Creatures), the mapper
cards (Populous; Street Fighter II', also with a 6-button pad), the multitap
users (Bomberman '94 behind a multitap, Dungeon Explorer) and classics
(Blazing Lazers, Bonk's Adventure, R-Type, Gradius, Devil's Crush,
Splatterhouse). The Tennokoe card is not in the library; its mapper runs in
lockstep on the probe card built as a `TennokoeCard`, and in the unit tests.

## Rung 5

The reference's own socket fakes (`tests/fake_devices.py`) record
calls and answer constants; they are transcribed (`src/fake_devices.rs`) and
drive the transcribed unit tests (`tests/sockets.rs`). For the lockstep,
three more devices are defined twice, line for line, in
`src/fake_devices.rs` and in `scripts/pce_trace.py`, and a game runs on each:

| Device | Sockets | What it does | Cases |
| --- | --- | --- | --- |
| `LockstepCard` | 1, 3, 4 | a ROM card that also claims banks `$F8-$FB`, so the work RAM is the card's, and that holds the CPU 3 master clocks on every 4096th write | Bonk's Adventure, the probe |
| `LockstepPort` | 2, 3, 5, 6, 9 | claims `$FF:1800-18FF`, is present (the I/O port's D7), raises IRQ2 by itself for 700 master clocks every 300,000, owns a 32 kHz audio source with a gain the mixer applies | Blazing Lazers, the probe |
| `LockstepPath` | 3, 7, 8, 12 | on a row with two VDCs: claims the VDC pages, sends `ST0`-`ST2` writes to both VDCs and other writes to the first (by `pins.opcode`), acknowledges both, lets the second VDC's pixel win where it is not zero | R-Type and the probe, with a power-on seed |
| `Pad6`, `MultiTap` (both readings of C10), no pad, a constant pad | 11 | the reference's own | Street Fighter II', Bomberman '94, the probe |

The machine rows are crossed (Blazing Lazers on the PC Engine row, where it
shows its region screen; Gradius and the probe on the TurboGrafx-16 row).
Each named switch is put away from its default on the probe card and on a
game: `power_on_seed`, `start_line`, `start_offset`, `vblank_dots` (`None`
and 0), `vertical_at_line_start` (with both vblank positions),
`dma_resumes`, `underflow_window_mclk` (0 and 1), `lines_follow_vce_bit2`,
the PSG's thirteen switches one at a time and its `mame` and `mednafen`
variants.

**A switch that changes nothing proves nothing**: if the reference's trace
with the switch is its trace without, both boards agree whether or not the
Rust board reads the switch. `scripts/switch_effect.py` compares each case's
reference trace with the same card's default trace and fails if a switch or
device has no case it changes. The library's first seconds do not reach
most of them (the LFO, DDA with the channel off, noise frequency 0, a DMA
running into the display period, the timer's reload window), so
`scripts/make_probe.py` builds a synthetic 8 KiB card, this repository's own
code, that does: 37 of 37 switches and devices change the probe's trace or a
game's.

## Rung 6

As the reference's `scripts/state_roundtrip.py`: each of the 18
titles is saved 148 to 2,477 instructions into frame 300 (anywhere in a
frame, between two instructions, with pad input before and after), the state
loaded into a fresh system and into one that had run 150 frames, and the 60
frames after compared by the gate's own lines; the loaded system saved at
once gives the same bytes. States are 10 to 35 KiB. Then the snapshot pair:
120 frames, after each a snapshot, two frames run ahead with other buttons
held, the snapshot put back; every frame's line and the end state equal the
plain run's. A snapshot is 247 KiB (5 MHz titles) to 302 KiB (7 MHz).
`tests/state.rs` has the round trip on a synthetic card with a device in
every socket, the mappers' registers and RAM, and the refusals: bytes that
are not a state, another version, another machine row, another card (and
the same image behind another mapper, and an empty slot), a port device or
a pad the state has not, a second VDC; a refused state leaves the system
untouched.
