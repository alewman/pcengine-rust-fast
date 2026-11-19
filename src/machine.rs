// SPDX-License-Identifier: MIT
// HSYNC_CLOCKS and LINE_CLOCKS cite MAME's huc6260.cpp (BSD-3-Clause, copyright Wilbert Pol;
// notice in NOTICE.md): values from its comment, no code.
//! The machine table and the clock constants every module shares (`machine.py`).
//!
//! One board, a table of rows: the PC Engine and the TurboGrafx-16 differ by a pin
//! (the reference's docs/start-here.md). Add-ons (SuperGrafx, CD) add rows or pin this board;
//! they do not fork it.
//!
//! Rust: Python's integer `//` and float `round()` are spelled out here (`py_floordiv`,
//! `round_ties_even`) for the modules that need the reference's arithmetic.
/// [hw: pcetech 2.4] 21.47727 MHz master clock (6 x NTSC colour burst).
pub const MASTER_HZ: i64 = 21_477_272;

/// [emu: MAME huc6260.cpp (BSD-3-Clause) "A full line lasts 1365 master cycles"; doc: wiki
/// "Timing" (the dot clocks as master / 4, 3, 2)] a line is 1365 master clocks whatever the dot
/// clock (C5: pcetech's "roughly 342 dots" at 5 MHz is 1365 / 4 = 341.25).
pub const LINE_CLOCKS: i64 = 1365;

/// [emu: MAME huc6260.cpp (BSD-3-Clause, Wilbert Pol) "HSync is low for 237 and high for 1128
/// master cycles"] the VCE's horizontal sync is 237 master clocks at the start of a line. Emulator
/// tier, permissive source; no document or hardware measurement here gives the width (U19).
pub const HSYNC_CLOCKS: i64 = 237;

/// [hw: pcetech 7] "3 lines for the sync area", the last three lines of a frame; [emu: MAME
/// huc6260.cpp "VSync is low for 4095 master cycles (3 lines)"].
pub const VSYNC_LINES: i64 = 3;

/// [hw: pcetech 2.4] CPU clock is master/3 (high) or master/12 (low); power-up is low.
pub const CPU_CLOCKS_HIGH: i64 = 3;
pub const CPU_CLOCKS_LOW: i64 = 12;

/// [hw: pcetech 2.2] the timer counts once per 1024 high-speed CPU cycles, whatever the CPU speed.
pub const TIMER_TICK_CLOCKS: i64 = 1024 * 3;

/// [hw: pcetech 7] the frame layout: 14 top blank, 242 active, 4 bottom blank, 3 sync.
pub const TOP_BLANK_LINES: i64 = 14;
pub const ACTIVE_LINES: i64 = 242;

/// [hw: pcetech 7; doc: wiki] 263-line frame; the 262-line form follows VCE bit 2 (C3).
pub const FRAME_LINES_LONG: i64 = 263;
pub const FRAME_LINES_SHORT: i64 = 262;

/// A page of the page table is 256 bytes over the 21-bit space: 8192 pages (docs/sockets.md).
pub const PAGE_BITS: u32 = 8;
pub const PAGE_COUNT: usize = 1 << 13;
pub const ADDRESS_MASK: u32 = (1 << 21) - 1;

/// The hardware page, bank $FF: physical $1FE000-$1FFFFF (pages 0x1FE0-0x1FFF).
pub const HARDWARE_BASE: u32 = 0x1FE000;
pub const HARDWARE_FIRST_PAGE: usize = (HARDWARE_BASE >> PAGE_BITS) as usize;

/// One row of the machine table.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Machine {
    pub name: &'static str,
    /// [hw: pcetech 4] I/O port D6: 1 = PC Engine, 0 = TurboGrafx-16 (US games check it).
    pub country_pc_engine: bool,
    /// 8 KiB of work RAM, mirrored through banks $F8-$FB; the SuperGrafx add-on has 32 KiB.
    pub work_ram_bytes: usize,
    pub vdc_count: usize,
    /// C3: MacDonald calls VCE bit 2 "blur"; Mednafen, ares and Mesen2 take it as 263 lines when
    /// set and 262 when clear; MAME ignores it. True follows the three emulators (tier 5).
    pub lines_follow_vce_bit2: bool,
}

impl Machine {
    /// A row with the reference's defaults (8 KiB, one VDC, lines follow VCE bit 2).
    pub const fn new(name: &'static str, country_pc_engine: bool) -> Machine {
        Machine {
            name,
            country_pc_engine,
            work_ram_bytes: 8 * 1024,
            vdc_count: 1,
            lines_follow_vce_bit2: true,
        }
    }
}

pub const PC_ENGINE: Machine = Machine::new("PC Engine", true);
pub const TURBOGRAFX_16: Machine = Machine::new("TurboGrafx-16", false);

/// Python's `//` on integers: the quotient rounded toward minus infinity.
#[inline]
pub fn py_floordiv(a: i64, b: i64) -> i64 {
    a.div_euclid(b) - if b < 0 && a.rem_euclid(b) != 0 { 1 } else { 0 }
}

/// Python's `round()` of a float: ties to even.
#[inline]
pub fn round_ties_even(value: f64) -> i64 {
    value.round_ties_even() as i64
}
