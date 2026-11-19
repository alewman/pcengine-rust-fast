<!-- SPDX-License-Identifier: MIT -->
# pcengine-rust-fast

A speed fork of [pcengine-rust](https://github.com/alewman/pcengine-rust), the
Rust **PC Engine / TurboGrafx-16** board (itself transcribed from, and proven
equivalent to, [pcengine-python](https://github.com/alewman/pcengine-python)
`3318027`), forked at `49e3d48`, merged with pcengine-rust 0.2.0 at
`194c120` and with pcengine-rust's clean rewrite at `26ea621` (2026-11-19,
[docs/restart.md](docs/restart.md)). It is **held in lockstep with it**: every change must leave this
board's traces identical to the base board's on every case (and so to the
reference's), and must measure faster, or it is not kept. Its CPU is
[huc6280-rust-fast](https://github.com/alewman/huc6280-rust-fast), the speed
fork of [huc6280-rust](https://github.com/alewman/huc6280-rust), held in
lockstep with it in the same way. pcengine-rust is the readable one; this one
is for players, and above all for the wasm build on modest hardware: it runs
the PC Engine in the browser at
[play-pce.supertux.com](https://play-pce.supertux.com). The add-ons
[supergrafx-rust](https://github.com/alewman/supergrafx-rust) and
[pcenginecd-rust](https://github.com/alewman/pcenginecd-rust) run on it with
one dependency line changed. MIT licensed. Built with Claude (Claude Fable
5.2, through Claude Code) for alewman; the CPU swap and the preparation for
publication by Claude Opus 5.5. No ROMs, System Card or other BIOS are
included: bring your own.

**The crate is still `pcengine_rust` and its public API is the base board's**
(the socket traits, `BoardOptions`, `System`, `Frame`,
`save_state`/`load_state`, `save_snapshot`/`load_snapshot`), with three
functions added (below, "What a host must know"). A crate written for
pcengine-rust changes one line, its dependency.

## Status

**2026-11-19: the clean rewrite (unreleased, local).** pcengine-rust `26ea621`
merged (pcengine-python `3318027`: the VDC's line machine, DMAs and parts of
the chip rewritten from documents after the licence audit) and the core at
huc6280-rust-fast `73f0829` (huc6280-python `6ce348e`), by path from a sibling
clone until both are pushed (CI fails until then). The fork's counterparts of
the withdrawn code were moved aside unread first; every speed change outside
that code is kept, the two inside it are lost ([docs/not-kept.md](docs/not-kept.md)).
**x4.6 over pcengine-rust** on the five titles (x4.624, x4.596 and x4.728 on
three code layouts), and level with the 0.2.0 release (x0.988, x1.034,
x1.028): under "Speed". Gates, run on 2026-11-19 after the merge:

| Gate | What | Result |
| --- | --- | --- |
| `cargo test` | the sockets, the pads, the mappers on synthetic images, the PSG level table, the save-state round trip and refusals (a 0.2.0 state among them), the VDC wait (`tests/vdc_wait.rs`), `tests/fork.rs`'s invariants, the README's example | 68 pass |
| Lockstep | every case of `scripts/cases.tsv` against the reference's traces (`external/refcache`, rebuilt by pcengine-rust from `3318027`): Covell's test, the probe card, the 18 picture-gate titles, the five PSG games, the whole library of 179 HuCards and the Street Fighter II' card, every socket, machine row and switch | 275 of 275 identical, on the first run |
| Save states (format version 3) | the round trip and the run-ahead pass on the 18 picture-gate titles | 18 of 18 |
| Seeded faults | pcengine-rust's 66 and seventeen on this fork's shortcuts | 80 caught, 3 missed (the board's wait, as on pcengine-rust) |
| clippy `-D warnings` (rustc 1.93.1 and stable 1.99.1), fmt, wasm32 build | | pass |

pcengine-rust's ladder, which this gate runs from the same cases and the
same reference cache, is in its README (its
[Status](https://github.com/alewman/pcengine-rust#status) and
[How it is proven](https://github.com/alewman/pcengine-rust#how-it-is-proven));
the reference's traces are pcengine-python `3318027`'s on all 275 cases, 84,200
frames. **Nothing here was run on a real console**: the board is proven equal
to pcengine-python, and how close that is to the hardware is the reference's
record.

## Using it

```toml
[dependencies]
pcengine_rust = { package = "pcengine-rust-fast", git = "https://github.com/alewman/pcengine-rust-fast", rev = "<commit>" }
```

(Pin the commit you have checked; the crate is not on crates.io. It brings
huc6280-rust-fast by the git revision in its own `Cargo.toml`.) Rust 1.85 or
newer. The library is `pcengine_rust`, and `pcengine_rust::huc6280`
re-exports the CPU core. **Build with `lto = "fat"` and `codegen-units = 1`**
in your own release profile: this crate's profile reaches only its own
binaries, and the whole-program pass across the CPU core is worth a tenth.

```rust
use pcengine_rust::board::{Board, BoardOptions};
use pcengine_rust::cards::make_card;
use pcengine_rust::machine::TURBOGRAFX_16;
use pcengine_rust::pads::{self, Pad2};
use pcengine_rust::state::{load_snapshot, load_state, save_snapshot, save_state};
use pcengine_rust::system::System;

fn run(image: &[u8]) -> Result<(), String> {
    // A board from card bytes and a machine row (US cards want TURBOGRAFX_16).
    let board = Board::new(BoardOptions {
        machine: TURBOGRAFX_16,
        card: Some(make_card(image)?), // SF2 by size, Populous by its title, else plain ROM
        pad: Some(Box::new(Pad2::new())),
        ..BoardOptions::default()
    })?;
    let mut system = System::new(board);

    // One frame: hold buttons, run, take the sound and the picture.
    system.board_mut().set_pressed(0, pads::RUN | pads::I);
    system.run_frame();
    let _samples: Vec<(i16, i16)> = system.board_mut().take_audio(); // 44,100 Hz stereo
    if let Some(frame) = system.board().capture.last_frame.as_ref() {
        let table = &system.board().vce.rgb_table; // 512 entries of (r, g, b)
        for (row, _divider) in frame.rows.iter().zip(&frame.dividers) {
            // 242 rows (frame lines 14-255); a row has 1365 / divider dots: 341, 455 or 682
            let _rgb: Vec<(u8, u8, u8)> = row.iter().map(|&value| table[value as usize]).collect();
        }
    }

    // Save states (checked, compressed) and snapshots (neither; for run-ahead).
    let state: Vec<u8> = save_state(&system);
    load_state(&mut system, &state)?;
    let snapshot: Vec<u8> = save_snapshot(&system);
    load_snapshot(&mut system, &snapshot)
}

run(&[0u8; 0x2000]).unwrap(); // an empty 8 KiB image stands in for your card
```

This example is compiled and run by `cargo test` (as a doctest), as is the
one in `src/lib.rs`. The machine rows, cards, frames, picture, audio, pads,
save states, switches, reset and the twelve sockets an add-on plugs into are
the base board's: its README's
[Using it](https://github.com/alewman/pcengine-rust#using-it) and
[How an add-on crate plugs in](https://github.com/alewman/pcengine-rust#how-an-add-on-crate-plugs-in)
are the guide.

### What a host must know

Nothing, if it uses `System` (`board_mut()`, `step()`, `run_frame()`,
`run_until()`) and `BoardOptions`, as the add-on crates and the pages do. The
fork adds two public functions and changes none: `Board::touch` and
`vdc::background::row_into` (`vdc::sprites::scan_into`, a third until
2026-11-19, is gone: docs/not-kept.md).

- **`Board::touch()`** tells the board it was reached from outside a bus
  access, so it forgets the event horizon and decides its shortcuts again.
  `System::board_mut`, `step`, `run_frame` and `run_until` call it, and so do
  `Board::reset`, `hold`, `advance` and `vdc`. The one case a host must call
  it itself: it holds a bare `Board` (or goes through `system.cpu.bus`),
  changes a public field between two bus callbacks, and then drives the bus
  (or `system.cpu.step()`) directly.
- **A `RomCard`'s `data` is with the board while it runs** (moved, not
  copied), when there is no port device and the pixel path is the default:
  `touch()` puts it back, so a host that looks through `board_mut()` finds
  it, and changes to it are what the CPU reads next. The mapper cards and any
  card that is not this crate's own are not lent.
- **The taps** (`bus_log`, `bus_detail`) are honoured on every path; the gate
  runs with them on.
- **Save states** are the base board's, byte for byte, for the same run (the
  bench's checksum covers the snapshot); the format and the refusals are
  the base board's (format version 3 since the clean rewrite: a state saved
  by 0.2.0 or earlier is refused, not converted). The PSG's level curve is a written-out table of doubles
  (`LEVEL_BITS`, from pcengine-rust), so a state saved in a browser has a
  native build's bytes.

## Speed

Measured on an Intel i9-13900K (32 threads, shared with other work), rustc
1.93.1, Node 22 for wasm; `pce-bench` by the thread's CPU time, two builds
alternated title by title on one pinned efficiency core, each side's best of
12 runs, the checksums of every picture, every sample and the final snapshot
equal on both sides (`scripts/ab.py`).

**The clean rewrite (2026-11-19): x4.6 over pcengine-rust, level with 0.2.0.**
`scripts/ab.sh` (each side built under three code layouts, `ab.py` on each,
best of 8 runs a side, one pinned efficiency core, load average about 20 from
other work and this pass's seeded faults). Against pcengine-rust `26ea621`
with this fork's `pce-bench` dropped in and the same release profile (the
checksums, final snapshot included, equal on both sides):

| Title (600 frames) | default | nf5 | nf6 |
| --- | ---: | ---: | ---: |
| Blazing Lazers | x4.692 | x4.658 | x4.922 |
| Bonk's Adventure | x4.648 | x4.774 | x5.110 |
| R-Type | x4.638 | x4.943 | x4.867 |
| After Burner II | x4.675 | x4.480 | x4.368 |
| Street Fighter II' | x4.470 | x4.166 | x4.420 |
| all five (geometric mean) | **x4.624** | **x4.596** | **x4.728** |

(pcengine-rust ran 195 to 239 frames a second, this fork 872 to 1108.)
Against the 0.2.0 release (`0364837`, `--behaviour-changed`: the state
format and the picture changed, each build agrees with itself): **x0.988,
x1.034, x1.028** on the three layouts, titles from x0.951 to x1.095: no change
the measurement can tell from its noise. Under wasm (`scripts/wasm-bench.sh`,
Node 22, best of 5): 504 to 655 frames a second on the five titles, 8.4 to
10.9 times real time, with the snapshot's bytes equal to the native build's
(not an A/B: the 2026-11-12 figures below were taken on a quieter machine).

**The board's own changes (2026-11-12): x4.349 natively, x4.04 under wasm**
over the fork point, measured directly, both on huc6280-rust. Frames per
second, one core, from power-on:

| Title (600 frames, the picture gate's input) | fork point | board changes | | fork point, wasm | board changes, wasm | |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| Blazing Lazers | 236.6 | 1045.4 | x4.418 | 150.5 | 600.9 | x3.99 |
| Bonk's Adventure | 263.0 | 1245.4 | x4.735 | 166.8 | 751.0 | x4.50 |
| R-Type | 258.0 | 1193.4 | x4.626 | 153.2 | 634.1 | x4.14 |
| After Burner II | 253.9 | 1022.4 | x4.027 | 155.8 | 598.3 | x3.84 |
| Street Fighter II' | 257.0 | 1026.3 | x3.993 | 154.3 | 580.3 | x3.76 |
| all five (geometric mean) | | | **x4.349** | | | **x4.04** |

On a performance core of the same machine the five titles ran 1166 to 1912
frames a second (350 to 397 at the fork point, x3.80), which is 19 to 32
times real time. Under wasm (`wasm-bench/` under Node 22, fat LTO as the
pages build, no `wasm-opt`): 9.7 to 12.6 times real time, where the fork
point has 2.5 to 2.8. With a device on the expansion port (the CD unit's
place; here `--device lockstep-port`, the gate's test device) the board must
call the device every cycle, and the chain is x1.713 (214.1 to 344.3 frames
a second on Blazing Lazers).

**The merge of pcengine-rust 0.2.0 (2026-11-13): x0.954.** The fork before
the merge (`0ed0390`) against the fork after it, on one efficiency core while
other gate work ran on the machine (`scripts/ab.py --behaviour-changed`: the
checksums differ, because the final snapshot is in them and the state format
changed; each build agrees with itself on every run). An A/A run of the
first build: x1.011.

| Title (600 frames) | before the merge | after | |
| --- | ---: | ---: | ---: |
| Blazing Lazers | 1007.1 | 996.9 | x0.990 |
| Bonk's Adventure | 1160.8 | 1081.4 | x0.932 |
| R-Type | 1058.4 | 1013.7 | x0.958 |
| After Burner II | 988.0 | 954.0 | x0.966 |
| Street Fighter II' | 983.7 | 913.4 | x0.929 |
| all five (geometric mean) | | | **x0.954** |

Split in two with a build of the old board on the new core (measurement
only, not kept: the old board's state given `polled_irq_lines: None`): the
core alone (huc6280-rust-fast `2f4d4d2` to `90711f2`, which records the
interrupt lines at every access; its own Dormann measurement was x0.95)
x0.966, and the board's part of the merge on top of it (the VDC access log
reset at every decode, the wait check after it, the PSG's level-table check
per emitted step, `run_to` as `step` and `settle`) x0.964. The two halves
were measured one after the other on a busy machine and their product
(x0.931) is below the direct x0.954: read each to about two percent. Not
optimised here; that is the fork's next work.

**The CPU core swapped for huc6280-rust-fast (2026-11-13): x1.075.** This
tree built against huc6280-rust (`65c18df`) and against huc6280-rust-fast,
nothing else different, on one efficiency core (load average about 11 to 14
at the start and end of the runs); an A/A run of the first build against itself first:

| Title (600 frames) | huc6280-rust | huc6280-rust-fast | |
| --- | ---: | ---: | ---: |
| Blazing Lazers | 980.5 | 1091.1 | x1.113 |
| Bonk's Adventure | 1234.9 | 1290.6 | x1.045 |
| R-Type | 1109.2 | 1186.6 | x1.070 |
| After Burner II | 1032.8 | 1123.8 | x1.088 |
| Street Fighter II' | 983.7 | 1043.6 | x1.061 |
| all five (geometric mean) | | | **x1.075** |

A/A: x0.996 overall (x0.979 to x1.002 a title). A second A/B on another
efficiency core gave x1.073 (every title up in both runs, x1.045 to x1.113). huc6280-rust-fast's own measurement of the
same swap was x1.067 natively and x1.016 under wasm (its README); a CD game
on pcenginecd-rust gained x1.04 to x1.07. The two factors were measured
separately; their product, about x4.7 over the fork point natively, was not
measured directly.

| Kept, in order | A/B against the build before it (both with fat LTO, one codegen unit) |
| --- | ---: |
| **The event horizon.** The board keeps the earliest master clock at which `run_to` has anything to do (the next line, the end of the row, a VDC event, the timer's next tick); a CPU cycle that ends before it only moves the clock, and a work RAM access then skips `finish_access`. The base board runs `run_to` and `deliver_lines` after every cycle | x1.719 |
| **The VDC's next event by hand**, not three options folded through an iterator (it is asked after every card access) | x1.176 |
| **This crate's own cards finish like work RAM**: `RomCard`, `Sf2Card`, `PopulousCard` and `TennokoeCard` use nothing of their pins, so no stretch, hold, line or horizon can follow an access to one | x1.479 |
| **A captured row with no palette write inside it** is looked up in one pass, with no dot times | x1.052 |
| **A tile line's eight pixels from a bitplane table** (four lookups ORed), drawn straight into the row | x1.038 |
| **The line path without copies**: with the default pixel path the capture reads the VDC's row where it lies; the row, the sprite list and the event list reuse their buffers (since the clean rewrite only the reading in place and the drawing into the row remain: docs/not-kept.md) | x1.031 |
| **With a port device, only its `catch_up` before the horizon** (it is promised one every cycle), not the whole of `run_to` | x1.183 with a port device |
| **The bus callbacks' quiet case keeps no register across a call**; the taps, the events and the full access are out of line | x1.132 |
| **A `RomCard`'s image on loan to the board while it runs**: the ROM read inline, not through the card's trait with a `BoardPins` built for it | x1.066 |
| **Work RAM and plain-card accesses stay quiet beside a port device**, unless its `catch_up` left a hold | x1.231 with a port device |
| **Fat LTO, one codegen unit** for the crate's own binaries: the CPU core and the 65xx frame under it are other crates | x1.078 |
| **The CPU core: huc6280-rust-fast** in place of huc6280-rust (2026-11-13) | x1.075 |

The fat-LTO row is the tree of that day built without LTO against itself
built with it; the fork point gains only x1.029 from the same two lines (its
time was in the scheduler, inside this crate). A/A (that build against
itself, 1,500 frames): x0.987. Tried and not kept, with their numbers:
[docs/not-kept.md](docs/not-kept.md) (two attempts at the sprites, a colour
table by pixel value for the capture, thin LTO, one codegen unit alone).

**supergrafx-rust on this crate** (a scratch copy of it at `1d0528a` with the
dependency line changed and nothing else, 2026-11-12, on huc6280-rust): it
builds, its 52 tests pass, its own 22 lockstep cases are identical from its
reference cache, and its bench goes from about 150 to about 625 frames a
second on 1941 Counter Attack and from 160 to 700 on Aldynes (wall clock, the
same efficiency core, its own profile without LTO). huc6280-rust-fast's
README records both add-ons passing on this board with the fast core too.

### Where the time goes

Share of the samples by source file (`pce-profile`, library code charged to
its caller), 600 frames at the fork point and 2,000 after the board's
changes, both on huc6280-rust:

| | Blazing Lazers, fork point | board changes | Street Fighter II', fork point | board changes |
| --- | ---: | ---: | ---: | ---: |
| The CPU core (huc6280-rust, and the 65xx frame of 6502-rust under it) | 10.4% | 32.1% | 8.0% | 31.8% |
| `board.rs`: the bus callbacks, `run_to`, the decode | 28.4% | 26.4% | 29.2% | 23.6% |
| `vdc/chip.rs`: `next_event`, `catch_up`, the port | 35.7% | 5.1% | 39.2% | 3.9% |
| `vdc/sequencer.rs`, `vdc/dma.rs` | 5.8% | 3.7% | 5.4% | 3.1% |
| `vdc/background.rs`, `vdc/sprites.rs`, `video.rs`: the picture | 9.0% | 20.6% | 10.0% | 21.9% |
| `cards.rs`, `pagetable.rs` | 3.3% | 3.6% | 2.4% | 7.8% |
| `psg/chip.rs`, `mixer.rs` | 1.1% | 3.4% | 0.7% | 3.7% |

At the fork point two thirds of a frame were the scheduler: after every CPU
cycle `run_to` asked each VDC and the timer for their next event
(`Vdc::next_event` alone was a third, most of it an iterator over an array
of three options) and `deliver_lines` recomputed the three interrupt lines.
The picture, the sound and the CPU together were a fifth.

After the board's changes the profile was flat (no source line above 4%) and
the largest share was the CPU core, which is what huc6280-rust-fast then
took on: on huc6280-rust-fast the core is 29.0% of Blazing Lazers and 27.3%
of Street Fighter II', with no line of it above 3% (its README). The board's
own share is mostly the bus callbacks: about thirty instructions a quiet
cycle (the clock, the horizon test, the page's handler, the byte, the taps).
Under wasm the largest single function of a frame is the board's `Bus::read`,
about a fifth: the next gain for phones is there. The picture is a fifth: the
sprite scan and drawing, the background, the capture's lookup, each a few
per cent (two attempts at the sprites did not measure: docs/not-kept.md).

## How it is proven

**The rule.** A change is kept only if the gate stays green and an A/B
measurement shows a gain above the A/A noise; it is then one commit with its
factor in the message. A rejected one is a line in
[docs/not-kept.md](docs/not-kept.md). Every kept change's commit passed the
gate as a tree of its own.

**The gate**, `scripts/ladder.sh` (a few minutes from the cache; `--faults`
adds the seeded faults, `JOBS=n` the parallelism, default 12): `cargo test`;
every lockstep case against the reference traces (`python3
scripts/lockstep.py --reference-cache external/refcache`: 275 of 275; the
cache is pcengine-rust's, whose traces are pcengine-python's on all of them,
so no Python runs); the save-state round trip and the run-ahead pass on the
18 picture-gate titles. `pce-bench` adds a check the gate's lines do not
have: its checksum covers the snapshot's bytes at the end of the run, and
`scripts/ab.py` refuses to compare two builds whose checksums differ, so
**the state is the base board's too**, not only the bus, the picture and the
sound.

**The state after every cycle is the base board's.** The shortcuts skip work
only where it would have found nothing to do; `tests/fork.rs` checks it
directly: after every instruction of a program that runs the timer, a
VRAM-to-VRAM DMA through the display period and VRAM writes at high CPU
speed, a copy of the VDC and of the timer caught up to the clock equals the
original; and cycle by cycle with a port device that starts a DMA from its
`catch_up`.

**Seeded faults** (`scripts/seeded_faults.py`, rung 7): pcengine-rust's 66
and seventeen on this fork's shortcuts (at least one on every kept change that
has code): **80 caught, 3 missed** (the three on the board's wait that pcengine-rust misses too) on 2026-11-19 after the clean rewrite. The table is
in [docs/seeded-faults.md](docs/seeded-faults.md); the 0.2.0 count (76
caught) was overstated, as pcengine-rust's was: the seeder copied no README
into its tree, so `cargo test` failed there whatever the fault.

**Measuring** on a shared machine: `pce-bench` times `run_frame` and
`take_audio` by the thread's own CPU time (`/proc/thread-self/schedstat`),
and `scripts/ab.py` alternates two builds title by title on one pinned core
and compares each side's best of 12 runs (other work only ever slows a run
down). The five titles are Blazing Lazers, Bonk's Adventure, R-Type, After
Burner II and Street Fighter II', with the picture gate's input (RUN at frame
250, I at 400, right with II from 480); `--extra "--device lockstep-port"`
measures the port-device path. An A/A run is within 1.5% overall and 6% a
title. A build has its own luck too: without LTO, how the compiler splits the
crate into codegen units moved one and the same change from x1.09 to x1.01
between two builds of it, so each change's factor is between two builds with
fat LTO and one codegen unit, and a change below about 3% is not kept.
**Profiling**: `pce-profile` (feature `profile`) samples the program counter
on SIGPROF and `scripts/profile.py` resolves the samples with `addr2line`,
inlined frames included, by crate and file, by function and by line.

### Reproducing

```text
scripts/ladder.sh [--faults]            # the gate: cargo test, 275 lockstep cases from the cache, the save states
scripts/ab.py BIN_A BIN_B [--frames N] [--extra ARGS]    # A/B two builds of pce-bench; --fps BIN for one
cargo run --release --features profile --bin pce-profile -- IMAGE --frames 600 && scripts/profile.py
scripts/wasm-bench.sh [--base DIR]      # the five titles under Node, the PSG level table against native
scripts/seeded_faults.py [--only REGEX] [--check]
```

The base board's rung scripts (`scripts/rung1.sh` to `rung7.sh`,
`make_cases.py`, `make_probe.py`, `fetch_pcenginepython.sh`,
`fetch_covell.sh`) are inherited and work here unchanged; pcengine-rust's
README describes them. What the gate needs: Rust 1.85 or newer; Python 3 for
the scripts (standard library only); `7z` to read the `.7z` cards;
`external/refcache`, the reference's traces of the 275 cases (copy
pcengine-rust's, or let `scripts/lockstep.py` make it with the reference,
`scripts/fetch_pcenginepython.sh`, about 25 minutes of 16 PyPy processes);
and your own ROMs. The script defaults are the author's layout and each is
overridden by an environment variable: `LIB` (the directory of the 179
HuCard `.7z` images, named as No-Intro names them), `COVELL` (Covell's
`CPU_Test_10.pce`, which `fetch_covell.sh` fetches), `SF2` (the Street
Fighter II' card, which is not in the library) and `JOBS`. The benchmark's
five titles are extracted once into `scratch/bench/` (gitignored); the
library is only read.

CI (`.github/workflows/ci.yml`, main and pull requests) runs `cargo fmt
--check`, `cargo clippy -D warnings`, `cargo test` and rung 1 on Covell's
test, fetched by the reference's pin. The lockstep gate needs the library
and runs by hand.

### Layout

pcengine-rust's modules, with the same names (its README's
[Layout](https://github.com/alewman/pcengine-rust#layout)). The fork adds
`src/bin/pce-profile.rs`, `tests/fork.rs`, `scripts/ladder.sh`, `ab.py`,
`profile.py`, `wasm-bench.sh`, `wasm-bench/` (a crate of its own: the board
behind five plain exports for `wasm32-unknown-unknown`, run under Node) and
`docs/fast-brief.md`, `docs/not-kept.md`; `pce-bench` times by CPU time and
prints a checksum.

## Notes on the reference, provenance

Nothing found here is new: the places where pcengine-python's behaviour
looked unintended were found while pcengine-rust was transcribed, and are its
README's [Notes on the reference](https://github.com/alewman/pcengine-rust#notes-on-the-reference),
filed in pcengine-python as
[issue 0004](https://github.com/alewman/pcengine-python/blob/main/docs/issues/0004-findings-from-the-rust-port.md);
the CPU core's are huc6280-python's
[issue 0005](https://github.com/alewman/huc6280-python/blob/main/docs/issues/0005-interrupt-edge-cases-found-by-the-rust-port.md).
Both are **settled** (pcengine-python `v0.2.0`, huc6280-python `v0.4.0`), and
this fork follows the settlements through the merge, as the lockstep
requires.

No GPL or LGPL source was copied, read or transcribed for this crate, and no
emulator's source was opened. The only sources transcribed are our own MIT
repositories, pcengine-python and (through the CPU core) huc6280-python,
whose trees are checked for copied or transcribed emulator code by
pcengine-python's `scripts/provenance_check.py`
([docs/provenance.md](https://github.com/alewman/pcengine-python/blob/main/docs/provenance.md));
this fork's changes are restructurings of pcengine-rust's own code. Since the
clean rewrite of 2026-11-19 ([docs/restart.md](docs/restart.md),
[docs/provenance.md](docs/provenance.md)) the code that followed GPL sources
in the reference, and its counterpart here with the fork's changes inside it,
has been withdrawn unread and replaced by pcengine-rust's transcription of the
reference's replacement. MAME's BSD-3-Clause sources the reference read as
descriptions, and one small table, are acknowledged in [NOTICE.md](NOTICE.md).

[docs/fast-brief.md](docs/fast-brief.md) is the brief this fork was built
from, with what was built appended; [docs/handoff-brief.md](docs/handoff-brief.md)
is pcengine-rust's, inherited, and [docs/ladder.md](docs/ladder.md) its
account of the gate.

## Licence

MIT (see [LICENSE](LICENSE)), with the acknowledgement in
[NOTICE.md](NOTICE.md). Nothing third-party is committed: the reference, the
cards and Covell's test are fetched or read in place, and `.gitignore`
refuses the ROM, CD image and trace files.
