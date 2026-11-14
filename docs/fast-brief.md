# Brief: pcengine-rust-fast

> **This is the brief the fork was built from (2026-11-12), kept as written**, with "What was
> built" appended at the end by the session that built it. Its instructions ("do not push", the
> working rules, the local paths under `/data/emu/`, which are the author's machine) are history,
> not instructions to a reader. The [README](../README.md) is the current account; where the two
> differ, the README is right (for example, the CPU is now huc6280-rust-fast, and the PSG's level
> curve is a written-out table, which resolves the wasm caveat the build left).

## The request

Aubrey, 2026-11-12: "get our rust version and rust fast and the play-pce site ready", and play-pce
is to run any game of the PC Engine family. This repository is the family's **ladder-gated speed
fork** of the base board, in the pattern of `/data/emu/a2600-rust-fast` (read its README,
`scripts/ladder.sh`, `scripts/ab.py`, `scripts/profile.py`, `docs/not-kept.md` first) and
`megadrive-rust-fast`.

It is a fork of `/data/emu/pcengine-rust` at `49e3d48` (remote `upstream`), package
`pcengine-rust-fast`, **crate name kept `pcengine_rust`** and **the public API kept exactly**, so
`supergrafx-rust` and `pcenginecd-rust` (the add-on crates, built on the base board's socket
traits) and the browser page can sit on this crate with nothing but a dependency path changed.
That is the constraint that shapes everything: no socket trait, `BoardOptions` field, `Frame`
shape, save-state or snapshot behaviour may change in a way an add-on or a page can see. If a
shortcut needs an API change, it is not kept (record it in `docs/not-kept.md`).

Why it matters: the reference Rust board runs about 5 times real time natively on a desktop and
the SuperGrafx about 3 times; in a browser on a phone that is not enough. The target is the
**wasm** build on modest hardware, so measure there too.

## The rule

The board's behaviour is the base board's, bit for bit: every optimisation is kept only if the
**gate** stays green and an **A/B measurement** shows it pays (Aubrey: ablate each feature, keep
what moves the number, delete the rest).

- **Gate** `scripts/ladder.sh`: `cargo test --release`, the lockstep gate from the reference cache
  (`python3 scripts/lockstep.py --reference-cache external/refcache --jobs 16`: 275 of 275 cases,
  about 30 seconds; the cache was copied from pcengine-rust, so no Python run is needed; if a case
  is missing from the cache see pcengine-rust's README for rebuilding it), the save-state round
  trip and run-ahead (rung 6), and the seeded faults at the end.
- **A/B** (`scripts/ab.py`, modelled on a2600-rust-fast's): a bench binary measuring CPU time (not
  wall time: the machine is shared and loaded) over N frames of five titles (Blazing Lazers, Bonk's
  Adventure, R-Type, After Burner II, and Street Fighter II' or another busy one), alternating
  A and B builds, best of 12 runs; report the A/A noise first. Keep a build of the fork point to
  measure the chained gain directly at the end.
- **Profile first** (a2600-rust-fast's `scripts/profile.py` approach, or `perf` if permitted):
  say where the time goes (VDC per-dot work, the sequencer's events, the PSG and mixer, the bus
  decode, the CPU core) before changing anything. If the CPU core (`../huc6280-rust`) is a large
  share, do not edit it: report the share and what a `huc6280-rust-fast` would target.
- Each kept change: one commit, with its measured factor in the message; each rejected one: a line
  in `docs/not-kept.md` with its number.
- Seeded faults: the inherited 48 must still be caught, plus at least one new fault on every
  shortcut the fork adds (a shortcut that no fault can break is not tested).

Also measure **under wasm**: build the library for `wasm32-unknown-unknown` with the rustup
1.93.1 toolchain (`/usr/bin/cargo` has no wasm std), run a bench under Node (see how
`/data/emu/play-2600` or `/data/emu/play-nes` wrap a board with wasm-bindgen and time it), and
report frames per second there before and after. Check the PSG's `powf` level table gives the same
128 values under wasm as natively (the base README's caveat); if not, say so loudly.

## Rules

- Do not modify pcengine-rust, huc6280-rust, 6502-rust or the Python repositories. Fixes that are
  not speed (a bug found on the way) are reported, not made here.
- No GPL or LGPL code, copied or transcribed; do not open any `reference/` directory.
- Never commit ROMs. At most 12 processes, `nice`. No `sed -i`, `rm -rf` or `ln -sfn`. Temp files
  under `external/`, `target/`, `scratch/`.
- clippy via `~/.rustup/toolchains/1.93.1-*/bin/cargo-clippy`, `-D warnings`; `cargo fmt`.
- Commits locally, trailer `Co-Authored-By: Claude Fable 5.2 <noreply@anthropic.com>`. **Do not
  create a GitHub repository and do not push.**
- Stop when the profile is flat (no line above about 4 per cent) or three attempts in a row are not
  kept. Report: the profile before and after, each kept change with its factor, the chained factor
  measured against the fork point natively and under wasm, what was not kept, the faults table,
  the gate's final result, and confirmation that the public API is unchanged (the add-on crates
  build against this crate: try `cargo build` of `/data/emu/supergrafx-rust` with a temporary
  `--config 'patch...'` or a scratch copy, without editing that repository).

## What was built

(2026-11-12) The fork as asked: package `pcengine-rust-fast`, crate `pcengine_rust`, the public
API unchanged but for three added functions (`Board::touch`, `background::row_into`,
`sprites::scan_into`). Eleven kept changes, one commit each with its measured factor; x4.3
natively and x4.0 under wasm against the fork point, measured directly; four attempts not
kept (docs/not-kept.md); 68 seeded faults caught (the inherited 48 and 20 on the fork's own
shortcuts); the gate green on every commit's tree. The README has the tables, the profile
before and after, what a host must know (`touch`, the lent ROM image, LTO) and the PSG
level-table finding under wasm. The CPU core is now the largest share of a frame (about 32%)
and was not edited. Not pushed; no GitHub repository.

(2026-11-13, at publication) pcengine-rust's publication cleanup was merged, and the CPU core
was swapped for its speed fork, huc6280-rust-fast: x1.075 on the five titles, the gate green.
