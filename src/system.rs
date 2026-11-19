// SPDX-License-Identifier: MIT
//! The machine as a whole: a board, a CPU on it, and frame-by-frame running (`system.py`).
//!
//! The CPU is huc6280-rust's `HuC6280` with the board as its bus: one `step()` runs one
//! instruction, one interrupt entry or one whole block transfer. The board prices every cycle
//! inside its callbacks, so `step()`'s return value is never added to the clock.
//!
//! Differences forced by Rust: the CPU owns the board (`system.cpu.bus`, also
//! `system.board()`), where the reference's `System` holds both side by side; `System::new` is
//! the reference's `build` (`make_core`, then `Board.attach_cpu`). A CPU with other switches is
//! the same type with its public fields set (`system.cpu.irq_poll_cycle = ...`), not a subclass.

use huc6280::HuC6280;

use crate::board::Board;

pub struct System {
    pub cpu: HuC6280<Board>,
}

impl System {
    /// The HuC6280 core on this board's bus, with a reset requested: the first `step()` runs
    /// the start sequence and reads the reset vector.
    pub fn new(board: Board) -> System {
        let mut cpu = HuC6280::new(board);
        cpu.request_reset();
        // Board.attach_cpu
        let disable = cpu.bus.cpu_io.irq.disable;
        cpu.pins.set_irq_disable(disable);
        cpu.bus.deliver_lines(Some(&mut cpu.pins), true);
        System { cpu }
    }

    #[inline]
    pub fn board(&self) -> &Board {
        &self.cpu.bus
    }

    #[inline]
    pub fn board_mut(&mut self) -> &mut Board {
        // the fork: whatever the caller changes, the event horizon is recomputed
        self.cpu.bus.touch();
        &mut self.cpu.bus
    }

    #[inline]
    pub fn frame(&self) -> i64 {
        self.cpu.bus.timeline.frame
    }

    pub fn step(&mut self) {
        self.cpu.bus.touch();
        self.cpu.step();
    }

    /// Run until the VCE starts the next frame.
    pub fn run_frame(&mut self) {
        self.cpu.bus.touch();
        let start = self.frame();
        while self.frame() == start {
            self.cpu.step();
        }
    }

    /// Run whole instructions until the master clock has reached `mclk`.
    pub fn run_until(&mut self, mclk: i64) {
        self.cpu.bus.touch();
        while self.cpu.bus.mclk < mclk {
            self.cpu.step();
        }
    }
}
