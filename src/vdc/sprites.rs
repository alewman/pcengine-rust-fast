// SPDX-License-Identifier: MIT
//! Sprites: the line scan of the sprite attribute table, and the drawing of the line's sprites
//! (`vdc/sprites.py`).
//!
//! [hw: pcetech 5.3; mfr: Hudson HuC6270 manual] The 64-entry table (SAT, filled only by the VRAM ->
//! SAT DMA) holds, per sprite: Y (10 bits, the first visible line is 64), X (10 bits, 32 is the
//! display's left edge), pattern (bits 10-1; bit 0 is the CG bit), and flags: palette (3-0), priority
//! (7), width 16/32 (8), flip X (11), height 16/32/64 (13-12), flip Y (15). A pattern is 16 x 16
//! pixels = 64 words, four planes of 16 words. Larger sprites use neighbouring patterns: width 32
//! clears pattern bit 0, height 32 clears bit 1, height 64 clears bits 2 and 1; flips act on the whole
//! sprite.
//!
//! Per line the scan finds the sprites that cover the *next* line, in table order, up to 16 pattern
//! entries (a 32-wide sprite is two entries); a 17th needed entry sets the overflow flag and ends the
//! scan. The first opaque pixel in buffer order wins; a pixel is in front of the background if its
//! priority bit is set or the background pixel is transparent; sprite 0 colliding with another
//! sprite's opaque pixel sets the collision flag. When MWR's sprite access width is 1 the pattern
//! is two bitplanes, picked by the CG bit (SP0/SP1 or SP2/SP3).
//!
//! The scan runs, and reads the pattern words, at the line step before the line is drawn (the
//! blanking in which the manual says the patterns are fetched); how many entries are searched and
//! fetched follows the manual's table (2.4.1 (6), in vdc/chip.rs `fetch_sprites`). Provisional (U3).

use serde::{Deserialize, Serialize};

pub const HEIGHTS: [i64; 4] = [16, 32, 64, 64];
pub const HEIGHT_MASKS: [u32; 4] = [!0, !2, !6, !6];
pub const SPRITE_LIMIT: usize = 16;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    /// left pixel, in display columns (may be negative or past the width)
    pub x: i64,
    /// palette << 4
    pub palette: u16,
    pub priority: bool,
    pub hflip: bool,
    pub sprite0: bool,
    pub planes: [u16; 4],
}

/// The entries of sprites covering `line` (counted from the first display line) among the
/// first `searched` table entries, and whether a 17th entry was needed (overflow).
///
/// Rust: the reference's keyword arguments `limit` (default `SPRITE_LIMIT`) and `searched`
/// (default 64) are plain arguments.
pub fn scan(
    vram: &[u16],
    size: usize,
    sat: &[u16],
    line: i64,
    mwr: u16,
    limit: usize,
    searched: usize,
) -> (Vec<Entry>, bool) {
    let mut entries: Vec<Entry> = Vec::new();
    let cg_two_planes = ((mwr >> 2) & 3) == 1;
    for i in 0..searched.min(64) {
        let (w0, w1, w2, w3) = (sat[4 * i], sat[4 * i + 1], sat[4 * i + 2], sat[4 * i + 3]);
        let y = (w0 & 0x3FF) as i64 - 64;
        let shape = ((w3 >> 12) & 3) as usize;
        let height = HEIGHTS[shape];
        if !(y..y + height).contains(&line) {
            continue;
        }
        let wide = w3 & 0x100 != 0;
        let hflip = w3 & 0x800 != 0;
        let mut row = line - y;
        if w3 & 0x8000 != 0 {
            row = height - 1 - row;
        }
        let row = row as u32;
        let mut pattern = ((w2 as u32 >> 1) & 0x3FF) & HEIGHT_MASKS[shape];
        pattern |= (row & 0x30) >> 3;
        if wide {
            pattern &= !1;
        }
        let base_x = (w1 & 0x3FF) as i64 - 32;
        for half in 0..(if wide { 2u32 } else { 1 }) {
            if entries.len() == SPRITE_LIMIT {
                // [mfr] "17 or more": the rest of the scan is dropped
                entries.truncate(limit);
                return (entries, true);
            }
            let cell = pattern | if wide { half ^ hflip as u32 } else { 0 };
            let address = (cell * 64 + (row & 15)) as usize;
            let planes = if address + 48 >= size {
                // U23/U27, a choice: an empty sprite past the VRAM
                [0, 0, 0, 0]
            } else if cg_two_planes {
                let shift = if w2 & 1 != 0 { 32 } else { 0 };
                [vram[address + shift], vram[address + shift + 16], 0, 0]
            } else {
                [
                    vram[address],
                    vram[address + 16],
                    vram[address + 32],
                    vram[address + 48],
                ]
            };
            entries.push(Entry {
                x: base_x + 16 * half as i64,
                palette: (w3 & 0xF) << 4,
                priority: w3 & 0x80 != 0,
                hflip,
                sprite0: i == 0,
                planes,
            });
        }
    }
    entries.truncate(limit);
    (entries, false)
}

/// Draw the entries over `row` (the line's 9-bit pixels; the display starts at index `first`).
///
/// Returns the display column of the first pixel where sprite 0 hit another sprite, or None.
pub fn draw(entries: &[Entry], width: i64, row: &mut [u16], first: usize) -> Option<i64> {
    let mut buffer = vec![0u16; width.max(0) as usize];
    let mut hit: Option<i64> = None;
    for entry in entries.iter().rev() {
        let [p0, p1, p2, p3] = entry.planes;
        for x in 0..16i64 {
            let column = entry.x + x;
            if !(0..width).contains(&column) {
                continue;
            }
            let bit = if entry.hflip { x } else { 15 - x };
            let raw = ((p0 >> bit) & 1)
                | (((p1 >> bit) & 1) << 1)
                | (((p2 >> bit) & 1) << 2)
                | (((p3 >> bit) & 1) << 3);
            if raw == 0 {
                continue;
            }
            let slot = &mut buffer[column as usize];
            if entry.sprite0 && *slot & 0x0F != 0 && hit.is_none_or(|h| column < h) {
                hit = Some(column);
            }
            *slot = 0x100 | entry.palette | raw | if entry.priority { 0x200 } else { 0 };
        }
    }
    for (column, &value) in buffer.iter().enumerate() {
        if value & 0x0F != 0 {
            let target = first + column;
            if row[target] & 0x0F == 0 || value & 0x200 != 0 {
                row[target] = value & 0x1FF;
            }
        }
    }
    hit
}
