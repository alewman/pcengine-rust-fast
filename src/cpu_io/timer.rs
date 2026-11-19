// SPDX-License-Identifier: MIT
//! The HuC6280's 7-bit timer ($0C00-$0FFF, mirrored every 2 bytes) (`cpu_io/timer.py`).
//!
//! [hw: pcetech 2.2] One tick per 1024 high-speed CPU cycles (master/3072) whatever the CPU speed.
//! `$0C00` is a latch (0 counts as 1, $7F as 128); `$0C01` bit 0 enables. The interrupt line is
//! asserted when the counter goes from 0 to $7F; at that moment it is reloaded from the latch and
//! keeps counting, whether or not the interrupt has been acknowledged.
//!
//! [mfr: HuC6280 hardware manual 2.7] "When 0 is written in the Timer Start/Stop flag, the timer
//! stops and the prescaler is reset", and the down-counter is loaded from the reload register when the
//! flag goes from 0 to 1. So a tick is one full prescaler period (1024 cycles of master/3, 3072 master
//! clocks) after the enabling write, and a stop and restart always begin a fresh period (U24, closed
//! as far as the sources go: tier 3 for the reset, the full first period follows from it; what stays
//! tier 5 is the window below, and this model otherwise follows pcetech 2.2's "reloaded when it
//! occurs"). The manual also cautions that the
//! count can read invalid while it changes; that is not modelled.
//!
//! C25 (M5 sweep, Battle Royale): the count is visible as $7F for a short window after an underflow
//! before the reload shows. Battle Royale programs a latch of 0 and polls `$0C00` until it reads
//! non-zero, so with an instant reload it never leaves the poll (black screen; both detectors run it).
//! pcetech 2.2's "goes from 0 to $7F" is the same statement. The window is `underflow_window_mclk`,
//! **3 master clocks** (since 2026-11-19): one period of the timer's input clock PPS3, "OSC1 divided by
//! 3" [mfr: HuC6280 hardware manual 2.7 (1)], on the HYPOTHESIS that the 7-bit down-counter shows its
//! borrow ($7F) until the next input clock loads the reload register. **Closed-box measurement**
//! (the reference's docs/restart.md): Battle Royale run on this board with windows of 0, 1, 3, 6, 9, 12, 15, 18, 24,
//! 30, 45 and 60 master clocks against Mesen2 b9fa69d and Beetle PCE b96c11e (run, not read) at
//! frames 300 and 600: every window from 1 to 60 gives the same
//! picture (rows 0.992 and 0.607 against Mesen2, 0.996 and 0.607 against Beetle: the known attract
//! "phase" of the M5 record), and 0 the stuck poll; so the referees bound the window only as "at
//! least one master clock", and the value inside that is the hypothesis above. The width is not
//! measured on hardware. (The earlier default of 15 came from reading an emulator's source and was
//! withdrawn.)

use serde::{Deserialize, Serialize};

pub const TICK_CLOCKS: i64 = 1024 * 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timer {
    /// C25: master clocks for which the count reads $7F after an underflow (see the module doc).
    /// The reference's class attribute; a field here (`BoardOptions::timer_underflow_window_mclk`).
    pub underflow_window_mclk: i64,
    /// [hw: pcetech 2] power-up: count 0, disabled
    pub latch: u8,
    pub count: u8,
    pub enabled: bool,
    /// the TIQ line: held until $1403 is written
    pub irq: bool,
    next_tick: i64,
    window_end: i64,
}

impl Default for Timer {
    fn default() -> Self {
        Timer::new()
    }
}

impl Timer {
    pub fn new() -> Timer {
        Timer {
            underflow_window_mclk: 3,
            latch: 0,
            count: 0,
            enabled: false,
            irq: false,
            next_tick: 0,
            window_end: 0,
        }
    }

    pub fn reset(&mut self) {
        self.latch = 0;
        self.count = 0;
        self.enabled = false;
        self.irq = false;
        self.window_end = 0;
    }

    pub fn catch_up(&mut self, mclk: i64) {
        while self.enabled && self.next_tick <= mclk {
            self.next_tick += TICK_CLOCKS;
            if self.count == 0 {
                // 0 -> $7F underflow: assert the line, reload from the latch (pcetech 2.2).
                self.irq = true;
                self.count = self.latch;
                self.window_end = self.next_tick - TICK_CLOCKS + self.underflow_window_mclk;
            } else {
                self.count -= 1;
            }
        }
    }

    /// Master-clock time of the next underflow, or None while disabled.
    pub fn next_event(&self) -> Option<i64> {
        if !self.enabled {
            return None;
        }
        Some(self.next_tick + self.count as i64 * TICK_CLOCKS)
    }

    /// The fork's: the master clock of the next tick of any kind (`catch_up` has nothing to
    /// do before it), or None while disabled.
    #[inline]
    pub(crate) fn next_tick(&self) -> Option<i64> {
        if self.enabled {
            Some(self.next_tick)
        } else {
            None
        }
    }

    pub fn read(&mut self, buffer: u8, mclk: i64) -> u8 {
        self.catch_up(mclk);
        // Bit 7 returns the I/O buffer; the low seven bits are the live count.
        if mclk < self.window_end {
            return 0x7F | (buffer & 0x80); // C25: $7F before the reload shows
        }
        self.count | (buffer & 0x80)
    }

    pub fn write(&mut self, offset: u32, value: u8, mclk: i64) {
        self.catch_up(mclk);
        if offset & 1 == 0 {
            // The latch only; the running count is untouched until the next reload.
            self.latch = value & 0x7F;
        } else {
            let enable = value & 1 != 0;
            if enable && !self.enabled {
                // Disabled -> enabled: reload from the latch, restart the prescaler (U24).
                self.count = self.latch;
                self.next_tick = mclk + TICK_CLOCKS;
            }
            self.enabled = enable;
        }
    }

    /// [hw: pcetech 2.1] any write to $1403 acknowledges the timer.
    pub fn acknowledge(&mut self) {
        self.irq = false;
    }
}
