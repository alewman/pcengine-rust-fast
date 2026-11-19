// SPDX-License-Identifier: MIT
//! The HuC6270's register file and the fields the timing generator reads (`vdc/regs.py`).
//!
//! [hw: pcetech 5.1; mfr: Hudson HuC6270 manual] Registers are 16 bits, written a byte at a time.
//! Valid register numbers are $00-$02 and $05-$13; the rest do nothing.

use serde::{Deserialize, Serialize};

// Status port bits (read of $0). [hw: pcetech 5]
pub const BSY: u8 = 0x40;
pub const VD: u8 = 0x20; // vertical blank
pub const DV: u8 = 0x10; // VRAM -> VRAM DMA done
pub const DS: u8 = 0x08; // VRAM -> SAT DMA done
pub const RR: u8 = 0x04; // raster compare
pub const OR: u8 = 0x02; // sprite overflow
pub const CR_FLAG: u8 = 0x01; // sprite 0 collision
pub const IRQ_BITS: u8 = 0x3F;

// Control register enable bits.
pub const CR_COLLISION_IRQ: u16 = 0x01;
pub const CR_OVERFLOW_IRQ: u16 = 0x02;
pub const CR_RASTER_IRQ: u16 = 0x04;
pub const CR_VBLANK_IRQ: u16 = 0x08;
pub const CR_SPRITES: u16 = 0x40;
pub const CR_BACKGROUND: u16 = 0x80;

/// [hw: pcetech 5.1 register $05] CR bits 12-11 select how much MAWR/MARR advance per access.
pub const INCREMENTS: [u16; 4] = [1, 32, 64, 128];

/// $00-$02 and $05-$13.
pub fn valid_register(register: u8) -> bool {
    matches!(register, 0x00..=0x02 | 0x05..=0x13)
}

pub const MAWR: usize = 0x00;
pub const MARR: usize = 0x01;
pub const VRW: usize = 0x02;
pub const CR: usize = 0x05;
pub const RCR: usize = 0x06;
pub const BXR: usize = 0x07;
pub const BYR: usize = 0x08;
pub const MWR: usize = 0x09;
pub const HSR: usize = 0x0A;
pub const HDR: usize = 0x0B;
pub const VPR: usize = 0x0C;
pub const VDW: usize = 0x0D;
pub const VCR: usize = 0x0E;
pub const DCR: usize = 0x0F;
pub const SOUR: usize = 0x10;
pub const DESR: usize = 0x11;
pub const LENR: usize = 0x12;
pub const SATB: usize = 0x13;

/// Raw 16-bit register values, with the fields the sequencer uses as methods.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VdcRegisters {
    pub select: u8,
    pub value: [u16; 0x20],
    /// [hw: pcetech 5.1] The read buffer is indeterminate on power-up, "usually $FFFF".
    pub read_buffer: u16,
    pub write_latch: u8,
}

impl Default for VdcRegisters {
    fn default() -> Self {
        VdcRegisters::new()
    }
}

impl VdcRegisters {
    pub fn new() -> VdcRegisters {
        VdcRegisters {
            select: 0,
            value: [0; 0x20],
            read_buffer: 0xFFFF,
            write_latch: 0,
        }
    }

    pub fn set_byte(&mut self, register: usize, byte: u8, msb: bool) {
        let old = self.value[register];
        if msb {
            self.value[register] = (old & 0x00FF) | ((byte as u16) << 8);
        } else {
            self.value[register] = (old & 0xFF00) | byte as u16;
        }
    }

    // -- fields ---------------------------------------------------------------------------------

    #[inline]
    pub fn cr(&self) -> u16 {
        self.value[CR]
    }

    #[inline]
    pub fn increment(&self) -> u16 {
        INCREMENTS[((self.value[CR] >> 11) & 3) as usize]
    }

    #[inline]
    pub fn rcr(&self) -> i64 {
        (self.value[RCR] & 0x3FF) as i64
    }

    #[inline]
    pub fn mwr(&self) -> u16 {
        self.value[MWR]
    }

    #[inline]
    pub fn hds(&self) -> i64 {
        ((self.value[HSR] >> 8) & 0x7F) as i64
    }

    #[inline]
    pub fn hdw(&self) -> i64 {
        (self.value[HDR] & 0x7F) as i64
    }

    #[inline]
    pub fn hde(&self) -> i64 {
        ((self.value[HDR] >> 8) & 0x7F) as i64
    }

    #[inline]
    pub fn vsw(&self) -> i64 {
        (self.value[VPR] & 0x1F) as i64
    }

    #[inline]
    pub fn vds(&self) -> i64 {
        ((self.value[VPR] >> 8) & 0xFF) as i64
    }

    #[inline]
    pub fn vdw(&self) -> i64 {
        (self.value[VDW] & 0x1FF) as i64
    }

    #[inline]
    pub fn vcr(&self) -> i64 {
        (self.value[VCR] & 0xFF) as i64
    }
}
