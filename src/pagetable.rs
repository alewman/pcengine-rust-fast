// SPDX-License-Identifier: MIT
//! One decode for every bus master: the 8192-entry page table (`pagetable.py`).
//!
//! A page is 256 bytes of the 21-bit physical space. Each page has a handler; the board fills
//! the base map (RAM, the hardware page, an empty slot answering $FF) and a card or expansion
//! device *claims* ranges of pages, which replaces the base handler there, as the hardware does
//! bank by bank (a Super System Card's RAM overrides the ROM mirror). There is no second bus master
//! on a PC Engine, so the CPU's data and fetch accesses, `ST0`-`ST2`, block transfers, the
//! debugger's peek and the lockstep probe all go through here.
//!
//! Differences forced by Rust: a handler is a name (`Handler`), not an object, and the access
//! itself is the board's (`Board::page_read`, `Board::page_write`), which owns what each name
//! stands for. `release` compares names where the reference compares identities.

use serde::{Deserialize, Serialize};

use crate::machine::{ADDRESS_MASK, PAGE_BITS, PAGE_COUNT};

/// Who answers a page.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Handler {
    /// [hw: pcetech 3] Unused banks always return $FF; writes do nothing.
    Unmapped,
    /// Work RAM.
    Ram,
    /// The first VDC, $FF:0000-03FF (hardware_page.rs).
    Vdc,
    /// The VCE, $FF:0400-07FF.
    Vce,
    /// PSG, timer, I/O port, interrupt registers: $FF:0800-17FF.
    CpuIo,
    /// The card in the slot.
    Card,
    /// The expansion port's device.
    Port,
    /// The pixel path (the SuperGrafx's VPC decodes the VDC pages).
    PixelPath,
}

/// Work RAM: `size` bytes mirrored through the pages it is claimed on.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ram {
    pub data: Vec<u8>,
    mask: usize, // sizes are powers of two (8 KiB, 32 KiB)
}

impl Ram {
    pub fn new(size: usize) -> Ram {
        Ram {
            data: vec![0; size],
            mask: size - 1,
        }
    }

    #[inline]
    pub fn read(&self, address: u32) -> u8 {
        self.data[address as usize & self.mask]
    }

    #[inline]
    pub fn write(&mut self, address: u32, value: u8) {
        self.data[address as usize & self.mask] = value;
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PageTable {
    base: Vec<Handler>,
    current: Vec<Handler>,
}

impl Default for PageTable {
    fn default() -> Self {
        PageTable::new()
    }
}

impl PageTable {
    pub fn new() -> PageTable {
        PageTable {
            base: vec![Handler::Unmapped; PAGE_COUNT],
            current: vec![Handler::Unmapped; PAGE_COUNT],
        }
    }

    // -- construction ---------------------------------------------------------------------------

    /// The board's own map. Replaces whatever is there, claims included.
    pub fn set_base(&mut self, first_page: usize, last_page: usize, handler: Handler) {
        for page in first_page..=last_page {
            self.base[page] = handler;
            self.current[page] = handler;
        }
    }

    /// A device takes a range of pages from the base map.
    pub fn claim(
        &mut self,
        first_page: usize,
        last_page: usize,
        handler: Handler,
    ) -> Result<(), String> {
        if first_page > last_page || last_page >= PAGE_COUNT {
            return Err(format!(
                "page range {first_page:#x}-{last_page:#x} outside 0-{PAGE_COUNT:#x}"
            ));
        }
        self.current[first_page..=last_page].fill(handler);
        Ok(())
    }

    /// Give pages back to the base map, where `handler` still holds them.
    pub fn release(&mut self, first_page: usize, last_page: usize, handler: Handler) {
        for page in first_page..=last_page {
            if self.current[page] == handler {
                self.current[page] = self.base[page];
            }
        }
    }

    #[inline]
    pub fn handler(&self, address: u32) -> Handler {
        self.current[((address & ADDRESS_MASK) >> PAGE_BITS) as usize]
    }
}
