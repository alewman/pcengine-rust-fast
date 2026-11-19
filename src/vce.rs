// SPDX-License-Identifier: MIT
//! The HuC6260 VCE: colour table, control register, dot clock, frame length (`vce.py`).
//!
//! [mfr: Hudson HuC6260 manual; hw: pcetech 6; doc: wiki "VCE pinout", "Timing"] (MacDonald's
//! headings swap the VDC and VCE names: C1.)
//!
//! Registers ($FF:0400-07FF, 8-byte mirror):
//!
//! ```text
//! $0 control (write only)    D7 monochrome, D2 blur / line count (C3), D1-D0 dot clock
//! $2/$3 colour address LSB/MSB (write only; 9 bits)
//! $4/$5 colour data LSB/MSB  (the address advances after the MSB, read or write)
//! ```
//!
//! Everything else reads $FF. The palette is 512 entries of `-------GGGRRRBBB`.
//!
//! Output colour: the board exposes the 9-bit pixel and palette values; turning GRB into RGB is a
//! replaceable table (`rgb_table`), because there is no standard conversion (docs/vce.md). The
//! default is a plain linear 3-bit expansion, which is *not* any emulator's colours.

use serde::{Deserialize, Serialize};

use crate::mt::PythonRandom;

/// Dot clock divider by CR bits 1-0 [hw: pcetech 6, doc: wiki Timing]: 5.37, 7.16, 10.7 MHz.
/// Hudson's manual knows only 4 and 3 (C8); bit 1 set is MacDonald's 10 MHz mode.
pub const DOT_DIVIDERS: [i64; 4] = [4, 3, 2, 2];

/// Entry `GGGRRRBBB` -> (r, g, b), each 3-bit value scaled by 255/7.
pub fn linear_rgb_table() -> Vec<(u8, u8, u8)> {
    (0..512u32)
        .map(|entry| {
            let (g, rd, b) = ((entry >> 6) & 7, (entry >> 3) & 7, entry & 7);
            (
                (rd * 255 / 7) as u8,
                (g * 255 / 7) as u8,
                (b * 255 / 7) as u8,
            )
        })
        .collect()
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vce {
    pub palette: Vec<u16>,
    pub control: u8,
    pub address: u16,
    /// Host-side: not state (a loaded state keeps the system's own table).
    #[serde(skip, default = "linear_rgb_table")]
    pub rgb_table: Vec<(u8, u8, u8)>,
    /// (mclk, entry, value) of every palette change, drained by the line capture.
    pub palette_log: Vec<(i64, u16, u16)>,
}

impl Vce {
    pub fn new(power_on: Option<&mut PythonRandom>) -> Vce {
        // U7: zeros by default, random on request.
        let (palette, control, address) = match power_on {
            None => (vec![0; 512], 0, 0),
            Some(rng) => {
                let palette = (0..512).map(|_| rng.getrandbits(9) as u16).collect();
                let control = rng.getrandbits(8) as u8;
                let address = rng.getrandbits(9) as u16;
                (palette, control, address)
            }
        };
        Vce {
            palette,
            control,
            address,
            rgb_table: linear_rgb_table(),
            palette_log: Vec::new(),
        }
    }

    // -- derived --------------------------------------------------------------------------------

    #[inline]
    pub fn dot_divider(&self) -> i64 {
        DOT_DIVIDERS[(self.control & 3) as usize]
    }

    pub fn monochrome(&self) -> bool {
        self.control & 0x80 != 0
    }

    pub fn bit2(&self) -> bool {
        self.control & 0x04 != 0
    }

    pub fn rgb(&self, entry: usize) -> (u8, u8, u8) {
        self.rgb_table[self.palette[entry & 0x1FF] as usize]
    }

    // -- the CPU's port -------------------------------------------------------------------------

    pub fn read(&mut self, offset: u32, _mclk: i64) -> u8 {
        let reg = offset & 7;
        if reg == 4 {
            return (self.palette[self.address as usize] & 0xFF) as u8;
        }
        if reg == 5 {
            let value = 0xFE | ((self.palette[self.address as usize] >> 8) & 1) as u8;
            self.address = (self.address + 1) & 0x1FF;
            return value;
        }
        0xFF
    }

    pub fn write(&mut self, offset: u32, value: u8, mclk: i64) {
        let reg = offset & 7;
        let value = value as u16;
        match reg {
            0 => self.control = value as u8,
            2 => self.address = (self.address & 0x100) | value,
            3 => self.address = (self.address & 0x0FF) | ((value & 1) << 8),
            4 => self.set_entry((self.palette[self.address as usize] & 0x100) | value, mclk),
            5 => {
                self.set_entry(
                    (self.palette[self.address as usize] & 0x0FF) | ((value & 1) << 8),
                    mclk,
                );
                self.address = (self.address + 1) & 0x1FF;
            }
            _ => {}
        }
    }

    fn set_entry(&mut self, value: u16, mclk: i64) {
        self.palette[self.address as usize] = value;
        self.palette_log.push((mclk, self.address, value));
    }
}
