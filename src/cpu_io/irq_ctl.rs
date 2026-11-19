// SPDX-License-Identifier: MIT
//! The interrupt controller ($1400-$17FF, mirrored every 4 bytes) (`cpu_io/irq_ctl.py`).
//!
//! [hw: pcetech 2.1] `$1402`: bits 2-0 are disable bits, 1 = disabled (bit 2 timer, bit 1 IRQ1, bit 0
//! IRQ2), readable and writable, bits 7-3 return the I/O buffer. `$1403`: reads the *line* state
//! in bits 2-0 even when masked, upper five bits the buffer; any write acknowledges the timer.
//! `$1400`/`$1401`: writes do nothing, reads return the buffer. The mask only stops the CPU from
//! taking the interrupt; the core gets the raw lines and the mask (`irq_disable`) and decides.
//!
//! Differences forced by Rust: the reference's two callables (acknowledge the timer, tell the
//! CPU the mask changed) are `write`'s return value and the `mask_changed` flag.

use serde::{Deserialize, Serialize};

pub const TIMER: u8 = 0b100;
pub const IRQ1: u8 = 0b010;
pub const IRQ2: u8 = 0b001;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IrqController {
    /// [hw: pcetech 2] power-up: the disable mask is zero
    pub disable: u8,
    pub irq1: bool,
    pub irq2: bool,
    /// `$1402` was written since the board last looked (the reference's `on_mask_change`).
    #[serde(skip)]
    pub mask_changed: bool,
}

impl IrqController {
    pub fn new() -> IrqController {
        IrqController::default()
    }

    pub fn reset(&mut self) {
        self.disable = 0;
        self.irq1 = false;
        self.irq2 = false;
    }

    pub fn status(&self, timer_irq: bool) -> u8 {
        (if timer_irq { TIMER } else { 0 })
            | (if self.irq1 { IRQ1 } else { 0 })
            | (if self.irq2 { IRQ2 } else { 0 })
    }

    pub fn read(&self, offset: u32, buffer: u8, timer_irq: bool) -> u8 {
        if offset & 2 == 0 {
            return buffer;
        }
        let low = if offset & 1 == 0 {
            self.disable
        } else {
            self.status(timer_irq)
        };
        low | (buffer & 0xF8)
    }

    /// True when the write acknowledges the timer (any write to `$1403`).
    pub fn write(&mut self, offset: u32, value: u8) -> bool {
        if offset & 3 == 2 {
            self.disable = value & 7;
            self.mask_changed = true;
        } else if offset & 3 == 3 {
            return true;
        }
        false
    }
}
