//! The VDC's vertical state machine and the horizontal events of one line (`vdc/sequencer.py`).
//!
//! Provisional, emulator tier 5 (U19): no tier 1-3 source gives the dot at which the vblank flag
//! rises or the raster compare fires. This is Mednafen's `huc6270/vdc.cpp` (HSync/VSync/IncRCR
//! and the HPHASE machine), restated in master clocks so that it runs from the board's one
//! timeline. What *is* published: the frame layout and "VDS+VSW lines before the first active
//! line", [hw: pcetech 7]; the flag rules of status bit 5 [hw: pcetech 5]; the RCR range
//! $40-$146 counted from the first active line [hw: pcetech 5.1].
//!
//! Vertical phases run VSW -> VDS -> VDW -> VCR -> VSW, counted once per line at the end of the
//! displayed width (4 dots before it ends: Mednafen's HDW_FINAL). The VCE starts the VSW phase at
//! the vertical sync, which is the last three lines of its frame, so with VSW = $02 and VDS = $17
//! the first line of the display period is frame line 25, as pcetech 7 says.
//!
//! Horizontal events within a line (dot = the VCE's dot divider in master clocks, measured from the
//! VCE's hsync start at the line's origin):
//!
//! ```text
//! origin + 237            hsync ends (`HSYNC_CLOCKS`, emu)
//! + 8 dots                HDS: the display-start phase begins; a pending line counter step
//!                         from the previous line is taken here if not yet done
//! + (HDS + 1) * 8 dots    HDW: the display width begins; vblank flag test, sprite scan (M3)
//! + (HDW + 1) * 8 - 4     HDW final: the line counter steps (RCR compare, phase counters)
//! ```
//!
//! Differences forced by Rust: `Sequencer` is the state; its methods are `impl Vdc` here, with
//! the reference's names behind `seq_` (the reference's sequencer calls back into the chip
//! through a reference). The event kinds are an enum where the reference has strings; the
//! list is kept sorted by time with a stable sort, as the reference's `list.sort` is.

use serde::{Deserialize, Serialize};

use crate::machine::HSYNC_CLOCKS;
use crate::vdc::chip::Vdc;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum VPhase {
    Vsw = 0,
    Vds = 1,
    Vdw = 2,
    Vcr = 3,
}

impl VPhase {
    fn following(self) -> VPhase {
        match self {
            VPhase::Vsw => VPhase::Vds,
            VPhase::Vds => VPhase::Vdw,
            VPhase::Vdw => VPhase::Vcr,
            VPhase::Vcr => VPhase::Vsw,
        }
    }
}

/// [emu: Mednafen `Cycles_Between_RCRIRQ_And_HDWEnd = 4`]
pub const FINAL_DOTS: i64 = 4;
/// [emu: Mednafen HSW phase counter 8 after leaving hsync]
pub const HSW_DOTS: i64 = 8;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Event {
    Byr,
    Bxr,
    Leave,
    Collision,
    Vblank,
    Hds,
    Hdw,
    Final,
}

/// State and events only; the chip owns the registers.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Sequencer {
    /// The vblank flag test happens this many dots after the sync ends (negative: before it
    /// ends).
    ///
    /// Default -8 (U19, provisional, tier 1): the position that fits Covell's hardware
    /// photographs best, anchored on the test's own text (docs/m1-record.md): 0, MAME's, leaves
    /// the splits 7-13 dots right of the hardware's; -8 leaves -0.6 to +6.6. None is Mednafen's,
    /// the start of the display width (32 dots on for HDS = 2), which Covell calls too late.
    pub vblank_dots: Option<i64>,
    /// Experiment (MAME's `huc6270.cpp`): the vertical phase counters step as each line
    /// *starts*, and the vblank flag is raised as the display period ends; Mednafen's (the
    /// default) step 4 dots before the end of each line's display width, almost a line later.
    pub vertical_at_line_start: bool,
    /// [emu: Mednafen Reset] power-up phase VDS with one line left; arbitrary but fixed (U1).
    pub vphase: VPhase,
    pub vphase_counter: i64,
    pub rcr_count: i64,
    pub scanline_from_vsync: i64,
    pub need_rcr_inc: bool,
    pub need_vb_irq_test: bool,
    pub burst_mode: bool,
    /// this line is in the display period and not burst
    pub display_line: bool,
    /// CR as the line's display start latched it
    pub cr_cache: u16,
    // cached at the vertical sync / start of the VSW phase
    pub vds: i64,
    pub vsw: i64,
    pub vdw: i64,
    pub vcr: i64,
    // line geometry, set by hsync()
    pub divider: i64,
    pub origin: i64,
    pub hds_start: i64,
    pub active_start: i64,
    pub final_at: i64,
    pub active_end: i64,
    events: Vec<(i64, Event)>,
    next_line: (i64, i64, i64, i64),
}

impl Sequencer {
    pub fn new(vblank_dots: Option<i64>, vertical_at_line_start: bool) -> Sequencer {
        Sequencer {
            vblank_dots,
            vertical_at_line_start,
            vphase: VPhase::Vds,
            vphase_counter: 1,
            rcr_count: 0,
            scanline_from_vsync: 0,
            need_rcr_inc: false,
            need_vb_irq_test: false,
            burst_mode: false,
            display_line: false,
            cr_cache: 0,
            vds: 0,
            vsw: 0,
            vdw: 0,
            vcr: 0,
            divider: 4,
            origin: 0,
            hds_start: 0,
            active_start: 0,
            final_at: 0,
            active_end: 0,
            events: Vec::new(),
            next_line: (0, 4, 0, 0),
        }
    }

    /// The current line is in the display period and not burst (valid from HDW start).
    #[inline]
    pub fn active_display(&self) -> bool {
        self.display_line
    }

    fn sort(&mut self) {
        self.events.sort_by_key(|event| event.0);
    }
}

impl Vdc {
    // -- the board's hooks ------------------------------------------------------------------------

    #[inline]
    pub(crate) fn seq_next_event(&self) -> Option<i64> {
        self.seq.events.first().map(|event| event.0)
    }

    pub(crate) fn seq_run_to(&mut self, mclk: i64) {
        while !self.seq.events.is_empty() && self.seq.events[0].0 <= mclk {
            let (when, kind) = self.seq.events.remove(0);
            self.now = when;
            match kind {
                Event::Byr => self.latch_byr(),
                Event::Bxr => self.latch_bxr(),
                Event::Leave => self.seq_leave_hsync(),
                Event::Collision => self.collision_hit(),
                Event::Vblank => self.seq_vblank(),
                Event::Hds => self.seq_hds(),
                Event::Hdw => self.seq_hdw(),
                Event::Final => self.seq_final(),
            }
        }
    }

    /// The VCE's horizontal sync starts: cache the horizontal registers; the line proper
    /// begins when the sync ends. Events of the previous line may still be pending: with
    /// HDS = 2 and HDW = $1F a 5 MHz line's width-end lands 8 master clocks into the next
    /// line's sync, and it must still run (emu: Mednafen caches at sync start, restarts at
    /// sync end).
    pub(crate) fn seq_hsync(&mut self, mclk: i64, divider: i64) {
        self.seq.next_line = (mclk, divider, self.regs.hds(), self.regs.hdw());
        self.seq.events.push((mclk + HSYNC_CLOCKS, Event::Leave));
        if let Some(vblank_dots) = self.seq.vblank_dots {
            if vblank_dots < 0 {
                // before the sync ends: the line's own events do not exist yet
                self.seq
                    .events
                    .push((mclk + HSYNC_CLOCKS + vblank_dots * divider, Event::Vblank));
            }
        }
        self.seq.sort();
    }

    fn seq_leave_hsync(&mut self) {
        let (origin, divider, hds, hdw) = self.seq.next_line;
        let seq = &mut self.seq;
        seq.origin = origin;
        seq.divider = divider;
        seq.hds_start = origin + HSYNC_CLOCKS + HSW_DOTS * divider;
        seq.active_start = seq.hds_start + (hds + 1) * 8 * divider;
        seq.active_end = seq.active_start + (hdw + 1) * 8 * divider;
        seq.final_at = seq.active_end - FINAL_DOTS * divider;
        if self.seq.vertical_at_line_start {
            self.seq_advance_vertical();
        }
        // Stale events of the previous line are dropped, as Mednafen's HPhase is overwritten.
        // [emu: Mednafen TimeFromHDSStartToBYRLatch / TimeFromBYRLatchToBXRLatch] the scroll
        // registers are latched a few dots after the display-start phase begins: BYR at dot 1,
        // BXR at dot 3 for HDS <= 2; later and one dot apart for a wider HDS (U3).
        let to_byr = if hds <= 2 { 1 } else { (hds + 1) * 8 - 21 };
        let to_bxr = to_byr + if hds <= 2 { 2 } else { 1 };
        let seq = &mut self.seq;
        // (the fork: the list's own buffer again)
        seq.events.clear();
        seq.events.extend_from_slice(&[
            (seq.hds_start, Event::Hds),
            (seq.hds_start + to_byr * divider, Event::Byr),
            (seq.hds_start + to_bxr * divider, Event::Bxr),
            (seq.active_start, Event::Hdw),
            (seq.final_at, Event::Final),
        ]);
        if let Some(vblank_dots) = seq.vblank_dots {
            if vblank_dots >= 0 {
                seq.events
                    .push((origin + HSYNC_CLOCKS + vblank_dots * divider, Event::Vblank));
                seq.sort();
            }
        }
    }

    /// Run `kind` at `mclk` (a time inside the current line).
    pub(crate) fn seq_schedule(&mut self, mclk: i64, kind: Event) {
        self.seq.events.push((mclk, kind));
        self.seq.sort();
    }

    /// The vertical sync starts (the last three lines of the VCE's frame).
    ///
    /// Deviation from Mednafen (tier 5), which drops the previous line's pending line-counter
    /// step here: pcetech 5.1 (tier 2) gives RCR a range of 263 values ($40-$146) counted from
    /// the first active line, so a 263-line frame must make 263 steps and the step is taken
    /// now instead (U19).
    pub(crate) fn seq_vsync(&mut self) {
        if self.seq.need_rcr_inc {
            self.seq_inc_rcr();
            self.seq.need_rcr_inc = false;
        }
        if self.seq.vphase == VPhase::Vdw {
            // [emu: Mednafen] the SAT transfer test runs whenever the phase is not VDW after the
            // display period began, so a VDW that the sync cuts short (Rastan Saga II: VDS $18,
            // VDW $FF) still transfers (C27, found by the M5 sweep).
            self.display_ended();
        }
        self.seq_cache_vertical();
        self.seq.vphase = VPhase::Vsw;
        self.seq.vphase_counter = self.seq.vsw + 1;
        self.seq.scanline_from_vsync = 0;
    }

    // -- events -----------------------------------------------------------------------------------

    fn seq_hds(&mut self) {
        self.seq.cr_cache = self.regs.cr();
        if self.seq.need_rcr_inc {
            self.seq_inc_rcr();
            self.seq.need_rcr_inc = false;
        }
    }

    fn seq_hdw(&mut self) {
        self.seq.display_line = self.seq.vphase == VPhase::Vdw && !self.seq.burst_mode;
        self.render_line();
        self.seq.need_rcr_inc = true;
        if self.seq.vblank_dots.is_none() && !self.seq.vertical_at_line_start {
            self.seq_vblank();
        }
        self.commit_unless_dma();
    }

    fn seq_vblank(&mut self) {
        if self.seq.vphase != VPhase::Vdw && self.seq.need_vb_irq_test {
            self.seq.need_vb_irq_test = false;
            self.vblank_test();
        }
    }

    fn seq_final(&mut self) {
        if self.seq.scanline_from_vsync == self.seq.vsw + 1 {
            // [emu: Mednafen] burst mode is decided once a frame, from CR as the display
            // period begins; pcetech 5.1 "locked when the next frame starts" (footnote 1).
            self.seq.burst_mode = self.regs.cr() & 0xC0 == 0;
            if self.seq.burst_mode {
                self.burst_began();
            }
        }
        if self.seq.need_rcr_inc {
            self.seq_inc_rcr();
            self.seq.need_rcr_inc = false;
        }
    }

    fn seq_cache_vertical(&mut self) {
        self.seq.vds = self.regs.vds();
        self.seq.vsw = self.regs.vsw();
        self.seq.vdw = self.regs.vdw();
        self.seq.vcr = self.regs.vcr();
    }

    fn seq_inc_rcr(&mut self) {
        self.line_counter_step(self.seq.rcr_count);
        self.seq.rcr_count += 1;
        if !self.seq.vertical_at_line_start {
            self.seq_advance_vertical();
        }
        self.raster_test(self.seq.rcr_count);
        if self.seq.vphase == VPhase::Vdw && !self.seq.burst_mode {
            self.fetch_sprites(self.seq.rcr_count); // for the line about to be drawn
        }
    }

    fn seq_advance_vertical(&mut self) {
        self.seq.vphase_counter -= 1;
        self.seq.scanline_from_vsync += 1;
        if self.seq.vphase_counter <= 0 {
            self.seq.vphase = self.seq.vphase.following();
            if self.seq.vphase == VPhase::Vcr {
                self.display_ended();
            }
            match self.seq.vphase {
                VPhase::Vds => self.seq.vphase_counter = self.seq.vds + 2,
                VPhase::Vdw => {
                    self.seq.vphase_counter = self.seq.vdw + 1;
                    self.seq.rcr_count = 0;
                    self.seq.need_vb_irq_test = true;
                    self.display_started();
                }
                VPhase::Vcr => {
                    self.seq.vphase_counter = self.seq.vcr;
                    if self.seq.vertical_at_line_start && self.seq.vblank_dots.is_none() {
                        self.seq.need_vb_irq_test = false;
                        self.vblank_test();
                    } else if self.seq.vertical_at_line_start {
                        // raised by the "vblank" event, N dots on
                        self.seq.need_vb_irq_test = true;
                    }
                }
                VPhase::Vsw => {
                    self.seq.vphase_counter = self.seq.vsw + 1;
                    self.seq.scanline_from_vsync = 0;
                    self.seq_cache_vertical();
                }
            }
        }
    }
}
