// SPDX-License-Identifier: MIT
//! The HuC6280's on-die peripherals that are not the CPU: timer, $1400 interrupt
//! controller, I/O port and the I/O buffer (`cpu_io/__init__.py`; Aubrey's rule: in the board,
//! behind a module boundary).
//!
//! Nothing here uses the rest of the crate except the `sockets` traits, so the module can move
//! into a core or a repository of its own if a second consumer appears (a Data East arcade
//! board's HuC6280). Never copied.
//!
//! `CpuIo` answers the hardware-page offsets $0800-$17FF: the PSG range is forwarded to an
//! object the board supplies, so this module does not use the PSG.
//!
//! Differences forced by Rust: the PSG and the CD-sense level are arguments of each access
//! (the reference holds a reference and a callable); a change of the disable mask is left in
//! `IrqController::mask_changed` for the board to hand the CPU when the access returns (the
//! reference calls back from inside the write; nothing touches the CPU in between).

pub mod io_buffer;
pub mod io_port;
pub mod irq_ctl;
pub mod timer;

pub use io_buffer::IoBuffer;
pub use io_port::IoPort;
pub use irq_ctl::IrqController;
pub use timer::Timer;

use crate::sockets::ControllerPort;

/// What CpuIo needs from the sound generator: its register writes.
pub trait PsgPins {
    fn write(&mut self, offset: u32, value: u8, mclk: i64);
}

pub struct CpuIo {
    pub buffer: IoBuffer,
    pub timer: Timer,
    pub irq: IrqController,
    pub port: IoPort,
}

impl CpuIo {
    pub fn new(country_pc_engine: bool, pad: Option<Box<dyn ControllerPort>>) -> CpuIo {
        CpuIo {
            buffer: IoBuffer::new(0),
            timer: Timer::new(),
            irq: IrqController::new(),
            port: IoPort::new(country_pc_engine, pad),
        }
    }

    pub fn reset(&mut self) {
        self.buffer.reset();
        self.timer.reset();
        self.irq.reset();
        self.port.reset();
    }

    pub fn catch_up(&mut self, mclk: i64) {
        self.timer.catch_up(mclk);
    }

    pub fn next_event(&self) -> Option<i64> {
        self.timer.next_event()
    }

    pub fn tiq(&self) -> bool {
        self.timer.irq
    }

    /// `offset` is the hardware-page offset, $0800-$17FF. Loads the buffer, as pcetech 3.
    pub fn read(&mut self, offset: u32, mclk: i64, cd_attached: bool) -> u8 {
        let offset = offset & 0x1FFF;
        if offset < 0x0C00 {
            // PSG: every address returns the buffer in full
            return self.buffer.value;
        }
        let value = if offset < 0x1000 {
            self.timer.read(self.buffer.value, mclk)
        } else if offset < 0x1400 {
            self.port.read(cd_attached)
        } else {
            self.timer.catch_up(mclk);
            self.irq.read(offset, self.buffer.value, self.timer.irq)
        };
        self.buffer.value = value;
        value
    }

    pub fn write(&mut self, offset: u32, value: u8, mclk: i64, psg: &mut impl PsgPins) {
        let offset = offset & 0x1FFF;
        self.buffer.value = value;
        if offset < 0x0C00 {
            psg.write(offset & 0x0F, value, mclk);
        } else if offset < 0x1000 {
            self.timer.write(offset, value, mclk);
        } else if offset < 0x1400 {
            self.port.write(value);
        } else {
            self.timer.catch_up(mclk);
            if self.irq.write(offset, value) {
                self.timer.acknowledge();
            }
        }
    }

    /// A read with no side effects, for the debugger and the lockstep probe.
    pub fn peek(&mut self, offset: u32, cd_attached: bool) -> u8 {
        let offset = offset & 0x1FFF;
        let buf = self.buffer.value;
        if offset < 0x0C00 {
            return buf;
        }
        if offset < 0x1000 {
            return self.timer.count | (buf & 0x80);
        }
        if offset < 0x1400 {
            return self.port.read(cd_attached);
        }
        self.irq.read(offset, buf, self.timer.irq)
    }
}
