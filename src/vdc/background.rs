// SPDX-License-Identifier: MIT
//! The background layer: one line of 9-bit pixels from the BAT and the character generator
//! (`vdc/background.py`).
//!
//! [hw: pcetech 5.2; mfr: Hudson HuC6270 manual] Tiles are 8x8, 4 bits per pixel, planar: 16 words
//! per tile, word `y` holds bitplanes 0 (low byte) and 1 (high byte) of line `y`, word `y + 8`
//! bitplanes 2 and 3. The BAT starts at VRAM word 0, one word per tile `ppppnnnnnnnnnnnn`: palette
//! and name, no flips and no priority. Only names 0-2047 exist in 64 KiB of VRAM; the rest read as
//! 0 here (U23).
//!
//! The line is drawn in one go when the display width begins, from the scroll offsets taken then
//! (vdc/sequencer.rs; U3, provisional); a VRAM change during the line shows on the next one.
//!
//! Differences forced by Rust: none in behaviour. The reference's per-instance decode cache (a
//! dict that only saves PyPy time) is not carried over; the eight pixels are decoded each time
//! (the fork: from a table, a bitplane byte at a time, into the row itself).

pub const BAT_WIDTHS: [usize; 4] = [32, 64, 128, 128]; // MWR D5-D4 [hw: pcetech 5.1 $09]
pub const BAT_SHIFTS: [u32; 4] = [5, 6, 7, 7];
pub const BAT_HEIGHTS: [i64; 2] = [32, 64]; // MWR D6

/// The fork: one bitplane byte of a tile line as eight nibbles, bit `7 - x` of the byte at bit
/// `4 * x`, so the four planes of a line OR into its eight pixels at once (the base board
/// shifts each pixel's four bits out one at a time).
const SPREAD: [u32; 256] = {
    let mut table = [0u32; 256];
    let mut byte = 0;
    while byte < 256 {
        let mut x = 0;
        while x < 8 {
            if (byte >> (7 - x)) & 1 != 0 {
                table[byte] |= 1 << (4 * x);
            }
            x += 1;
        }
        byte += 1;
    }
    table
};

/// `width` pixels, each `palette << 4 | colour`; colour 0 is transparent.
pub fn row(
    vram: &[u16],
    size: usize,
    mwr: u16,
    x_offset: i64,
    y_offset: i64,
    width: i64,
) -> Vec<u16> {
    let mut out = vec![0; width.max(0) as usize];
    row_into(vram, size, mwr, x_offset, y_offset, &mut out);
    out
}

/// `row`, drawn into `out` (its length is the width).
pub fn row_into(
    vram: &[u16],
    size: usize,
    mwr: u16,
    x_offset: i64,
    y_offset: i64,
    out: &mut [u16],
) {
    let shape = ((mwr >> 4) & 3) as usize;
    let bat_mask = BAT_WIDTHS[shape] - 1;
    let bat_row = (((y_offset >> 3) & (BAT_HEIGHTS[((mwr >> 6) & 1) as usize] - 1)) as usize)
        << BAT_SHIFTS[shape];
    let line = (y_offset & 7) as usize;
    // [hw: pcetech 5.2] MWR D1-D0 = 3 is the one-bitplane-pair mode: D7 picks planes 0/1 or
    // 2/3 and the other pair reads as zero.
    let mut mask = 0x0F;
    if mwr & 3 == 3 {
        mask = if mwr & 0x80 != 0 { 0x0C } else { 0x03 };
    }
    let planes = mask * 0x1111_1111u32; // the mask on every pixel of a tile line
    let mut skip = (x_offset & 7) as u32;
    let mut column = ((x_offset >> 3) as usize) & bat_mask;
    let mut at = 0;
    while at < out.len() {
        let bat = vram[bat_row | column];
        let base = (bat & 0xFFF) as usize * 16 + line;
        let w0 = if base < size { vram[base] } else { 0 };
        let w1 = if base + 8 < size { vram[base + 8] } else { 0 };
        let palette = (bat >> 8) & 0xF0;
        let pixels = (SPREAD[(w0 & 0xFF) as usize]
            | (SPREAD[(w0 >> 8) as usize] << 1)
            | (SPREAD[(w1 & 0xFF) as usize] << 2)
            | (SPREAD[(w1 >> 8) as usize] << 3))
            & planes;
        let mut pixels = pixels >> (4 * skip);
        let take = ((8 - skip) as usize).min(out.len() - at);
        for slot in &mut out[at..at + take] {
            *slot = palette | (pixels & 0x0F) as u16;
            pixels >>= 4;
        }
        at += take;
        skip = 0;
        column = (column + 1) & bat_mask;
    }
}
