<!-- SPDX-License-Identifier: MIT -->
# Provenance

## Licence provenance (audit 2026-11-13)

pcengine-rust-fast is a speed fork of pcengine-rust, held in lockstep with it, and **inherits
pcengine-rust's verdicts**, which are pcengine-python's (its `docs/provenance.md`, "Licence
provenance (audit 2026-11-13)"; audited from the records, no GPL source opened):

| Module | Verdict |
| --- | --- |
| `src/vdc/sequencer.rs` (Mednafen's `vdc.cpp` line machine, restated) | **rewrite** |
| `src/vdc/chip.rs` (the scroll-latch state and `commit_unless_dma` follow Mednafen) | **rewrite** (those parts) |
| `src/vdc/dma.rs`, `src/machine.rs` (`HSYNC_CLOCKS` with a quoted Mednafen line), `src/cpu_io/timer.rs` (a Mesen2 constant) | **unclear** |
| `src/cards.rs`, `src/psg/chip.rs` (MAME, BSD-3; the PSG notice is in NOTICE.md) | **cite** |
| every other module | **clean** |

What the fork added: speed changes to the same modules (its own work, gated by lockstep against
pcengine-rust), `src/bin/pce-profile.rs`, `scripts/*` (A/B, profiling, ladder), and `wasm-bench/`
(a Node bench crate, MIT, depending only on this crate): **clean**. The fork's brief forbids opening
`reference/`. Crates are the base board's set (with `huc6280-rust-fast`), all permissive.

Counts: clean 2, cite 1, rewrite 2, unclear 1 (grouped rows). provenance_check (2026-11-13,
permissive trees) found nothing here. History: 33 commits, no deleted files.

This is the board the live play-pce site is built from, so the **rewrite** items are what that
site distributes until they are rebuilt.

## Since the clean rewrite of 2026-11-19

[restart.md](restart.md): the fork's counterparts of the withdrawn code were moved aside unread
(`7a6a3f0`) and pcengine-rust `26ea621`'s clean code merged in their place (`1e8dcf4`). The rows
above are the audit as it was. Item by item now:

| Module | What happened | Verdict now |
| --- | --- | --- |
| `src/vdc/sequencer.rs` | withdrawn unread (whole file, with the fork's six-line change in it); pcengine-rust's transcription of the reference's new `vdc/sequencer.py` (`3318027`: Hudson's manual, pcetech, MAME huc6270/huc6260 BSD-3, closed-box fits) merged | **clean** (MAME BSD-3 named in the header and NOTICE.md) |
| `src/vdc/chip.rs` | the module doc, the scroll state, `latch_byr`, `latch_bxr`, `step_bg_y`, `line_counter_step`, `commit_unless_dma`, `write`, `fetch_sprites` withdrawn unread; pcengine-rust's new code merged. The fork's own changes outside those items (`earlier`, `next_event`, `holding`, `drawn_row`, `render_line`'s buffer) kept; the sprite list's reuse in the new `fetch_sprites` written fresh against the new code | **clean** |
| `src/vdc/dma.rs` | withdrawn unread (whole file); pcengine-rust's transcription of the new `vdc/dma.py` (pcetech 5.4, the manual's MWR) merged | **clean** |
| `src/machine.rs` | the constants and module doc withdrawn unread; `HSYNC_CLOCKS`, `LINE_CLOCKS`, `VSYNC_LINES` now cite MAME huc6260.cpp's comments (BSD-3) | **clean** (notice in NOTICE.md) |
| `src/cpu_io/timer.rs` | the window field and module doc withdrawn unread; 3 master clocks (a hypothesis on the manual's PPS3, bounded by the reference's closed-box measurement). The fork's `next_tick` kept | **clean** |
| `src/cards.rs`, `src/psg/chip.rs` | headers name MAME's BSD-3 sources (NOTICE.md); GPL file and function names gone from comments | **clean** (cite done) |
| `scripts/seeded_faults.py` | eight faults holding withdrawn lines cut unread (pcengine-rust's seven, the fork's `line-events-not-emptied`); pcengine-rust's seventeen new ones merged | **clean** |
| `docs/seeded-faults.md` | the fork's row for `line-events-not-emptied` named a GPL emulator's internal; the row is gone with the fault | **clean** |
| the fork's additions (`src/bin/pce-profile.rs`, `scripts/ab.py`, `ab.sh`, `profile.py`, `ladder.sh`, `wasm-bench.sh`, `wasm-bench/`, `tests/fork.rs`) | ours | **clean** |
| every other module | the same modules | **clean** |

Counts since 2026-11-19: every row clean; nothing left to rewrite or settle for licence reasons.
The core, huc6280-rust-fast `73f0829`, is audited in its own repository (huc6280-rust's clean
rewrite merged). What play-pce distributes becomes clean when it is rebuilt on this revision.
