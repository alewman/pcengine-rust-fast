# Changelog

Not on crates.io; pin a commit (README).

## 0.2.0 (2026-11-13)

Merges pcengine-rust 0.2.0 (`39b29d1` and its follow-ups: pcengine-python v0.2.0, `4c32f26`) and
moves the CPU core to huc6280-rust-fast `90711f2` (huc6280-rust 0.2.0 merged: huc6280-python
v0.4.0, `265df88`).

- **The core**: huc6280-rust-fast by path (`../huc6280-rust-fast`) until `90711f2` is on GitHub.
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
