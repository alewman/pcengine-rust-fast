<!-- SPDX-License-Identifier: MIT -->
# Changelog

Not on crates.io; pin a commit (README).

## Unreleased

### The early display start (pcengine-rust `37f17ba`, pcengine-python `03ac699`)

- **Merged pcengine-rust `37f17ba`** (local, not pushed yet): `render_line` crops a display that
  starts before the row (a small HDS puts the display start inside the sync, so `first` is
  negative). The fork panicked there on `first as usize`. The merge conflicted in `render_line`,
  because the fork draws into the row in place (`background::row_into`); the fix was transcribed
  into that shape: with no pad (every case so far) the row is drawn in place as before; with a
  pad, the line goes into a work row of `dots + pad` that is drawn in place the same way, the
  sprites draw into it, and the pad is split off, so the row keeps 1365 / divider dots. The base
  board's unit test `a_display_that_starts_before_the_row_is_cropped_and_the_row_keeps_its_length`
  came with the merge and passes.
- The gate's reference is pcengine-python `03ac699` (`scripts/fetch_pcenginepython.sh`, now with
  `--force-reinstall`); the reference cache key includes the reference's commit
  (`scripts/lockstep.py`, `cache_name(case, reference=None)`). The cache was rebuilt from scratch
  in this fork with the reference checked out under `external/pcengine-python`; the `3318027`
  cache was moved aside to `external/refcache-3318027`. The 275 rebuilt traces are byte-identical
  to pcengine-rust's `03ac699` cache.
- The seeder's fresh-mtime copy and the README text: the fork had both already (`8de227a`,
  `7c53f35`); the fork's versions were kept.
- **Gate** (2026-11-19, `LIB` set to the 179-card library): `cargo test` 69; lockstep **275 of
  275 identical**; save states and run-ahead 18 of 18; seeded faults **80 caught, 3 missed** (the
  same count as before); clippy `-D warnings` (debug and release) on rustc 1.93.1 and stable
  1.99.1, each with its own target directory and the rustup toolchain first on `PATH`; fmt;
  wasm32 build. Speed against `7c53f35`: see the README's "Speed".

### The clean rewrite (pcengine-rust `26ea621`)

The clean rewrite of 2026-11-19 ([docs/restart.md](docs/restart.md)): merges pcengine-rust
`26ea621` (pcengine-python `3318027`, local, not pushed yet) and moves the core to
huc6280-rust-fast `73f0829` (huc6280-rust `e1d24a3`, huc6280-python `6ce348e`; also local). The
fork's counterparts of the withdrawn code were moved aside unread first (`7a6a3f0`).

- **From the base board** (pcengine-rust's CHANGELOG, "Unreleased", has the details): the VDC's
  new line machine (`vdc/sequencer.rs`, whole; the picture's origin moves 8 dots earlier), the
  vertical scroll counter, `write`, `fetch_sprites` with the manual's budget, `mwr_cache` at
  vblank, the DMAs (`vdc/dma.rs`, whole; `dma_finish_without_display` replaces
  `dma_force_complete`), the timer window 3 (was 15), `vblank_dots` default `Some(-16)`,
  `HSYNC_CLOCKS`/`LINE_CLOCKS`/`VSYNC_LINES` re-cited from MAME's BSD-3 huc6260.cpp.
- **API changes, inherited**: `BoardOptions::vertical_at_line_start` removed;
  `Vdc::new(power_on, vblank_dots, dma_resumes)`; `Sequencer::new(vblank_dots)` and
  `Sequencer::active_display` a field; `sprites::scan(vram, size, sat, line, mwr, limit,
  searched)`; `CLOCKS_PER_SPRITE`, `SAT_DOTS`, `DMA_DOTS_PER_WORD` removed; the core's
  `VanishedInterrupt::Latched` and `IrqPollCycle::Mednafen` removed.
- **API change, the fork's own**: `vdc::sprites::scan_into` is gone (its reuse no longer pays:
  below). The fork's public additions are now two, `Board::touch` and `vdc::background::row_into`.
- **Save states: format version 3** (the VDC's and the timer's layouts changed); versions 1 and 2
  are refused, not converted, as on the base board.
- **The core**: huc6280-rust-fast `73f0829` by path from a sibling clone (`../huc6280-rust-fast`);
  `Cargo.toml` names the git pin for after the push, and CI clones it by that revision, so **CI
  fails until huc6280-rust-fast `73f0829` is on GitHub**.
- **The fork's speed changes**: every one outside the withdrawn code kept unchanged. Lost with it:
  the line event list's reuse (old sequencer; nothing to re-apply) and the sprite list's reuse
  (re-applied fresh, x1.024/x1.010/x0.979 on three layouts: not kept). The row buffer's reuse
  in `render_line` was inert after the merge (the base board now drops the row at every line
  step) and is the base board's allocation again. docs/not-kept.md.
- **Gate** (2026-11-19): `cargo test` 68, lockstep 275 of 275 against the rebuilt cache, save
  states 18 of 18, seeded faults 80 caught and 3 missed of 83 (the board's wait, as on
  pcengine-rust), clippy on 1.93.1 and stable 1.99.1, fmt, wasm32. Speed: x4.624/x4.596/x4.728
  over pcengine-rust `26ea621` on three layouts, x0.988/x1.034/x1.028 against 0.2.0.
- **Tooling**: `scripts/ab.sh`, the layout-aware A/B (three code layouts, `ab.py` on each); the
  seeder copies the README into its tree and checks the unfaulted tree's `cargo test` and rung 2
  first (the 0.2.0 count of "76 caught" was overstated: every fault that reached the tests stage
  counted as caught) and copies `src/` and `tests/` with fresh file times (with copytree's
  `copy2` a file restored after a fault kept its old time and cargo could keep the faulted
  build of it); `ladder.sh` defaults to 8 processes; SPDX headers on every file the fork
  added; the reference cache rebuilt (pcengine-rust's, from `3318027`; the old one in
  `external/refcache-v0.2.0`).

## 0.2.0 (2026-11-13)

Merges pcengine-rust 0.2.0 (`39b29d1` and its follow-ups: pcengine-python v0.2.0, `4c32f26`) and
moves the CPU core to huc6280-rust-fast `90711f2` (huc6280-rust 0.2.0 merged: huc6280-python
v0.4.0, `265df88`).

- **The core**: huc6280-rust-fast `90711f2`, by its GitHub revision.
  `CpuState::polled_irq_lines`; the core records the interrupt lines at every access.
- **From the base board** (its CHANGELOG has the details): the board's own events run while the
  CPU waits on a VDC access (`Board::step`, `settle`, `serve_vram`, `sockets::VdcAccess`,
  `BoardPins::vdc_read` / `vdc_write`); the PSG's level table follows `silent_units`; dead code
  removed; save states at format version 2 (a 0.1.0 state is refused).
- **The fork's own changes for the merge**: the fork's "decide what `touch` left" step is now
  `decide` (the base board's new `settle` took the name); the hold of a served VDC wait is added
  to `held` directly (the fork's `Board::hold` also calls `touch`, which is for holds left from
  outside an access); the fork's `card_pins` hands the VDC access log too.

## 0.1.0 (2026-11-12)

The speed fork of pcengine-rust 0.1.0, held in lockstep with it.
