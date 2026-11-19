
use serde::{Deserialize, Serialize};

pub const TICK_CLOCKS: i64 = 1024 * 3;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timer {
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
