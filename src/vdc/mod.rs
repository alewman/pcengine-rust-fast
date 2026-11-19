// SPDX-License-Identifier: MIT
//! The HuC6270 VDC (`vdc/`): instances, no module state.
//!
//! It uses only `machine` constants from the rest of the board, so it can move to a core of its
//! own if PC-FX or a second consumer needs it: broken out, never copied.
//!
//! Differences forced by Rust: the reference's `Vdc`, `Sequencer` and `DmaEngine` are three
//! objects that call each other through references. Here `Sequencer` and `DmaEngine` are the
//! state of each (fields of `Vdc`: `seq`, `dma`) and their methods are `impl Vdc` blocks in
//! `sequencer.rs` and `dma.rs`, with the reference's names behind a `seq_` or `dma_` prefix.

pub mod background;
pub mod chip;
pub mod dma;
pub mod regs;
pub mod sequencer;
pub mod sprites;

pub use chip::{Vdc, VRAM_WORDS};
pub use regs::VdcRegisters;
pub use sequencer::VPhase;
