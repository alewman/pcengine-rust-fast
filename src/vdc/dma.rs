// SPDX-License-Identifier: MIT
// This file transcribes the reference's vdc/dma.py of the clean rewrite of 2026-11-19
// (pcengine-python 3318027; docs/restart.md here), written without the withdrawn code or any GPL
// source.
//! The VDC's two DMAs and burst mode: VRAM -> VRAM, and VRAM -> sprite attribute table
//! (`vdc/dma.py`).
//!
//! [hw: pcetech 5.4; mfr: Hudson HuC6270 manual DCR/SOUR/DESR/LENR/DVSSR]
//!
//! VRAM -> VRAM: starts when LENR's MSB is written; SOUR and DESR step up or down by DCR D2/D3 (not
//! by the CR increment), LENR counts down and ends at $FFFF; MAWR, MARR, the read buffer and the
//! write latch are untouched. A transfer that is running when the display period starts is **paused
//! and resumes once the VDC is free again** (C7: MacDonald, tier 2; Mesen2 is observed to do the
//! same; Hudson's manual says the remainder is not transferred, and Mednafen is observed to cancel
//! it; `resume=false` selects that reading).
//!
//! Time per word: a word is one VRAM read and one VRAM write, and the VRAM access width (MWR VM) is
//! "how many clocks should be used for VRAM ... accesses, or for block transfer" [mfr: MWR (a)]: one
//! dot per access at width 1, so **2 dots a word** in mode 0, 4 in modes 1 and 2, 8 in mode 3 (the
//! mode as the last vertical blanking took it). DOC for the reading of the table, the per-word count
//! a derivation from it (HYPOTHESIS for modes 1-3, which no game here uses: pcetech 5.1).
//!
//! VRAM -> SAT: armed by a write to DVSSR's MSB [mfr: "triggered by access to the high byte"],
//! started when the vertical blanking period begins (or every frame with DCR D4) [mfr: note (b)]; the
//! 256 words are copied at the start and the VDC is busy until the done flag (DCR D0), which comes
//! **three lines later**: "four scanlines after the end of the active display period (e.g. if the
//! last line of the active display is $DF, it happens at $E3)" [hw: pcetech 5.4, tier 2; MacDonald
//! measured it at the 5 MHz dot clock as far as he says; what it is at 7 and 10 MHz is open, U2].
//!
//! Burst mode (CR D6 and D7 clear when the VDC's frame starts) is held by the sequencer; the SAT
//! transfer and VRAM DMA are allowed whenever the display is not drawing.
//!
//! While either runs, CPU VRAM accesses wait (BSY).
//!
//! Rust: the reference's `DmaEngine` holds its chip; here it is the state (`Vdc::dma`) and its
//! methods are an `impl Vdc` block with a `dma_` prefix (vdc/mod.rs).

use serde::{Deserialize, Serialize};

use crate::machine::LINE_CLOCKS;
use crate::vdc::chip::Vdc;
use crate::vdc::regs as r;
use crate::vdc::sequencer::VPhase;

/// [hw: pcetech 5.4] done four lines after the last display line
pub const SAT_CLOCKS: i64 = 3 * LINE_CLOCKS;
/// [mfr: MWR VM] dots per VRAM access by access-width mode
pub const ACCESS_DOTS: [i64; 4] = [1, 2, 2, 4];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DmaEngine {
    pub resume: bool,
    /// a VRAM -> VRAM transfer has been started and not finished
    pub running: bool,
    read_phase: bool,
    buffer: u16,
    /// master-clock time up to which the transfer has been run
    t0: i64,
    pub satb_pending: bool,
    /// the VDC is busy with the SAT transfer until then
    pub sat_end: Option<i64>,
    /// finish_without_display: a VDC with no sync never displays
    no_display: bool,
}

impl DmaEngine {
    pub fn new(resume: bool) -> DmaEngine {
        DmaEngine {
            resume,
            running: false,
            read_phase: true,
            buffer: 0,
            t0: 0,
            satb_pending: false,
            sat_end: None,
            no_display: false,
        }
    }

    // -- state ------------------------------------------------------------------------------------

    /// CPU VRAM accesses wait while a transfer runs, paused or not.
    #[inline]
    pub fn busy(&self) -> bool {
        self.running || self.sat_end.is_some()
    }
}

impl Vdc {
    /// The VRAM is free of display fetches (blanking, or burst mode).
    fn dma_allowed(&self) -> bool {
        let seq = &self.seq;
        self.dma.sat_end.is_none()
            && (self.dma.no_display || seq.vphase != VPhase::Vdw || seq.burst_mode)
    }

    /// Master clocks of one access (the read or the write of a word).
    fn dma_half_word(&self) -> i64 {
        ACCESS_DOTS[(self.mwr_cache & 3) as usize] * self.seq.divider
    }

    pub(crate) fn dma_next_event(&self) -> Option<i64> {
        let mut times: [Option<i64>; 2] = [None, None];
        if let Some(sat_end) = self.dma.sat_end {
            times[0] = Some(sat_end);
        }
        if self.dma.running && self.dma_allowed() {
            let mut halves = (self.regs.value[r::LENR] as i64 + 1) * 2;
            if !self.dma.read_phase {
                halves -= 1; // the read of the current word is done
            }
            times[1] = Some(self.dma.t0 + halves * self.dma_half_word());
        }
        times.into_iter().flatten().min()
    }

    // -- commands ---------------------------------------------------------------------------------

    /// LENR's MSB was written.
    pub(crate) fn dma_start(&mut self, now: i64) {
        self.dma_advance(now);
        self.dma.running = true;
        self.dma.read_phase = true;
        self.dma.t0 = now;
    }

    pub(crate) fn dma_arm_satb(&mut self) {
        self.dma.satb_pending = true;
    }

    pub(crate) fn dma_display_started(&mut self, now: i64) {
        self.dma_advance(now);
        if self.dma.running && !self.seq.burst_mode && !self.dma.resume {
            self.dma.running = false; // the manual's reading: the remainder is lost, no done IRQ
            self.commit_pending();
        }
        self.dma.t0 = now;
    }

    /// The vertical blanking period begins: run the armed (or, with DCR D4, every frame's)
    /// sprite-table transfer [mfr: DVSSR note (b), DCR DSR]. The words are copied now; reads
    /// past the VRAM give 0, as the CPU port's do (U23).
    pub(crate) fn dma_display_ended(&mut self, now: i64) {
        self.dma_advance(now);
        if !(self.dma.satb_pending || self.regs.value[r::DCR] & 0x10 != 0) {
            return;
        }
        self.dma.satb_pending = false;
        let source = self.regs.value[r::SATB];
        let size = self.vram.len();
        for i in 0..256u16 {
            let address = source.wrapping_add(i) as usize;
            self.sat[i as usize] = if address < size {
                self.vram[address]
            } else {
                0
            };
        }
        self.dma.sat_end = Some(now + SAT_CLOCKS);
    }

    // -- running ----------------------------------------------------------------------------------

    /// Run what is due up to master-clock time `t`.
    pub(crate) fn dma_advance(&mut self, t: i64) {
        if let Some(end) = self.dma.sat_end {
            if end <= t {
                self.dma.sat_end = None;
                if self.regs.value[r::DCR] & 0x01 != 0 {
                    self.status |= r::DS;
                }
                self.commit_pending();
                self.dma.t0 = end;
            }
        }
        if !self.dma.running || !self.dma_allowed() {
            if self.dma.running {
                self.dma.t0 = t; // paused: no progress, the clock keeps moving
            }
            return;
        }
        let half = self.dma_half_word(); // one access: the read, then the write, of each word
        while self.dma.running && self.dma.t0 + half <= t {
            if self.dma.read_phase {
                let src = self.regs.value[r::SOUR] as usize;
                self.dma.buffer = if src < self.vram.len() {
                    self.vram[src]
                } else {
                    0
                };
                self.dma.t0 += half;
                self.dma.read_phase = false;
                continue;
            }
            let dst = self.regs.value[r::DESR] as usize;
            if dst < self.vram.len() {
                self.vram[dst] = self.dma.buffer;
            }
            let dcr = self.regs.value[r::DCR];
            let regs = &mut self.regs.value;
            regs[r::SOUR] = if dcr & 0x04 != 0 {
                regs[r::SOUR].wrapping_sub(1)
            } else {
                regs[r::SOUR].wrapping_add(1)
            };
            regs[r::DESR] = if dcr & 0x08 != 0 {
                regs[r::DESR].wrapping_sub(1)
            } else {
                regs[r::DESR].wrapping_add(1)
            };
            regs[r::LENR] = regs[r::LENR].wrapping_sub(1);
            self.dma.t0 += half;
            self.dma.read_phase = true;
            if self.regs.value[r::LENR] == 0xFFFF {
                self.dma.running = false;
                self.commit_pending();
                if dcr & 0x02 != 0 {
                    self.status |= r::DV;
                }
            }
        }
    }

    /// Run a paused transfer to its end as if the VRAM were free and return that time: for a
    /// VDC with no board, whose display never ends because no sync reaches it. (A choice for
    /// the chip on its own; a board's VDC always gets the VCE's syncs.)
    pub(crate) fn dma_finish_without_display(&mut self, now: i64) -> i64 {
        self.dma_advance(now);
        self.dma.no_display = true;
        if self.dma.running {
            self.dma.t0 = self.dma.t0.max(now);
        }
        let mut end = now;
        while self.dma.busy() {
            let Some(due) = self.dma_next_event() else {
                break;
            };
            self.dma_advance(due);
            end = end.max(due);
        }
        self.dma.no_display = false;
        end
    }
}
