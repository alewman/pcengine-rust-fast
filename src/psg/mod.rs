// SPDX-License-Identifier: MIT
//! The HuC6280 PSG (`psg/`): board-owned; Aubrey's rule: it may move out if a second consumer
//! ever wants it, never be copied.
//!
//! `Psg` is the six-channel generator (psg/chip.rs): register writes with master-clock times and
//! a host-rate box-averaged output for the `AudioSource` list. Provisional, tier 5: no hardware
//! audio reference exists (the reference's docs/m4-record.md).

pub mod chip;

pub use chip::{Channel, KeyOnMode, Psg, PsgSwitches};
