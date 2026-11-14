# Handoff brief: pcengine-rust

> **This is the brief the board was built from (2026-11-12), kept as written**, with "What was
> built" appended at the end by the session that built it. Its instructions ("do not push", the
> working rules, the local paths under `/data/emu/` and `~/.local/opt/`, which are the author's
> machine) are history, not instructions to a reader. The [README](../README.md) is the current
> account; where the two differ, the README is right (for example, the PSG's level curve is now a
> written-out table, `LEVEL_BITS`, which resolves the wasm caveat the build left).

## The request

Aubrey, 2026-11-12, after the PC Engine family's Python release: "get our rust version and rust
fast and the play-pce site ready." The order (megadrive, a2600 and famicom precedent):

1. huc6280-rust: the CPU core (`/data/emu/huc6280-rust`, transcribed from huc6280-python v0.3.0).
2. **pcengine-rust** (this repository): the board.
3. supergrafx-rust and pcenginecd-rust: the add-ons, transcribed from supergrafx-python and
   pcenginecd-python v0.1.0, plugging into this board's socket traits.
4. The ladder-gated speed forks, with the SDL2 window (`pce-play`).
5. play-pce: the fast boards in the browser (wasm), like play-2600 and play-nes.

**Aubrey, 2026-11-12: play-pce is to run any game of the PC Engine family**: HuCards, SuperGrafx
cards, CD-ROM², Super CD-ROM² and Arcade Card discs. So the add-ons are not optional later work:
the socket traits of this board are what they will be built on next, and they must be complete and
exercised here (rung 5 below drives each socket with a test device, as the Python board's
`tests/fake_devices.py` does).

## What it is

A Rust PC Engine / TurboGrafx-16 board **transcribed from**
[pcengine-python](https://github.com/alewman/pcengine-python) at **`v0.1.0`**
(`bc9b976f8b6ba29ff16fdcbd527978899d7e54b4`; local clone `/data/emu/pcengine-python`) and proven
equivalent to it: every CPU bus cycle (physical address, value, kind, master clock), every frame's
picture at the 9-bit palette value, every mixed audio sample. pcengine-python is the specification:
readable, cited, judged against Mesen2, Beetle PCE, Mednafen and Covell's photographs; this board
adds nothing and tunes nothing.

The CPU is `/data/emu/huc6280-rust` (crate `huc6280`): depend on it by path while it is local
(`huc6280 = { path = "../huc6280-rust" }`), and say so in the README; the session that started you
repins it to a GitHub revision once both are pushed. Read its README's "for a board author"
section for the `Bus` trait, the interrupt lines and `high_speed`.

Crate `pcengine_rust`, package `pcengine-rust`, edition 2021, MIT, `publish = false`.

## Read before writing any Rust, in this order

1. pcengine-python `README.md`, `docs/start-here.md` (the machines, the clocks, the module shape,
   save states), `docs/sockets.md` (the twelve sockets add-ons plug into).
2. `docs/memory-map.md`, `docs/vdc.md`, `docs/vce.md`, `docs/psg.md`, `docs/cartridge.md`.
3. The milestone records `docs/m1-record.md` to `docs/m5-record.md` for what each named switch
   (the U and C entries) is and why its default is what it is.
4. `src/pcengine_python/` module by module: `machine.py`, `sockets.py`, `pagetable.py`,
   `hardware_page.py`, `board.py`, `system.py`, `cpu_io/`, `vce.py`, `video.py`, `vdc/` (regs,
   chip, sequencer, background, sprites, dma), `psg/chip.py`, `mixer.py`, `cards.py`, `pads.py`,
   `state.py`.
5. For the Rust shape of a board on a `sixfiveohtwo`-family core: `/data/emu/a2600-rust` (small)
   and `/data/emu/famicom-rust` (large, with `state.rs` save states), their `scripts/` (the
   lockstep gate, the reference trace script, `make_cases.py`, the seeded faults) and READMEs.

## What to transcribe

Module for module, with the same names, **every named switch as a field with the Python default**
(the class attributes and constructor arguments: `vblank_dots`, `vertical_at_line_start`,
`dma_resumes`, `power_on_seed`, `start_line`, the VDC's and PSG's U/C switches and `VARIANTS`, the
timer's `underflow_window_mclk`, ...), and the comments that cite a source carried over.

- The machine table (PC Engine, TurboGrafx-16; keep `vdc_count` so the SuperGrafx row can exist).
- The board: one timeline in master clocks, the bus callbacks that price each cycle, `hold`, the
  interrupt lines; the page table (8192 pages); the hardware page decode.
- `cpu_io`: timer, interrupt registers, I/O port, I/O buffer. The pads: 2-button, 6-button, multitap.
- The VCE, the frame timeline and the line capture; the VDC (all of `vdc/`); the PSG; the mixer.
- The cards: ROM with mirroring, Street Fighter II', Populous, Tennokoe; `make_card` detection.
- **The sockets as traits** (`Card`, `PortDevice`, `ControllerPort`, `PixelPath`, `AudioSource`,
  `IrqLine`, ...): the SuperGrafx and the CD unit are later crates that plug into them exactly as
  supergrafx-python and pcenginecd-python plug into the Python board. Do not design them away; do
  not build the add-ons here.
- **Save states** (`state.py`): `save_state`/`load_state` for the Rust board. The format is the
  Rust board's own (serde; see famicom-rust's `state.rs`), not the Python JSON: what must hold is
  the Python board's *property*, a saved and loaded run continues bit for bit, and its refusals
  (another card by its hash, another machine). Also a cheap in-memory snapshot pair
  (`save_snapshot`/`load_snapshot`, no hashing or compression) for run-ahead in the browser, as
  famicom-rust has.

Not transcribed: the Python scripts that drive Mesen2, Beetle PCE, MAME and Mednafen; the picture
comparisons against them. The Python board has already been judged against those; this board is
judged against the Python board.

## The ladder (do not reorder)

Each rung has `scripts/rungN.sh` and passes only when its output says so.

1. **Skeleton**: board + huc6280-rust + a plain ROM card; Covell's CPU test
   (`/data/emu/pcengine-python/tests/test_roms/CPU_Test/CPU_Test_10.pce`, fetched there, read in
   place) runs 60 frames.
2. **The lockstep gate, built first and kept as the arbiter**: `scripts/pce_trace.py` runs the
   Python board (PyPy, from a checkout at the pin under `external/`, fetched by
   `scripts/fetch_pcenginepython.sh`) and writes, per frame, digests of the CPU bus log (physical
   address, value, kind, master clock: pcengine-python's `scripts/state_roundtrip.py` `observe()`
   shows the tap), of the picture (`capture.last_frame` rows and dividers), of the mixed audio
   (`board.take_audio()`) and of the CPU state; a Rust binary `pce-trace` writes the same lines;
   `scripts/lockstep.py` runs `scripts/cases.tsv` and reports the first differing frame, then
   narrows to the first differing bus cycle with a per-cycle mode. A reference-trace cache
   (`--reference-cache external/refcache`) so a rerun costs only the Rust side, as a2600-rust has.
   Pad input per case in pcengine-python's press spelling (`run@400+10`, `scripts/presses.py`).
3. **Picture and sound**: the 18 picture-gate titles of pcengine-python `docs/m3-record.md`
   (27 pictures: the list with frame counts is in that record; US cards as TurboGrafx-16) and the
   five PSG games of `docs/m4-record.md`, identical picture for picture and sample for sample,
   600 frames each with pad input.
4. **The library**: every HuCard of `/data/emu/roms-retrobat/pcengine/` (179 `.7z`, read in place;
   extract one at a time into `scratch/`, remove after; BYOR, never commit), 300 frames from
   power-on plus a RUN press, in lockstep by frame digest. Smoke first (a curated 20: the 9 cards
   pcengine-python `docs/m5-record.md` lists as different from Mesen2, the mapper cards, the
   6-button and multitap users if any, and classics), the full 179 only when the smoke is clean.
   The Street Fighter II' card is read in place from the path in pcengine-python
   `docs/cartridge.md`.
5. **Machines, sockets and switches**: every socket driven by a test device transcribed from
   pcengine-python `tests/fake_devices.py` and its `tests/test_sockets.py` (a second VDC with a
   pixel path, a port device with an IRQ2 source and an audio source, a card with RAM), in
   lockstep with the same fake on the Python side; the TurboGrafx-16 row, a `power_on_seed`, and each named switch in
   its non-default position on at least one case (the gate proves the switch is wired, not that
   it is right).
6. **Save states**: the round trip of pcengine-python `scripts/state_roundtrip.py` on the Rust
   board (saved mid-frame, loaded into a fresh board and into one that had run, 60 frames equal by
   the same digests; the loaded board saved at once gives the same bytes) on the 18 titles, the
   refusals as unit tests, and the snapshot pair in a run-ahead pass (run a frame, snapshot, run
   n ahead, load: the end state equals the plain run's).
7. **Seeded faults**: at least 20 single-line faults across the modules, each caught by a rung; a
   fault that changes no behaviour is not a fault. Report which rung caught each.

Speed is not a goal here (the fast fork's job) but measure it: frames per second on three titles
against the Python board on PyPy.

CI (`.github/workflows/ci.yml`, small: main and pull requests, concurrency cancel): `cargo fmt
--check`, `cargo clippy --release --all-targets -- -D warnings`, `cargo test --release`, and what
of the ladder needs no ROM (rung 1 if Covell's test can be fetched by the Python repository's pin;
unit tests of state refusals and mappers on synthetic images). The lockstep gate needs the library
and runs by hand.

## Traps the Python board has already paid for

- **The board prices every cycle inside the bus callback** (12 master clocks at low speed, 3 at
  high, plus the VDC/VCE stretch and any `hold`); `step()`'s return is never added to the clock.
- **One master-clock timeline.** The VDC, VCE, timer and PSG catch up to the clock at an access;
  the order of catch-ups inside a cycle is what the lockstep with Mednafen settled. Keep it.
- **Python arithmetic.** Floor division and modulo of negatives, `round` (ties to even), floats
  in the mixer (`carry`, gains, `_clip`): transcribe the arithmetic exactly (a2600-rust's
  `py_floordiv` and `round_ties_even` are the precedent). Audio must be sample for sample.
- **Dict and attribute order do not exist in Rust**; where the Python walks a dict (the page
  table claims, the mixer's sources) keep the same order explicitly.
- **PyPy's 80-attribute limit** shaped some Python classes; do not copy the workaround, and do
  not "tidy" the Python side.
- The mixer's lanes are kept by source position (v0.1.0), and `_Lane` is a dataclass.

## Rules (Aubrey's)

- The Python board is the spec. If you think it is wrong, stop, show the divergence and its
  judge, and report it; it is fixed in pcengine-python, then repinned. Do not modify
  pcengine-python, huc6280-python, huc6280-rust or 6502-rust.
- **No GPL or LGPL code, copied or transcribed** (pcengine-python `docs/provenance.md`): the only
  sources you transcribe are pcengine-python and huc6280-python, which are ours and MIT. Do not
  open the emulator trees under any `reference/` directory for this work.
- Never commit ROMs, traces or BIOS. Runs must not write into the library folders.
- At most 16 processes, `nice`; PyPy at `~/.local/opt/pypy3.11-v7.3.20-linux64/bin/pypy3.11` with
  `PYPY_GC_NURSERY=1M`; kill by PID, never pkill. No `sed -i`, `rm -rf` or `ln -sfn` one-liners:
  small Python helpers. Temp files under `external/`, `target/` or `scratch/` (gitignored).
- Smoke before full sweeps.
- Commits locally at good points, trailer `Co-Authored-By: Claude Fable 5.2
  <noreply@anthropic.com>`. **Do not create a GitHub repository and do not push.**
- Report at the end: each rung with numbers, the faults table, the Rust-forced differences,
  speed, anything in the Python board that looked wrong, and what the fast fork and the browser
  page should know (the public API: building a board from card bytes, a frame's picture and
  palette, audio, pad input, save states and snapshots).

## What was built (2026-11-12)

Appended by the session that built it; the README is the fuller account. A later session can
pick up from here and the repository alone.

**State: all seven rungs pass.** Nothing was changed in pcengine-python, huc6280-python,
huc6280-rust or 6502-rust; no `reference/` tree was opened; nothing is pushed and no GitHub
repository exists.

| Rung | Result |
| --- | --- |
| 1 | Covell's CPU test runs 60 frames (mclk 21,538,338, a picture every frame) |
| 2 | the gate on Covell's test: identical (bus log, picture, audio, CPU state), first run |
| 3 | 23 of 23 identical: the 18 picture-gate titles x 600 frames with pad input, the five PSG games x 900 |
| 4 | smoke 20 of 20, then the library 179 of 179 and the Street Fighter II' card: identical, 300 frames each |
| 5 | 71 of 71 identical; `switch_effect.py`: 37 of 37 switches and devices change at least one of their cases |
| 6 | 18 of 18 titles: saved 148-2,477 instructions into frame 300, loaded fresh and into a run system, 60 frames equal, same bytes when saved again; run-ahead 120 frames x 2 ahead equal |
| 7 | 48 seeded faults caught, 0 missed (28 by rung 2 (Covell's test or the probe card), 13 by rung 5, 1 by the smoke list, 5 by the unit tests, 1 by rung 6) |

84,200 frames in 275 lockstep cases, none differing; the transcription was identical to the
reference on its first run of every case (no divergence was ever found, so the per-cycle
narrowing has only been exercised by the seeded faults).

**How it was built, where that differs from the brief.** The brief says to build the gate
before the chips. The whole of `src/pcengine_python` (about 3,400 lines) was transcribed in one
pass first, because Covell's test does not run 60 meaningful frames without the VDC's timing;
rung 1 and the gate (rung 2) were then brought up together, and every later rung is cases of
that gate. No chip was ever debugged outside it.

**What is where** (the README's "Layout" has the rest):

- `src/`: one file per reference module; `mt.rs` (Python's `random.Random`, for
  `power_on_seed`), `trace.rs` (the gate's lines, `Setup`, `roundtrip`, `runahead`),
  `fake_devices.rs` (the reference's test fakes and three devices a game can run on).
- `scripts/pce_trace.py` / `src/bin/pce-trace.rs`: the two sides of the gate;
  `scripts/lockstep.py`; `scripts/make_cases.py` writes `cases.tsv` and `smoke.txt`.
- `scripts/make_probe.py`: a synthetic 8 KiB card (our own code, built into `scratch/`, never
  committed) written so that every named switch and every socket changes something on it;
  `scripts/switch_effect.py` checks that from the reference's traces.
- `scripts/seeded_faults.py`, `scripts/state_roundtrip.py`, `scripts/bench.py`,
  `scripts/reference_notes.py`.
- `external/refcache/`: the reference's traces (ignored by git). They take about 25 minutes
  of 16 PyPy processes to make; with them the whole gate (275 cases) reruns in half a minute.

**Sockets, for the add-on crates** (README, "How an add-on crate plugs in"): every socket is a
trait in `src/sockets.rs`. The decisions an add-on author should know:

- A device gets `&mut BoardPins` with every call (IRQ2, `hold`, the VDCs, the opcode, the
  card), instead of the reference's `bind`.
- The SuperGrafx's VPC is one `PixelPath` that also claims the VDC pages and reaches
  `pins.vdcs` and `pins.opcode`; `LockstepPath` in `fake_devices.rs` is its skeleton.
- The CD unit is one `PortDevice` that answers its own `audio_sources()` (CD-DA, ADPCM) and
  reaches the System Card through `pins.card` and `Card::as_any_mut`; `LockstepPort` is its
  skeleton.
- Every socket trait carries `DeviceState`, so an add-on's devices and media enter a save
  state; `state_identity()` is what refuses another disc.
- `Board::reset` also resets the pixel path (the reference's SuperGrafx wrapper does that
  itself).

**Left for the session that started this**: repin `huc6280` from the path to a GitHub
revision once both are pushed (CI builds only then); decide whether the notes on the
reference (README; `scripts/reference_notes.py` shows two of them on the reference itself) go
to pcengine-python as issues.
