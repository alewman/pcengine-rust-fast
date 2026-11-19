<!-- SPDX-License-Identifier: MIT -->
# The clean rewrite of 2026-11-19, carried into pcengine-rust

Aubrey, 2026-11-19: "rewrite the modules that used GPL sources if we haven't already." The licence
audit of 2026-11-13 ([provenance.md](provenance.md)) marked parts of pcengine-python **rewrite**
(the VDC's line machine and parts of the VDC chip, which followed Mednafen's VDC code or took a
formula from Mesen2) or **unclear** (a few constants whose only stated source was GPL code that
had been read), and this crate, a line-by-line transcription, inherited those verdicts.
pcengine-python replaced them first (its `docs/restart.md`, at `3318027`, local, not pushed yet),
and its CPU core huc6280-python had its own clean rewrite (`6ce348e`), carried into huc6280-rust
(`e1d24a3`, local). This pass carries both here. Fixes start upstream: nothing in this crate's
behaviour was decided here.

## The rules this pass kept

1. **No GPL or LGPL source was read**: Mednafen, Beetle PCE, Mesen2, Geargrafx, the MiSTer core's
   `rtl/`, MAME's GPL files. Not to learn, not to check a constant, not when a comparison failed.
2. **The replaced Rust code was not read.** Before writing, the Rust counterparts of the
   withdrawn Python code were moved, unread, into the git-ignored `scratch/withdrawn/` by a small
   Python helper (`scratch/withdraw.py`, also ignored) that printed only names and line counts:
   - whole files: `src/vdc/sequencer.rs`, `src/vdc/dma.rs` (the reference rewrote most of
     `dma.py` and withdrew two of its functions and its constants: the whole file went);
   - items by name from `src/vdc/chip.rs`: the module documentation, `latch_byr`, `latch_bxr`,
     `step_bg_y`, `line_counter_step`, `commit_unless_dma`, `write`, `fetch_sprites`, and the
     fields (declarations and initialisers, with their comments) `bg_y`, `need_bg_y_step`,
     `bg_y_offset`, `bg_x_offset`, `mwr_cache`;
   - from `src/cpu_io/timer.rs`: the module documentation and the `underflow_window_mclk` field
     (declaration and initialiser: the withdrawn value 15);
   - from `src/machine.rs`: the module documentation and `HSYNC_CLOCKS`, `LINE_CLOCKS`,
     `VSYNC_LINES` with their comments (the provenance doc's "unclear": a quoted Mednafen line);
   - from `scripts/seeded_faults.py`: the seven faults whose text was in the withdrawn code
     (`byr-set-on-the-msb-only`, `byr-latched-a-dot-late`, `vds-a-line-short`,
     `burst-from-the-background-bit`, `sat-transfer-time-fixed`, `dma-cancelled-by-the-display`,
     `hsync-a-clock-short`), cut by the same helper because a fault holds a line of the code it
     breaks.
   They were not opened again. The untouched parts of those files, the call sites (`board.rs`,
   `sockets.rs`, `state.rs`, `trace.rs`) and the tests defined the interface.
3. **The reference's new code was read, never its deleted diff lines**: the new
   `vdc/sequencer.py`, `vdc/chip.py`, `vdc/dma.py`, `vdc/sprites.py`, and the added lines only of
   the other files that changed (`board.py`, `machine.py`, `cpu_io/timer.py`, `video.py`,
   `vdc/background.py`, `pyproject.toml`, the tests).
4. **Transcribed line by line**, as the rest of the crate: the same names (behind the `seq_` and
   `dma_` prefixes of vdc/mod.rs), the same order, the comments and evidence tags carried over
   ([mfr], [hw], [emu], FITTED, HYPOTHESIS, "a choice"). Where Rust forced a difference it is
   said in the file.
5. **Comments elsewhere reworded** to match: no GPL file or function names (`huc.cpp`,
   `sf2_threshold`, `vce.cpp`) and no quotes of Mednafen code; a behaviour observed in an
   emulator keeps the emulator's name. MAME's BSD-3-Clause files are named where they inform the
   code, with the notice in [NOTICE.md](../NOTICE.md).
6. **The judge is the reference**, as always for this tier: every case of the lockstep gate on
   both boards. How close the reference is to the hardware is the reference's record (its
   `docs/restart.md`: the picture gate, the library sweep, Covell's photographs, the Mednafen
   lockstep, before and after).

## What was replaced

| Item | Now (transcribed from pcengine-python `3318027`) |
| --- | --- |
| `src/vdc/sequencer.rs` (whole) | the new line machine: events `Step` (sync end, `HSYNC_CLOCKS` after the line start), `Latch` (`SCROLL_LATCH_DOTS` = 16 dots before the display start, never before the step), `Display` ((HDS + 1) x 8 dots after the step, dropped if at or past the next step), `Raster` (unused while `RASTER_COMPARE_CLOCKS` = 0), `Vblank`, `Collision`; the raster compare as the sync falls, against the count the previous step left; vertical periods VSW + 1, VDS + 2, VDW + 1, VCR + 3 (`FRONT_PORCH_EXTRA_LINES`) counted at steps, the vsync's own line counted without a period change; the raster counter set to 0 on the last VDS line; burst mode taken at every VSW entry; a display cut by the vsync gets its vblank on the next line |
| `src/vdc/chip.rs` scroll state | `bg_y_counter` loaded from BYR at the display period's start and on either BYR byte, +1 (9 bits) at every step (`line_begins`); `latch_scroll` takes BXR (10 bits) and the counter |
| `src/vdc/chip.rs` `write` | VWR's high byte waits then queues the word; MARR's high byte waits then queues the read; LENR's high byte starts the DMA; DVSSR's high byte arms the SAT transfer; either BYR byte reloads the counter. The board's wait (issue 0004) stops the access as `read` does (module notes) |
| `src/vdc/chip.rs` `fetch_sprites` | at the step, on display lines only, with the manual's searched/fetched budget (2.4.1 (6)); OR on a short budget |
| `src/vdc/chip.rs` `mwr_cache` | MWR bits 6-0 at each vblank (`vblank_begins`); CM live; the background and the sprite scan use `mwr_cache \| (mwr & 0x80)` |
| `src/vdc/chip.rs` `commit_unless_dma`, `latch_byr`, `latch_bxr`, `step_bg_y`, `line_counter_step` | gone, as in the reference |
| `src/vdc/dma.rs` (whole) | the SAT transfer at the vblank event (256 words copied, reads past VRAM give 0), busy for `SAT_CLOCKS` = 3 x 1365; no burst mode set at its end; VRAM -> VRAM at `ACCESS_DOTS[VM]` x divider per access, two accesses a word, VM as the last vblank took it; `dma_finish_without_display` for a VDC with no board (replaces `dma_force_complete`) |
| `src/vdc/sprites.rs` | `scan(..., limit, searched)`; `CLOCKS_PER_SPRITE` removed; the module doc's last paragraph |
| `src/machine.rs` | `HSYNC_CLOCKS`, `LINE_CLOCKS`, `VSYNC_LINES` cited from MAME huc6260.cpp's comments (BSD-3-Clause) |
| `src/cpu_io/timer.rs` | `underflow_window_mclk` 3 (one PPS3 clock: HYPOTHESIS, closed-box bounded below), was 15 |
| `src/board.rs` | `vertical_at_line_start` removed; `vblank_dots` default `Some(-16)`; `timer_underflow_window_mclk` default 3 |
| `src/state.rs` | format version 3 (the VDC's and the timer's layouts changed); versions 1 and 2 refused |
| comments | `video.rs` (dot 24, MAME huc6260.cpp), `vdc/background.rs`, `cards.rs`, `tests/cards.rs`: no GPL file or function names |

## The core: huc6280-rust e1d24a3

huc6280-rust's clean rewrite (from huc6280-python `6ce348e`): decimal `ADC`/`SBC` re-derived,
`VanishedInterrupt::Latched` and `IrqPollCycle::Mednafen` withdrawn, `CpuState::polled_irq_lines`
`None` after a boundary that decides an interrupt. The board uses neither withdrawn mode, so
nothing here changed for it; its defaults are cycle-identical. It is not on GitHub yet, so
`Cargo.toml` takes it by path from a sibling clone (`../huc6280-rust`, checked out at `e1d24a3`)
and names the git pin to use once it is pushed; CI clones it there by that revision.

## The gate, before and after

"Before" is pcengine-rust 0.2.0 against pcengine-python `v0.2.0` (2026-11-13, README); "after" is
this pass against `3318027` with the reference cache rebuilt from scratch (the old one moved aside
to `external/refcache-v0.2.0`).

| Check | Before | After |
| --- | --- | --- |
| Rung 1: Covell's test runs | 60 frames, master clock 21,538,338 | the same |
| Lockstep, all cases (rungs 2-5) | 275 of 275 identical | **275 of 275 identical on the first run** (84,200 frames) |
| Rung 5: switches that change a case | 37 of 37 | 37 of 37 (`vertical_at_line_start` gone; `vblank_dots` -18 and the timer window 15 added as cases) |
| Rung 6: save states | 18 of 18, run-ahead 18 of 18 | 18 of 18, run-ahead 18 of 18 |
| Rung 7: seeded faults | "56 caught, 0 missed" | **63 caught, 3 missed** ([seeded-faults.md](seeded-faults.md)): the 0.2.0 count was wrong (a broken test stage counted five faults as caught); the three missed are on the board's wait, not on the rewrite |
| `cargo test` | all pass | all pass; new tests: the DVSSR rule, the version-2 refusal, the poll's lines in a state, MWR's SM at vblank, the waited write's time |
| clippy `-D warnings` (rustc 1.93.1 and stable 1.99.1), fmt, wasm32 build | pass | pass |

## What stays open

Everything the reference's `docs/restart.md` lists as open stays open here (the raster compare's
place in the line, the scroll latch fit, the display start, the front porch, the SAT transfer's
length at 7 and 10 MHz, the timer window, the vblank fit -16 against -18), with the two library
regressions it records against the referees (Barbarian (Atari ST), Pac-Land). They are questions
about the hardware and belong upstream; this crate follows whatever the reference settles.
