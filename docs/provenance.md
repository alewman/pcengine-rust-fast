<!-- SPDX-License-Identifier: MIT -->
# Provenance

## Licence provenance (audit 2026-11-13)

pcengine-rust is a module-for-module transcription of pcengine-python (ours, MIT) and **inherits
pcengine-python's verdicts** (its `docs/provenance.md`, "Licence provenance (audit 2026-11-13)"),
audited from the records only, no GPL source opened. Carried over:

| Rust module | Python module | Verdict |
| --- | --- | --- |
| `src/vdc/sequencer.rs` | `vdc/sequencer.py`: Mednafen's `vdc.cpp` line machine "restated" | **rewrite** |
| `src/vdc/chip.rs` | `vdc/chip.py`: the scroll-latch state and `commit_unless_dma` follow Mednafen | **rewrite** (those parts) |
| `src/vdc/dma.rs` | `vdc/dma.py`: two Mednafen-placed functions | **unclear** |
| `src/machine.rs` | `HSYNC_CLOCKS = 237` with a quoted Mednafen line | **unclear** |
| `src/cpu_io/timer.rs` | `underflow_window_mclk = 15` read from Mesen2's code | **unclear** |
| `src/cards.rs` | MAME mappers (BSD-3) | **cite** |
| `src/psg/chip.rs` | MAME `c6280.cpp` volume table (BSD-3), notice in NOTICE.md | **cite** (done) |

**Since the clean rewrite of 2026-11-19** ([restart.md](restart.md)): the rows above are the audit
as it was. Item by item now:

| Rust module | What happened | Verdict now |
| --- | --- | --- |
| `src/vdc/sequencer.rs` | withdrawn unread (whole file); transcribed from the reference's new `vdc/sequencer.py` (`3318027`: manual, pcetech, MAME huc6270/huc6260 BSD-3, closed-box fits) | **clean** (MAME BSD-3 named in the header and NOTICE.md) |
| `src/vdc/chip.rs` | the scroll state, `latch_byr`, `latch_bxr`, `step_bg_y`, `line_counter_step`, `commit_unless_dma`, `write`, `fetch_sprites` and the module doc withdrawn unread; transcribed from the new `vdc/chip.py` | **clean** |
| `src/vdc/dma.rs` | withdrawn unread (whole file); transcribed from the new `vdc/dma.py` (pcetech 5.4, the manual's MWR) | **clean** |
| `src/machine.rs` | the constants and module doc withdrawn unread; `HSYNC_CLOCKS`, `LINE_CLOCKS`, `VSYNC_LINES` cited from MAME huc6260.cpp's comments (BSD-3) as the reference now cites them | **clean** (notice in NOTICE.md) |
| `src/cpu_io/timer.rs` | the window field and module doc withdrawn unread; 3 master clocks, a hypothesis on the manual's PPS3, bounded by a closed-box measurement (the reference's) | **clean** |
| `src/cards.rs` | header names MAME's BSD-3 cartridge devices (notice in NOTICE.md); GPL file and function names dropped from comments | **clean** (cite done) |
| `src/psg/chip.rs` | header names MAME `c6280.cpp` (BSD-3) | **clean** (cite done) |
| `src/vdc/sprites.rs`, `src/vdc/background.rs`, `src/video.rs`, `src/board.rs`, `tests/cards.rs` | comments reworded to the reference's new wording (no emulator internals); `scan` gained `searched` | **clean** |
| `scripts/seeded_faults.py` | the seven faults holding withdrawn lines cut unread; sixteen new faults on the new code | **clean** |
| every other module (`board`, `vce`, `video`, `vdc/background`, `vdc/regs`, `vdc/sprites`, `cpu_io/*`, `pads`, `pagetable`, `hardware_page`, `system`, `sockets`, `state`, `mixer`, `fake_devices`) | the same modules | **clean** |

What the Rust tier added:

| Code | Source | Verdict |
| --- | --- | --- |
| `src/mt.rs` (Python's `random.Random` for seeded power-on) | MT19937 from Matsumoto and Nishimura's published description; CPython's behaviour as a closed box | **clean** |
| `src/trace.rs`, `src/bin/*`, `scripts/*` (lockstep, fault seeding, benches, `fetch_covell.sh` fetching Covell's test ROM by URL, never committed) | ours | **clean** |
| `tests/`, `tests/data/state-v1.pcestate` (a state of our own test program) | ours | **clean** |
| crates (Cargo.lock: serde, bincode, flate2, sha1 and their dependencies) | MIT/Apache-2.0/Zlib/Unicode-3.0, all permissive | **clean**; a binary distribution owes their notices |

Counts (the 2026-11-13 rows): clean 5, cite 2, rewrite 2, unclear 3. Since 2026-11-19: every
row clean; nothing left to rewrite or settle for licence reasons.

provenance_check (2026-11-13, permissive trees only, run from pcengine-python) found nothing here.
History: 15 commits, no deleted files. The huc6280-rust core it depends on is audited separately.
