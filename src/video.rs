//! The frame/line clock and the picture capture (`video.py`).
//!
//! `Timeline` is the VCE's sync generator: it owns the line and frame counters on the board's one
//! master-clock timeline and decides 262 or 263 lines per frame (C3). `Capture` turns each line into
//! a row of 9-bit palette values (`-------GGGRRRBBB`), looking the VDC's pixel indices up in the
//! colour table *as it stood at each dot*, so a mid-line palette write splits the line exactly where
//! the CPU wrote it: that is what Covell's CPU test draws.
//!
//! Compare pictures at the 9-bit values, never at RGB (docs/vce.md).
//!
//! Differences forced by Rust: the `Timeline` and the `Capture` are handed the VCE where they
//! need it (the reference holds a reference); the pixel path is the board's field, which mixes
//! the VDC rows and hands `finish_line` the result; a finished frame is moved into
//! `last_frame` (the reference aliases the row list, which it never touches again).

use serde::{Deserialize, Serialize};

use crate::machine::{
    Machine, ACTIVE_LINES, FRAME_LINES_LONG, FRAME_LINES_SHORT, HSYNC_CLOCKS, LINE_CLOCKS,
    TOP_BLANK_LINES, VSYNC_LINES,
};
use crate::sockets::VideoTiming;
use crate::vce::Vce;

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Timeline {
    lines_follow_vce_bit2: bool,
    pub frame: i64,
    pub frame_lines: i64,
    pub line: i64,
    pub line_start: i64,
}

impl Timeline {
    /// Time zero is the start of line `start_line` of frame 0 (U1: the power-up phase of the
    /// raster against the CPU is not published; MAME's CPU test trace starts at line 260).
    /// `start_offset` puts time zero that many master clocks *into* the line instead.
    pub fn new(
        machine: &Machine,
        vce: &Vce,
        start_line: i64,
        start_offset: i64,
    ) -> Result<Timeline, String> {
        let mut timeline = Timeline {
            lines_follow_vce_bit2: machine.lines_follow_vce_bit2,
            frame: 0,
            frame_lines: 0,
            line: start_line,
            line_start: -start_offset,
        };
        timeline.frame_lines = timeline.lines_for_frame(vce);
        if !(0..timeline.frame_lines).contains(&start_line) {
            return Err(format!(
                "start_line {start_line} outside the {}-line frame",
                timeline.frame_lines
            ));
        }
        Ok(timeline)
    }

    fn lines_for_frame(&self, vce: &Vce) -> i64 {
        // C3: sampled where the frame starts, as Mednafen samples it where the vsync ends.
        if self.lines_follow_vce_bit2 && !vce.bit2() {
            return FRAME_LINES_SHORT;
        }
        FRAME_LINES_LONG
    }

    #[inline]
    pub fn next_line_start(&self) -> i64 {
        self.line_start + LINE_CLOCKS
    }

    pub fn in_vsync(&self) -> bool {
        self.line >= self.frame_lines - VSYNC_LINES
    }

    pub fn timing(&self, vce: &Vce) -> VideoTiming {
        VideoTiming {
            frame: self.frame,
            line: self.line,
            start_mclk: self.line_start,
            dot_divider: vce.dot_divider(),
            frame_lines: self.frame_lines,
            in_vsync: self.in_vsync(),
        }
    }

    /// Step to the next line; at the end of a frame start the next and re-read the VCE.
    pub fn advance(&mut self, vce: &Vce) {
        self.line_start += LINE_CLOCKS;
        self.line += 1;
        if self.line >= self.frame_lines {
            self.line = 0;
            self.frame += 1;
            self.frame_lines = self.lines_for_frame(vce);
        }
    }
}

/// The 242 active lines of a frame: `rows[i]` is the 9-bit palette values of line 14 + i.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Frame {
    pub number: i64,
    pub rows: Vec<Vec<u16>>,
    pub dividers: Vec<u8>,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Capture {
    /// the colour table as the line capture has replayed it
    shadow: Vec<u16>,
    rows: Vec<Vec<u16>>,
    dividers: Vec<u8>,
    pub last_frame: Option<Frame>,
}

impl Capture {
    pub fn new(vce: &Vce) -> Capture {
        Capture {
            shadow: vce.palette.clone(),
            rows: Vec::new(),
            dividers: Vec::new(),
            last_frame: None,
        }
    }

    /// Called once the row of a line has ended, with the row the pixel path made of the VDCs'.
    ///
    /// [emu: Mednafen vce.cpp] A row is the 1365 master clocks from the end of the line's
    /// horizontal sync to the end of the next line's: dot 0 is the first dot after the sync, so
    /// a 256-wide picture (HDS 2) starts at dot 32 and fits in the row. This is the same
    /// raster as `timing.start_mclk`, shifted by `HSYNC_CLOCKS`.
    pub fn finish_line(&mut self, timing: &VideoTiming, row: &[u16], vce: &mut Vce) {
        let first = timing.start_mclk + HSYNC_CLOCKS;
        let end = first + LINE_CLOCKS;
        let div = timing.dot_divider;
        let log = &mut vce.palette_log;
        let mut used = 0;
        while used < log.len() && log[used].0 < end {
            used += 1;
        }
        let mut colours = Vec::with_capacity(row.len());
        let mut pending = 0;
        let shadow = &mut self.shadow;
        // The fork: with no palette write inside the row, which is nearly every line, the
        // colour table is the same at every dot and no dot's time is needed.
        let split = if used == 0 { &row[..0] } else { row };
        if used == 0 {
            colours.extend(row.iter().map(|&pixel| {
                // colour 0 of every palette shows the backdrop/overscan entry
                let entry = if pixel & 0x0F == 0 {
                    pixel & 0x100
                } else {
                    pixel
                };
                shadow[entry as usize]
            }));
        }
        for (x, &pixel) in split.iter().enumerate() {
            let when = first + x as i64 * div;
            while pending < used && log[pending].0 <= when {
                shadow[log[pending].1 as usize] = log[pending].2;
                pending += 1;
            }
            let mut pixel = pixel;
            if pixel & 0x0F == 0 {
                // colour 0 of every palette shows the backdrop/overscan entry
                pixel &= 0x100;
            }
            colours.push(shadow[pixel as usize]);
        }
        while pending < used {
            shadow[log[pending].1 as usize] = log[pending].2;
            pending += 1;
        }
        log.drain(..used);
        if timing.line == 0 {
            self.rows = Vec::new();
            self.dividers = Vec::new();
        }
        if (TOP_BLANK_LINES..TOP_BLANK_LINES + ACTIVE_LINES).contains(&timing.line) {
            self.rows.push(colours);
            self.dividers.push(div as u8);
            if self.rows.len() as i64 == ACTIVE_LINES {
                self.last_frame = Some(Frame {
                    number: timing.frame,
                    rows: std::mem::take(&mut self.rows),
                    dividers: std::mem::take(&mut self.dividers),
                });
            }
        }
    }
}
