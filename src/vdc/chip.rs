//! The HuC6270 VDC as the CPU sees it: the bus port, VRAM access, status, IRQ1 (`vdc/chip.py`).
//!
//! Instances, not module state: a SuperGrafx has two, a PC-FX may one day pin this module.
//!
//! Bus ($FF:0000-03FF, 4-byte mirror) [hw: pcetech 5]:
//!
//! ```text
//! $0  write: register select (low 5 bits); read: status
//! $1  unused, reads 0
//! $2  data LSB     $3  data MSB
//! ```
//!
//! Provisional (U1, tier 5): the VDC's CPU access schedule in active display. Hudson's table and
//! MacDonald's give the mode-0 slot order (CPU, BAT, CPU, ?, CPU, CG0, CPU, CG1) but not the
//! phase against the line. A VRAM data access made during the display width is *accepted* at once
//! and *committed* at the next CPU slot of an 8-dot cycle counted from the display start; BSY
//! (status bit 6) is set until then; a second data access while one is pending holds the CPU
//! (RDY) until the first commits. Mednafen instead charges a statistical 1-cycle penalty (C4).
//!
//! Differences forced by Rust: the reference's `hold` callable (the board's) is an accumulator
//! here, `held`, which the board takes when the access returns (`take_hold`): the reference's
//! board applies its own sum at the same point.

use serde::{Deserialize, Serialize};

use crate::machine::{py_floordiv, HSYNC_CLOCKS};
use crate::mt::PythonRandom;
use crate::vdc::dma::DmaEngine;
use crate::vdc::regs as r;
use crate::vdc::sequencer::{Event, Sequencer, VPhase};
use crate::vdc::{background, sprites};

/// [hw: pcetech 5] 64 KiB; the upper half of the 128 KiB space mirrors it
pub const VRAM_WORDS: usize = 0x8000;

/// [hw: pcetech 5.1 register $09] CPU slots within the 8-dot cycle, by VRAM access mode
/// (MWR D1-D0): mode 0 "CPU BAT CPU ??? CPU CG0 CPU CG1", modes 1 and 2 "BAT CPU CG0 CG1" over
/// two dots each, mode 3 none for the CPU (four-dot accesses, BAT then CG).
pub const CPU_SLOTS: [&[i64]; 4] = [&[0, 2, 4, 6], &[2], &[2], &[]];

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Vdc {
    pub regs: r::VdcRegisters,
    held: i64,
    pub vram: Vec<u16>,
    pub sat: Vec<u16>,
    pub status: u8,
    /// master-clock time of the sequencer event being handled
    pub now: i64,
    pub seq: Sequencer,
    pub dma: DmaEngine,
    // [emu: Mednafen BG_YMoo, BG_YOffset, BG_XOffset] the scroll as the line latched it
    pub(crate) bg_y: i64,
    pub(crate) need_bg_y_step: bool,
    pub(crate) bg_y_offset: i64,
    pub(crate) bg_x_offset: i64,
    /// MWR as the vertical sync latched it
    pub(crate) mwr_cache: u16,
    pub(crate) row: Option<Vec<u16>>,
    pub(crate) sprite_entries: Vec<sprites::Entry>,
    /// A pending CPU VRAM write: (address, word)
    pub(crate) pending_write: Option<(u16, u16)>,
    pub(crate) pending_read: Option<u16>,
    pub(crate) commit_at: i64,
}

/// The earlier of two optional times.
#[inline]
fn earlier(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(a), Some(b)) => Some(a.min(b)),
        (a, None) => a,
        (None, b) => b,
    }
}

impl Vdc {
    /// The reference's defaults: `vblank_dots = Some(-8)`, `vertical_at_line_start = false`,
    /// `dma_resumes = true`.
    pub fn new(
        power_on: Option<&mut PythonRandom>,
        vblank_dots: Option<i64>,
        vertical_at_line_start: bool,
        dma_resumes: bool,
    ) -> Vdc {
        // U7: zeros by default, random on request.
        let (vram, sat) = match power_on {
            None => (vec![0; VRAM_WORDS], vec![0; 256]),
            Some(rng) => {
                let vram = (0..VRAM_WORDS)
                    .map(|_| rng.getrandbits(16) as u16)
                    .collect();
                let sat = (0..256).map(|_| rng.getrandbits(16) as u16).collect();
                (vram, sat)
            }
        };
        Vdc {
            regs: r::VdcRegisters::new(),
            held: 0,
            vram,
            sat,
            status: 0,
            now: 0,
            seq: Sequencer::new(vblank_dots, vertical_at_line_start),
            dma: DmaEngine::new(dma_resumes),
            bg_y: 0,
            need_bg_y_step: false,
            bg_y_offset: 0,
            bg_x_offset: 0,
            mwr_cache: 0,
            row: None,
            sprite_entries: Vec::new(),
            pending_write: None,
            pending_read: None,
            commit_at: 0,
        }
    }

    /// The master clocks the accesses since the last call asked the CPU to wait (the
    /// reference's `hold` callable, summed).
    pub fn take_hold(&mut self) -> i64 {
        std::mem::take(&mut self.held)
    }

    fn hold(&mut self, clocks: i64) {
        self.held += clocks;
    }

    // -- outputs --------------------------------------------------------------------------------

    /// The /IRQ pin: held until the status port is read.
    #[inline]
    pub fn irq1(&self) -> bool {
        self.status & r::IRQ_BITS != 0
    }

    #[inline]
    pub fn bsy(&self) -> bool {
        self.pending_write.is_some() || self.pending_read.is_some()
    }

    /// The fork: the earliest of the three by hand (the base board folds an array of options
    /// through an iterator, which the compiler does not flatten).
    #[inline]
    pub fn next_event(&self) -> Option<i64> {
        earlier(
            earlier(self.seq_next_event(), self.commit_event()),
            self.dma_next_event(),
        )
    }

    #[inline]
    fn commit_event(&self) -> Option<i64> {
        if self.bsy() {
            Some(self.commit_at)
        } else {
            None
        }
    }

    pub fn catch_up(&mut self, mclk: i64) {
        loop {
            let seq_t = self.seq_next_event();
            let dma_t = self.dma_next_event();
            let commit_t = self.commit_event();
            let due = earlier(earlier(seq_t, dma_t), commit_t).filter(|&t| t <= mclk);
            let Some(t) = due else {
                self.dma_advance(mclk);
                return;
            };
            if dma_t == Some(t) {
                self.dma_advance(t);
            } else if commit_t == Some(t) && seq_t.is_none_or(|seq_t| t <= seq_t) {
                self.commit_pending();
            } else {
                self.dma_advance(t);
                self.seq_run_to(t);
            }
        }
    }

    // -- the sync generator's calls (the VCE's line clock) ----------------------------------------

    pub fn hsync(&mut self, mclk: i64, divider: i64) {
        self.seq_hsync(mclk, divider);
    }

    pub fn vsync(&mut self) {
        self.seq_vsync();
        self.need_bg_y_step = false;
        self.mwr_cache = self.regs.mwr();
    }

    // -- events from the sequencer ----------------------------------------------------------------

    pub(crate) fn vblank_test(&mut self) {
        if self.regs.cr() & r::CR_VBLANK_IRQ != 0 {
            self.status |= r::VD;
        }
    }

    pub(crate) fn raster_test(&mut self, count: i64) {
        // [hw: pcetech 5.1] RCR is compared with the line counted from $40 at the first active
        // line; values outside $40-$146 never match.
        if count == self.regs.rcr() - 0x40 && self.regs.cr() & r::CR_RASTER_IRQ != 0 {
            self.status |= r::RR;
        }
    }

    pub(crate) fn latch_byr(&mut self) {
        self.step_bg_y();
        self.bg_y_offset = self.bg_y;
    }

    pub(crate) fn latch_bxr(&mut self) {
        self.bg_x_offset = (self.regs.value[r::BXR] & 0x3FF) as i64;
    }

    fn step_bg_y(&mut self) {
        if self.need_bg_y_step {
            self.need_bg_y_step = false;
            self.bg_y = if self.seq.rcr_count == 0 {
                (self.regs.value[r::BYR] & 0x1FF) as i64
            } else {
                self.bg_y + 1
            };
        }
    }

    pub(crate) fn line_counter_step(&mut self, _count: i64) {
        self.step_bg_y();
        self.need_bg_y_step = true;
    }

    /// The scan for the next line's sprites, with the overflow flag and the narrow-display
    /// budget (U3: Mesen2's: the VDC fetches 4, 4, 8 or 16 dots per entry between the end of
    /// one line's display width and 16 dots before the next line's).
    pub(crate) fn fetch_sprites(&mut self, line: i64) {
        let seq = &self.seq;
        let dots = 1365 / seq.divider;
        let previous_end = py_floordiv(seq.active_end - seq.origin - HSYNC_CLOCKS, seq.divider);
        let next_start = dots + 8 + (self.regs.hds() + 1) * 8 - 16;
        let budget = 0.max(next_start - previous_end)
            / sprites::CLOCKS_PER_SPRITE[((self.mwr_cache >> 2) & 3) as usize];
        // The fork: into the last line's list (`scan` allocates one).
        let overflow = sprites::scan_into(
            &mut self.sprite_entries,
            &self.vram,
            VRAM_WORDS,
            &self.sat,
            line,
            self.mwr_cache,
            budget as usize,
        );
        if overflow && self.regs.cr() & r::CR_OVERFLOW_IRQ != 0 {
            self.status |= r::OR;
        }
    }

    pub(crate) fn collision_hit(&mut self) {
        if self.regs.cr() & r::CR_COLLISION_IRQ != 0 {
            self.status |= r::CR_FLAG;
        }
    }

    pub(crate) fn display_started(&mut self) {
        self.dma_display_started(self.now);
    }

    pub(crate) fn display_ended(&mut self) {
        self.dma_display_ended(self.now);
    }

    pub(crate) fn burst_began(&mut self) {
        self.dma_advance(self.now);
    }

    /// The 9-bit pixels of this line, drawn as the display width begins. [hw: pcetech 5.2, 6]
    /// Backdrop (background colour 0, $000) inside the width if either layer is on, overscan
    /// (sprite colour 0, $100) otherwise and outside.
    pub(crate) fn render_line(&mut self) {
        let seq = &self.seq;
        let dots = (1365 / seq.divider) as usize;
        // The fork: the last line's buffer again (the base board allocates a row a line).
        let mut row = self.row.take().unwrap_or_default();
        row.clear();
        row.resize(dots, 0x100u16);
        if !seq.display_line {
            self.row = Some(row);
            return;
        }
        let row_start = seq.origin + HSYNC_CLOCKS;
        let first = py_floordiv(seq.active_start - row_start, seq.divider);
        let width = py_floordiv(seq.active_end - seq.active_start, seq.divider);
        let cr = seq.cr_cache;
        let divider = seq.divider;
        let end = (first + width).min(dots as i64);
        if first >= end {
            // Not reachable: a display start at or past the row's end is later than the next
            // line's sync, which drops this line's events before `render_line` runs
            // (sequencer.rs, `seq_leave_hsync`). Kept so the slices below cannot panic.
            self.row = Some(row);
            return;
        }
        let (first_index, end_index) = (first as usize, end as usize);
        // The fork: drawn into the row itself (the base board draws the whole width apart and
        // copies what fits).
        let shown = &mut row[first_index..end_index];
        if cr & 0x80 != 0 {
            background::row_into(
                &self.vram,
                VRAM_WORDS,
                self.mwr_cache,
                self.bg_x_offset,
                self.bg_y_offset,
                shown,
            );
        } else {
            // sprites only, or neither layer inside a non-burst display period: the backdrop.
            // [emu: Mednafen, Mesen2 and Beetle all show colour 0 of the background palette when
            // both layers are off (Space Harrier's last 12 lines); pcetech 5.2 says overscan: C30]
            shown.fill(0x000);
        }
        let mut collision = None;
        if cr & 0x40 != 0 && !self.sprite_entries.is_empty() {
            let hit = sprites::draw(&self.sprite_entries, end - first, &mut row, first_index);
            if let Some(hit) = hit {
                if self.regs.cr() & r::CR_COLLISION_IRQ != 0 {
                    // the flag rises when the beam reaches the pixel (Mesen2 runs the VDC per CPU cycle
                    // for this; Mednafen raises it as the line is drawn): U3
                    collision = Some(row_start + (first + hit) * divider);
                }
            }
        }
        self.row = Some(row);
        if let Some(when) = collision {
            self.seq_schedule(when.max(self.now), Event::Collision);
        }
    }

    /// Commit a waiting CPU access unless a transfer holds the VRAM (Mednafen's
    /// `CheckAndCommitPending`: nothing is committed while the SAT or VRAM DMA runs).
    pub(crate) fn commit_unless_dma(&mut self) {
        if !self.dma.busy() {
            self.commit_pending();
        }
    }

    pub(crate) fn commit_pending(&mut self) {
        if let Some((address, word)) = self.pending_write.take() {
            if (address as usize) < VRAM_WORDS {
                // [hw] writes to the mirrored half are ignored
                self.vram[address as usize] = word;
            }
        }
        if let Some(address) = self.pending_read.take() {
            // Mirrored reads "sometimes return corrupted data" [hw]; U23: this model reads 0.
            self.regs.read_buffer = if (address as usize) < VRAM_WORDS {
                self.vram[address as usize]
            } else {
                0
            };
        }
    }

    // -- the CPU's port ---------------------------------------------------------------------------

    pub fn read(&mut self, offset: u32, mclk: i64) -> u8 {
        self.catch_up(mclk);
        let port = offset & 3;
        if port == 0 {
            let value = self.status | if self.bsy() { r::BSY } else { 0 };
            self.status &= !r::IRQ_BITS;
            return value;
        }
        if port == 1 {
            return 0;
        }
        let msb = port == 3;
        if self.pending_read.is_some() {
            // The word the last MARR write or VRR read asked for is not in the buffer yet (it
            // waits for a CPU slot or a DMA): the CPU waits for it rather than reading the old
            // word. The manual's read sequence and both Mednafen and Mesen2 ("D&D Order of the
            // Griffon breaks without this") agree; the model's slot wait made the case possible.
            self.data_access(mclk);
        }
        let value = if msb {
            (self.regs.read_buffer >> 8) as u8
        } else {
            (self.regs.read_buffer & 0xFF) as u8
        };
        if msb && self.regs.select as usize == r::VRW {
            // [hw: pcetech 5.1 $02] after the MSB read MARR advances and the next word is read
            self.data_access(mclk);
            self.regs.value[r::MARR] = self.regs.value[r::MARR].wrapping_add(self.regs.increment());
            self.pending_read = Some(self.regs.value[r::MARR]);
            self.schedule_commit(mclk);
        }
        value
    }

    pub fn write(&mut self, offset: u32, value: u8, mclk: i64) {
        self.catch_up(mclk);
        let port = offset & 3;
        if port == 0 {
            self.regs.select = value & 0x1F;
            return;
        }
        if port == 1 {
            return;
        }
        let register = self.regs.select;
        if !r::valid_register(register) {
            return;
        }
        let register = register as usize;
        let msb = port == 3;
        if register == r::VRW {
            if !msb {
                self.regs.write_latch = value;
            } else {
                self.data_access(mclk);
                let word = self.regs.write_latch as u16 | ((value as u16) << 8);
                self.pending_write = Some((self.regs.value[r::MAWR], word));
                self.regs.value[r::MAWR] =
                    self.regs.value[r::MAWR].wrapping_add(self.regs.increment());
                self.schedule_commit(mclk);
            }
            return;
        }
        self.regs.set_byte(register, value, msb);
        if register == r::MARR && msb {
            // [hw: pcetech 5.1 $01] writing the MSB loads the read buffer from VRAM[MARR]
            self.data_access(mclk);
            self.pending_read = Some(self.regs.value[r::MARR]);
            self.schedule_commit(mclk);
        } else if register == r::BYR {
            // [emu: Mednafen] set on both bytes: only changing on the MSB breaks a game
            self.bg_y = (self.regs.value[r::BYR] & 0x1FF) as i64;
        } else if register == r::SATB {
            self.dma_arm_satb(); // [mfr] armed by a write; starts when the display period ends
        } else if register == r::LENR && msb {
            self.dma_start(mclk);
        }
    }

    // -- VRAM access timing -----------------------------------------------------------------------

    /// A second data-port access while one is pending holds the CPU until it commits.
    fn data_access(&mut self, mclk: i64) {
        let mut now = mclk;
        while self.bsy() && self.dma.busy() {
            // the pending access is stuck behind a transfer
            let Some(due) = self.dma_next_event() else {
                // paused behind the display: nothing to wait for but the end of it
                self.dma_force_complete(now);
                break;
            };
            if due > now {
                self.hold(due - now);
            }
            self.catch_up(due);
            now = now.max(due);
        }
        if self.bsy() {
            let wait = self.commit_at - now;
            if wait > 0 {
                self.hold(wait);
            }
            self.commit_pending();
        }
    }

    fn schedule_commit(&mut self, mclk: i64) {
        if self.dma.busy() {
            // waits for the transfer; its end commits (BSY meanwhile)
            self.commit_at = 1 << 62;
            return;
        }
        self.commit_at = self.slot_time(mclk);
        if self.commit_at <= mclk {
            self.commit_pending();
        }
    }

    /// Earliest time at or after `mclk` that the CPU owns a VRAM slot (U1, provisional).
    pub fn slot_time(&self, mclk: i64) -> i64 {
        let seq = &self.seq;
        if !(seq.active_display() && (seq.active_start..seq.active_end).contains(&mclk)) {
            return mclk;
        }
        let div = seq.divider;
        // first dot boundary at or after mclk
        let mut dot = -py_floordiv(-(mclk - seq.active_start), div);
        let slots = CPU_SLOTS[(self.regs.mwr() & 3) as usize];
        if slots.is_empty() {
            return seq.active_end;
        }
        loop {
            let when = seq.active_start + dot * div;
            if when >= seq.active_end {
                return seq.active_end;
            }
            if slots.contains(&dot.rem_euclid(8)) {
                return when;
            }
            dot += 1;
        }
    }

    // -- display ----------------------------------------------------------------------------------

    pub fn vphase(&self) -> VPhase {
        self.seq.vphase
    }

    /// The fork: `pixel_row` without the copy, where the line was drawn at this width.
    #[inline]
    pub(crate) fn drawn_row(&self, dots: usize) -> Option<&[u16]> {
        match &self.row {
            Some(row) if row.len() == dots => Some(row),
            _ => None,
        }
    }

    /// The 9-bit VDC output for the line that just ran (drawn by `render_line`).
    pub fn pixel_row(&self, dots: usize) -> Vec<u16> {
        match &self.row {
            Some(row) if row.len() == dots => row.clone(),
            _ => vec![0x100; dots],
        }
    }
}
