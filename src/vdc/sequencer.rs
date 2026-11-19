// SPDX-License-Identifier: MIT
// Behaviour informed by MAME's src/devices/video/huc6270.cpp and huc6260.cpp, BSD-3-Clause,
// copyright Wilbert Pol (notice in NOTICE.md): read by the reference, not transcribed. This file
// transcribes the reference's vdc/sequencer.py of the clean rewrite of 2026-11-19 (pcengine-python
// 3318027; docs/restart.md here), written without the withdrawn module or any GPL source.
//! The HuC6270's line machine: the horizontal and vertical periods on the board's master clock
//! (`vdc/sequencer.py`).
//!
//! Sources, in the order they decide a question:
//!     [mfr] Hudson, HuC6270 CMOS Video Display Controller Manual: registers HSR, HDR, VPR, VDW, VCR,
//!           RCR, BXR, BYR, CR (SB/BB, burst mode), MWR, the status bits, Fig. 2-2-1, 2.4.1 (6).
//!     [hw]  Charles MacDonald, pcetech.txt sections 5.1, 5.4 and 7 (tested on hardware).
//!     [emu] MAME huc6270.cpp / huc6260.cpp (BSD-3-Clause): an emulator, used where the two
//!           documents say nothing, and named where it is used.
//!     [fit] Covell's CPU-test photographs (tier 1) through `vblank_dots`.
//!
//! **The line** (the board's frame: a line starts when the VCE's horizontal sync falls):
//!
//! ```text
//!     0 ........ HSYNC_CLOCKS ......... display start .......... display end ........ 1365
//!     |  sync    | HDS+1 characters       | HDW+1 characters       | HDE+1, HSW+1 ...  |
//! ```
//!
//! - The VCE drives /HSYN and /VSYN [doc: wiki VDC pinout, "the VDC synchronizes its video output"];
//!   the sync is low for the first `HSYNC_CLOCKS` = 237 master clocks of a line [emu: MAME
//!   huc6260.cpp "HSync is low for 237 and high for 1128 master cycles"].
//! - **Line step**, at the end of the sync (the sync's rising edge) [emu: MAME: the VDC's line
//!   begins there; mfr: HDS is counted "from the rising edge of a horizontal sync signal"]: the
//!   vertical period counts one line, the raster counter and the vertical scroll counter count one,
//!   and on a display line the sprites of this line are fetched (2.4.1 (6): fetched during the
//!   horizontal blanking that precedes the line's display).
//! - **Display start** (HDS+1) characters of 8 dots after the line step [mfr: HSR, Fig. 2-2-1];
//!   each field is read at the start of its own period [mfr: HSR/HDR "valid at the beginning of
//!   each period"]. The display lasts HDW+1 characters, cut at the next line step if longer [emu:
//!   MAME: the sync's end restarts the horizontal periods]. The scroll registers and CR's SB/BB are
//!   taken before the display starts: CR's SB/BB at the display start [mfr: "valid from the next
//!   horizontal display period"], BXR and the vertical scroll counter `SCROLL_LATCH_DOTS` = 16 dots
//!   (two characters) before it. MAME takes BXR at the display start; that drew one line wrong in
//!   Bomberman '94, and a **closed-box measurement** (the reference's docs/restart.md; Bomberman '94
//!   and After Burner II at frame 300 against Mesen2 b9fa69d and Beetle PCE b96c11e) gives both
//!   referees' pictures with the scroll taken 12 or 16 dots early, not 0, 8, 24 or 40: FITTED,
//!   emulator tier, provisional; the reason offered (the first character's fetch precedes its
//!   pixels) is a HYPOTHESIS. The line is drawn at the display start (vdc/chip.rs `render_line`).
//! - **Raster compare** as the sync falls, against the counter as the previous line's step left
//!   it [emu: MAME compares at the falling edge]. The counter is set to $40 at the step of the line
//!   before the display period [mfr: RCR "set to '64' at the previous scanning line of start timing
//!   of display period and is incremented for each scan"], so RCR $40 interrupts as the first
//!   display line begins [hw: pcetech 5.1, $40 = line 25, $146 = line 24 of the next frame]. Where
//!   in the line no document says. A **closed-box measurement** (the reference's docs/restart.md)
//!   found no single place that suits both After Burner II (identical to both referees at 0 of the
//!   offsets tried, 0-500 master clocks after the sync falls, and at the previous display's end)
//!   and Bomberman '94 (identical from 40 to 333): MAME's place is kept, After Burner II being one
//!   of the 18 gate titles; Bomberman '94 was settled instead by the scroll latch below
//!   (provisional).
//!
//! **The frame.** The vertical periods follow the manual's encodings, counted in line steps from the
//! line on which the vertical sync falls (the board calls `vsync` as line `frame_lines - 3` begins):
//! VSW+1 lines of sync, VDS+2 lines to the display, VDW+1 display lines [mfr: VPR, VDW], then the
//! front porch: VCR+3 lines [hw: pcetech 7: "3 blanked lines ... for as many lines as are specified
//! in VCR"; the manual's "VCR should be N" disagrees by the three lines: C-entry, pcetech taken].
//! After the front porch the VDC starts its own sync (VSW) without waiting for the VCE [hw: pcetech
//! 7, "the display counter resets"]; the VCE's vertical sync restarts VSW wherever it lands. Each
//! field is read when its period begins [mfr: VPR "at the beginning of each period", VDW "at the
//! beginning of a vertical display period", VCR "at the beginning of a vertical front porch"].
//!
//! - **Vertical blanking** begins on the first line after the display period [hw: pcetech 5, status
//!   bit 5]. Its time inside that line is `vblank_dots` dots after the line step, -16 by default
//!   (before it: FITTED to Covell's photographs, tier 1, U19: the place the earlier default of -8 held
//!   against the photographs before the display moved 8 dots nearer the sync; -18 fits them best,
//!   the reference's docs/restart.md), or the display start of that
//!   line with `vblank_dots=None` [emu: MAME's comment "VBlank IRQ happens at the beginning of HDW
//!   period after VDW ends"]. Then: the VD flag, the VRAM -> SAT transfer [mfr: "at the beginning of
//!   a vertical blanking period"] and the MWR fields that change only then [mfr: MWR VM/SM/SCREEN].
//! - A display period that the VCE's vertical sync cuts short has its vertical blanking on the next
//!   line [hw: pcetech 5: with more than 261 display lines "the interrupt will always occur on line
//!   261", the line after the sync begins on line 260].
//! - **Burst mode** is taken when the VDC's frame begins (its VSW): SB = BB = 0 then holds the frame
//!   in burst mode [mfr: CR "burst mode will start with the next frame ... cleared in next frame";
//!   hw: pcetech 5.1, "if bits 7 and 6 are cleared by the time the next frame starts", with his
//!   note that it may be the display start instead: provisional].
//!
//! Power-on (U7-like, a choice): the registers are zero and the vertical counter runs from VSW, so
//! the VDC free-runs on its own short frames until the first vertical sync arrives.
//!
//! Rust: the reference's `Sequencer` holds its chip; here it is the state (`Vdc::seq`) and its
//! methods are an `impl Vdc` block with a `seq_` prefix (vdc/mod.rs). Event names are an enum,
//! `Event`, not strings; the queue is a vector kept sorted by (time, order) as the reference's
//! list of tuples is (the order is unique, so the name is never compared).

use serde::{Deserialize, Serialize};

use crate::machine::{HSYNC_CLOCKS, LINE_CLOCKS};
use crate::vdc::chip::Vdc;
use crate::vdc::regs as r;

/// [mfr: Fig. 2-2-1] the horizontal fields count characters of 8 dots
pub const CHARACTER_DOTS: i64 = 8;
/// [hw: pcetech 7] the front porch is VCR + 3 lines
pub const FRONT_PORCH_EXTRA_LINES: i64 = 3;
/// Master clocks from the sync's fall to the raster compare [emu: MAME: 0]; an experiment's knob
/// (the reference's docs/restart.md: no single value fits both After Burner II and Bomberman '94).
pub const RASTER_COMPARE_CLOCKS: i64 = 0;
/// Dots before the display start at which the scroll registers are taken (closed-box fit, module
/// doc): two characters, the fetch of the first character having to precede its pixels.
pub const SCROLL_LATCH_DOTS: i64 = 16;

/// The vertical periods, in the manual's order [mfr: Fig. 2-2-1].
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum VPhase {
    /// vertical sync
    Vsw = 0,
    /// to the display ("vertical back porch")
    Vds = 1,
    /// the display period
    Vdw = 2,
    /// the front porch
    Vcr = 3,
}

/// The reference's event names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    Step,
    Latch,
    Display,
    Raster,
    Vblank,
    Collision,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sequencer {
    pub vblank_dots: Option<i64>,
    /// master clocks per dot on this line (the VCE's, read at the line start)
    pub divider: i64,
    /// master-clock time at which this line's sync fell
    pub origin: i64,
    pub vphase: VPhase,
    /// line steps left in the vertical period (power-on: VSW = 0 + 1)
    pub lines_left: i64,
    /// the period began at this line's start: its step counts it
    vsync_this_line: bool,
    /// raster counter - $40; out of range until the first display period
    pub rcr_count: i64,
    rcr_compared: i64,
    /// display lines begun in this display period, less one
    pub display_count: i64,
    pub burst_mode: bool,
    /// CR as the display start of this line took it
    pub cr_cache: u16,
    /// this line draws (display period, not burst mode)
    pub display_line: bool,
    /// alias kept for the slot model: the display fetches VRAM
    pub active_display: bool,
    /// this line's display, in master clocks
    pub active_start: i64,
    pub active_end: i64,
    /// the vertical sync cut a display period short
    vblank_owed: bool,
    /// (time, order, name), sorted
    events: Vec<(i64, u64, Event)>,
    order: u64,
}

impl Sequencer {
    /// The reference's default is `vblank_dots = Some(-16)`.
    pub fn new(vblank_dots: Option<i64>) -> Sequencer {
        let rcr_count = 0x3FF;
        Sequencer {
            vblank_dots,
            divider: 4,
            origin: 0,
            vphase: VPhase::Vsw,
            lines_left: 1,
            vsync_this_line: false,
            rcr_count,
            rcr_compared: rcr_count,
            display_count: -1,
            burst_mode: false,
            cr_cache: 0,
            display_line: false,
            active_display: false,
            active_start: 0,
            active_end: 0,
            vblank_owed: false,
            events: Vec::new(),
            order: 0,
        }
    }

    // -- the scheduler ----------------------------------------------------------------------------

    pub fn schedule(&mut self, when: i64, name: Event) {
        self.order += 1;
        self.events.push((when, self.order, name));
        self.events.sort_by_key(|&(when, order, _)| (when, order));
    }

    fn drop_events(&mut self, name: Event) {
        self.events.retain(|e| e.2 != name);
    }

    pub fn next_event(&self) -> Option<i64> {
        self.events.first().map(|e| e.0)
    }
}

impl Vdc {
    pub(crate) fn seq_schedule(&mut self, when: i64, name: Event) {
        self.seq.schedule(when, name);
    }

    pub(crate) fn seq_next_event(&self) -> Option<i64> {
        self.seq.next_event()
    }

    /// Handle every event due at or before master-clock time `t`.
    pub(crate) fn seq_run_to(&mut self, t: i64) {
        while let Some(&(when, _, name)) = self.seq.events.first() {
            if when > t {
                break;
            }
            self.seq.events.remove(0);
            self.now = when;
            match name {
                Event::Step => self.seq_step(when),
                Event::Latch => self.latch_scroll(),
                Event::Display => self.seq_display_start(when),
                Event::Raster => self.raster_test(self.seq.rcr_compared),
                Event::Vblank => self.vblank_begins(),
                Event::Collision => self.collision_hit(),
            }
        }
    }

    // -- the VCE's syncs --------------------------------------------------------------------------

    /// The horizontal sync falls: a line begins (the board calls this at each line start).
    pub(crate) fn seq_hsync(&mut self, mclk: i64, divider: i64) {
        self.now = mclk;
        self.seq.origin = mclk;
        self.seq.divider = divider;
        self.seq.vsync_this_line = false;
        self.seq.rcr_compared = self.seq.rcr_count; // the count as the previous line step left it
        if RASTER_COMPARE_CLOCKS != 0 {
            self.seq
                .schedule(mclk + RASTER_COMPARE_CLOCKS, Event::Raster);
        } else {
            self.raster_test(self.seq.rcr_compared); // [emu: MAME] as the sync falls
        }
        let step_at = mclk + HSYNC_CLOCKS;
        self.seq.schedule(step_at, Event::Step);
        // The vertical blanking of the line after the display period (or the line after a cut).
        if self.seq.vblank_owed || (self.seq.vphase == VPhase::Vdw && self.seq.lines_left == 0) {
            self.seq.vblank_owed = false;
            let when = match self.seq.vblank_dots {
                None => step_at + (self.regs.hds() + 1) * CHARACTER_DOTS * divider,
                Some(vblank_dots) => step_at + vblank_dots * divider,
            };
            self.seq.schedule(when.max(mclk), Event::Vblank);
        }
    }

    /// The vertical sync falls (the board calls this after `hsync` of the line it falls on).
    pub(crate) fn seq_vsync(&mut self) {
        if self.seq.vphase == VPhase::Vdw && !self.seq.events.iter().any(|e| e.2 == Event::Vblank) {
            self.seq.vblank_owed = true; // cut short: the vertical blanking comes on the next line
        }
        self.seq_enter(VPhase::Vsw);
        self.seq.vsync_this_line = true;
    }

    // -- the line step ----------------------------------------------------------------------------

    fn seq_step(&mut self, now: i64) {
        // [emu: MAME] the end of the sync restarts the horizontal periods: a display that ran on
        // from the last line ends here, and one that never started is dropped.
        self.seq.drop_events(Event::Display);
        self.seq.drop_events(Event::Latch);
        if self.seq.active_end > now {
            self.seq.active_end = now;
        }
        self.line_begins();
        self.seq.rcr_count += 1;
        if self.seq.vsync_this_line {
            self.seq.vsync_this_line = false; // the vertical sync's first line: no period change
        } else if self.seq.lines_left <= 0 {
            self.seq_enter(Self::seq_following(self.seq.vphase));
        }
        self.seq.lines_left -= 1;
        if self.seq.vphase == VPhase::Vds && self.seq.lines_left == 0 {
            self.seq.rcr_count = 0; // [mfr: RCR] 64 on the line before the display period
        }
        self.seq.display_line = self.seq.vphase == VPhase::Vdw && !self.seq.burst_mode;
        self.seq.active_display = self.seq.display_line;
        if self.seq.vphase == VPhase::Vdw {
            self.seq.display_count += 1;
            if self.seq.display_line {
                self.fetch_sprites(self.seq.display_count);
            }
        }
        let start = now + (self.regs.hds() + 1) * CHARACTER_DOTS * self.seq.divider;
        if start < self.seq.origin + LINE_CLOCKS + HSYNC_CLOCKS {
            let latch = now.max(start - SCROLL_LATCH_DOTS * self.seq.divider);
            self.seq.schedule(latch, Event::Latch);
            self.seq.schedule(start, Event::Display);
        }
    }

    fn seq_following(phase: VPhase) -> VPhase {
        match phase {
            VPhase::Vsw => VPhase::Vds,
            VPhase::Vds => VPhase::Vdw,
            VPhase::Vdw => VPhase::Vcr,
            VPhase::Vcr => VPhase::Vsw,
        }
    }

    fn seq_enter(&mut self, phase: VPhase) {
        self.seq.vphase = phase;
        match phase {
            VPhase::Vsw => {
                self.seq.lines_left = self.regs.vsw() + 1; // [mfr: VPR "N-1"]
                                                           // [mfr: CR] the VDC's frame begins: SB = BB = 0 now holds the frame in burst mode
                let burst = self.regs.cr() & (r::CR_SPRITES | r::CR_BACKGROUND) == 0;
                let began = burst && !self.seq.burst_mode;
                self.seq.burst_mode = burst;
                if began {
                    self.burst_began();
                }
            }
            VPhase::Vds => {
                self.seq.lines_left = self.regs.vds() + 2; // [mfr: VPR "N-2"]
            }
            VPhase::Vdw => {
                self.seq.lines_left = self.regs.vdw() + 1; // [mfr: VDW "N-1"]
                self.seq.display_count = -1;
                self.display_period_starts();
            }
            VPhase::Vcr => {
                self.seq.lines_left = self.regs.vcr() + FRONT_PORCH_EXTRA_LINES;
            }
        }
    }

    // -- the display width ------------------------------------------------------------------------

    fn seq_display_start(&mut self, now: i64) {
        self.seq.active_start = now;
        // [mfr: HDR "N-1"]
        let width = (self.regs.hdw() + 1) * CHARACTER_DOTS * self.seq.divider;
        self.seq.active_end = (now + width).min(self.seq.origin + LINE_CLOCKS + HSYNC_CLOCKS);
        // [mfr: CR SB/BB "valid from the next horizontal display"]
        self.seq.cr_cache = self.regs.cr();
        self.render_line();
    }
}
