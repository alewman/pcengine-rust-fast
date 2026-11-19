// SPDX-License-Identifier: MIT
//! The I/O port ($1000-$13FF, one register) and what answers on the pad pins
//! (`cpu_io/io_port.py`).
//!
//! [hw: pcetech 4] Reads: D7 CD-unit sense (1 = not attached; C6 records sgxtech's opposite
//! sentence), D6 country (1 = PC Engine, 0 = TurboGrafx-16), D5-D4 always 1, D3-D0 the pad pins.
//! Writes: D1 = CLR, D0 = SEL. The port never returns buffer bits, but a read does load the buffer.
//!
//! [mfr: HuC6280 hardware manual 2.5, 2.6] The CPU's output port (port O, O0-O7) is a latch that
//! reads back nothing and is driven high by a system reset, so SEL and CLR both power up *high*;
//! the eight bits are kept because the SuperGrafx's S-EXP port uses O2-O7. Port K (K0-K7) is the
//! input the pad pins and the CD sense reach.

use crate::sockets::ControllerPort;

pub struct IoPort {
    pub country_pc_engine: bool,
    pub device: Option<Box<dyn ControllerPort>>,
    /// [mfr: HuC6280 manual 2.5] "H level is output to the output port"
    pub latch: u8,
}

impl IoPort {
    pub fn new(country_pc_engine: bool, device: Option<Box<dyn ControllerPort>>) -> IoPort {
        IoPort {
            country_pc_engine,
            device,
            latch: 0xFF,
        }
    }

    pub fn sel(&self) -> bool {
        self.latch & 1 != 0
    }

    pub fn clr(&self) -> bool {
        self.latch & 2 != 0
    }

    pub fn reset(&mut self) {
        self.latch = 0xFF;
    }

    pub fn read(&mut self, cd_attached: bool) -> u8 {
        // Pins pulled up on an empty port: every pad bit reads 1 (docs: controller-info
        // "all inputs default to high"; pcetech: unconnected controllers return $0F).
        let (sel, clr) = (self.sel(), self.clr());
        let nibble = match self.device.as_mut() {
            None => 0x0F,
            Some(device) => device.read(sel, clr) & 0x0F,
        };
        let mut value = 0x30 | nibble;
        if !cd_attached {
            value |= 0x80;
        }
        if self.country_pc_engine {
            value |= 0x40;
        }
        value
    }

    pub fn write(&mut self, value: u8) {
        self.latch = value;
        let (sel, clr) = (self.sel(), self.clr());
        if let Some(device) = self.device.as_mut() {
            device.write(sel, clr);
        }
    }
}
