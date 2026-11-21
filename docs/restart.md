<!-- SPDX-License-Identifier: MIT -->
# The clean rewrite of 2026-11-19, carried into pcengine-rust-fast

Aubrey, 2026-11-19: "rewrite the modules that used GPL sources if we haven't already." The licence
audit of 2026-11-13 ([provenance.md](provenance.md)) marked parts of pcengine-python **rewrite**
(the VDC's line machine and parts of the VDC chip) or **unclear** (a few constants whose only
stated source was GPL code that had been read). pcengine-rust, transcribed line by line from it,
inherited those verdicts, and this fork, held in lockstep with pcengine-rust, inherited them again,
with its own speed changes on top of some of that code. pcengine-python replaced its parts first
(`3318027`), pcengine-rust carried them (`26ea621`, its own [restart.md](https://github.com/alewman/pcengine-rust/blob/main/docs/restart.md)),
and huc6280-rust-fast merged its core's rewrite (`73f0829`). This pass brings all of it here by
merges. Fixes start upstream: no behaviour of this crate was decided here.

## The rules this pass kept

1. **No GPL or LGPL source was read** (Mednafen, Beetle PCE, Mesen2, Geargrafx, the MiSTer core,
   MAME's GPL files): not to learn, not to check a constant, not when a comparison failed.
2. **The fork's counterparts of the withdrawn code were not read.** Before the merge they were
   moved, unread, into the git-ignored `scratch/withdrawn/` by `scratch/withdraw.py` (pcengine-rust's
   helper, also ignored), which prints only names and line counts, the same items pcengine-rust
   withdrew:
   - whole files: `src/vdc/sequencer.rs`, `src/vdc/dma.rs`;
   - from `src/vdc/chip.rs`: the module documentation, `latch_byr`, `latch_bxr`, `step_bg_y`,
     `line_counter_step`, `commit_unless_dma`, `write`, `fetch_sprites` and the fields `bg_y`,
     `need_bg_y_step`, `bg_y_offset`, `bg_x_offset`, `mwr_cache` (declarations and initialisers);
   - from `src/cpu_io/timer.rs`: the module documentation and `underflow_window_mclk`;
   - from `src/machine.rs`: the module documentation and `HSYNC_CLOCKS`, `LINE_CLOCKS`,
     `VSYNC_LINES`;
   - from `scripts/seeded_faults.py`: the eight faults whose text was in that code (pcengine-rust's
     seven and the fork's `line-events-not-emptied`).

   That is commit `7a6a3f0` (the tree there does not build). The merge of pcengine-rust `26ea621`
   followed in `1e8dcf4`; every conflict on a withdrawn item took pcengine-rust's side, where the
   fork's side was the empty cut, so no conflict showed old text. The untouched rest of those
   files (the fork's changes to it compared against the same cut of the merge base, both sides
   already without the withdrawn items), the call sites and the tests defined the interface.
3. **A speed change of the fork's that lived inside withdrawn code was dropped**, and re-applied
   only when it could be written fresh against the new code without looking at the old, and only if
   it paid on every code layout (`scripts/ab.sh`). Otherwise it is in [not-kept.md](not-kept.md) as
   lost to the rewrite.
4. **The judge is the base board**, as always for this fork: every lockstep case against the
   reference cache pcengine-rust rebuilt from pcengine-python `3318027` (the old one moved aside to
   `external/refcache-v0.2.0`), the save states, the seeded faults, `tests/fork.rs`.

## What came in

pcengine-rust's restart.md has the item list (the line machine, the scroll counter, `write`, the
sprite fetch with the manual's budget, `mwr_cache` at vblank, the DMAs, the machine constants
re-cited from MAME's BSD-3 huc6260.cpp, the timer window 3, `vblank_dots` default `Some(-16)`,
state format version 3) and the API changes (its CHANGELOG, "Unreleased"). The core is
huc6280-rust-fast `73f0829` (huc6280-rust `e1d24a3`: decimal `ADC`/`SBC` re-derived,
`VanishedInterrupt::Latched` and `IrqPollCycle::Mednafen` withdrawn, `polled_irq_lines` cleared at
a boundary that decides an interrupt), by path from a sibling clone until it was pushed
(2026-11-20), by its git pin since.

## The fork's speed changes, item by item

| Change | Where it lived | Now |
| --- | --- | --- |
| The event horizon, the quiet bus callbacks, the plain cards, the lent ROM, the port device's `catch_up` only (board.rs, cards.rs, system.rs) | not withdrawn | kept unchanged |
| The VDC's next event by hand (`earlier`, `next_event`, `catch_up`'s due time) | chip.rs, not withdrawn | kept unchanged |
| `holding()`, `drawn_row()`, the row drawn into in place (`render_line`, `row_into`) | chip.rs, not withdrawn | kept unchanged |
| The row buffer reused (`render_line`) | chip.rs, not withdrawn | inert after the merge (the new `line_begins` drops the row at every step), so the base board's allocation again, its fault gone (not-kept.md) |
| The timer's `next_tick()` for the horizon | timer.rs, not withdrawn | kept unchanged |
| The bitplane table, `row_into`, the capture's quick row | background.rs, video.rs, not withdrawn | kept unchanged |
| The sprite list reused from line to line (`scan_into`) | the call in the old `fetch_sprites` (one line) | `scan_into` kept (it gained the new `searched` argument); the call written fresh in the new `fetch_sprites`: see below |
| The line's event list reused (six lines of the old sequencer, with the fault `line-events-not-emptied`) | the old `sequencer.rs` | **lost to the rewrite**, with nothing to re-apply: the new sequencer keeps one sorted event list for its whole life (`schedule` pushes and sorts, `seq_run_to` takes from the front), so no list is made a line (not-kept.md) |

## The gate, before and after

"Before" is the fork's 0.2.0 record (2026-11-13, against pcengine-python `v0.2.0`); "after" is this
pass against `3318027`.

| Check | Before | After |
| --- | --- | --- |
| `cargo test` | 63 pass | 68 pass (pcengine-rust's new tests: the DVSSR rule, the version-2 refusal, the poll's lines in a state, MWR's SM at vblank, the waited write's time) |
| Lockstep, all cases | 275 of 275 identical | **275 of 275 identical on the first run** (84,200 frames) |
| Save states, run-ahead | 18 of 18 | 18 of 18 |
| Seeded faults | "76 caught, 0 missed" (overstated: the seeder copied no README, so `cargo test` failed in its tree whatever the fault, as on pcengine-rust) | **80 caught, 3 missed** of 83 ([seeded-faults.md](seeded-faults.md)): the three are the board's wait, missed on pcengine-rust too |
| clippy `-D warnings` (rustc 1.93.1 and stable 1.99.1, each its own target directory), fmt, wasm32 build | pass | pass |
| supergrafx-rust `4fa5aad` (on pcengine-rust `26ea621`) on this crate, dependency line changed in a scratch copy | | builds, 55 tests pass |
| Speed (`scripts/ab.sh`, three layouts) | x4.7 over the fork point, chained | x4.624 / x4.596 / x4.728 over pcengine-rust `26ea621`; x0.988 / x1.034 / x1.028 against 0.2.0 |

## Follow-up: the early display start (pcengine-rust `37f17ba`, 2026-11-19)

pcengine-python `03ac699` crops a display that starts before the row instead of growing the row,
and pcengine-rust carried it in `37f17ba` (found by supergrafx-rust's probe card; this fork
panicked on `first as usize` there too). Merged here by `git merge`; no withdrawn code was
involved. `render_line` conflicted because the fork draws into the row in place, so the fix was
transcribed into the fork's shape (CHANGELOG). The other conflicts (the seeder's fresh times,
seeded-faults.md, README, CHANGELOG) were the fork's own versions of the same changes, kept.

The reference moved to `03ac699`: the cache was rebuilt in this fork from a fresh checkout under
`external/pcengine-python` (the `3318027` cache is in `external/refcache-3318027`) and came out
byte-identical, all 275 traces, to pcengine-rust's `03ac699` cache. No case in the gate reaches
an early display start, so the traces are the same as `3318027`'s; the unit test is what covers
the new path. Two tooling holes turned up while rebuilding it, both in inherited scripts and left
as they are: `fetch_pcenginepython.sh` with `CORE_SOURCE` installs the core `--no-deps`, so the
venv lacks the core's own dependency, 6502-python (`sixfiveohtwo`); it was installed by hand at
the revision huc6280-python pins (`2146eba`). And `scripts/ab.py` takes `LIB` from
`make_cases.DEFAULTS` without the environment, so it only works on the five titles already
extracted into `scratch/bench/`.

| Check | Before (`7c53f35`, against `3318027`) | After (against `03ac699`) |
| --- | --- | --- |
| `cargo test` | 68 pass | 69 pass (the crop's unit test) |
| Lockstep, all cases | 275 of 275 | 275 of 275 |
| Save states, run-ahead | 18 of 18 | 18 of 18 |
| Seeded faults | 80 caught, 3 missed | 80 caught, 3 missed |
| clippy `-D warnings` (1.93.1, stable 1.99.1), fmt, wasm32 | pass | pass |

## What stays open

Everything pcengine-python's restart.md lists as open about the hardware (the raster compare's
place, the scroll latch fit, the display start, the front porch, the SAT transfer at 7 and 10 MHz,
the timer window, the two library regressions against the referees). They belong
upstream; this fork follows the base board. The vblank fit is settled (Aubrey, 2026-11-20: -16
kept as the default, -18 a switch).
