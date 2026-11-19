// SPDX-License-Identifier: MIT
//! The hardware page, bank $FF ($1FE000-$1FFFFF), as 32 pages of 256 bytes (`hardware_page.py`).
//!
//! [hw: pcetech 3]
//!
//! ```text
//! $0000-$03FF  VDC   (4-byte mirror)       $0800-$0BFF  PSG   (in the CPU)
//! $0400-$07FF  VCE   (8-byte mirror)       $0C00-$0FFF  timer (2-byte mirror)
//! $1000-$13FF  I/O port                    $1400-$17FF  interrupt controller (4-byte mirror)
//! $1800-$1FFF  unmapped, reads $FF
//! ```
//!
//! The pages $1800-$1FFF are free for an add-on to claim (`CD` at $1800-$18FF, the Arcade Card at
//! $1A00-$1AFF); the page table's claim replaces these handlers.
//!
//! Differences forced by Rust: the reference's three page objects (`VdcPage`, `VcePage`,
//! `CpuIoPage`) are the names `Handler::Vdc`, `Handler::Vce` and `Handler::CpuIo`; the address
//! masks they apply (`& 3`, `& 7`, `& 0x1FFF`) are in `Board::page_read` and `page_write`.

use crate::machine::HARDWARE_FIRST_PAGE;
use crate::pagetable::{Handler, PageTable};

/// Install the board's own handlers on the 32 pages of bank $FF.
pub fn map_hardware_page(pages: &mut PageTable) {
    let first = HARDWARE_FIRST_PAGE;
    pages.set_base(first, first + 0x03, Handler::Vdc);
    pages.set_base(first + 0x04, first + 0x07, Handler::Vce);
    pages.set_base(first + 0x08, first + 0x17, Handler::CpuIo);
}
