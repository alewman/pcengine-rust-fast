// SPDX-License-Identifier: MIT
//! The HuC6270 VDC as the CPU sees it: the bus port, VRAM access, status, IRQ1 (`vdc/chip.py`).
//!
//! Milestone 1 scope (the reference's docs/handoff-brief.md): timing, status/IRQ and the VRAM port
//! with no tiles. Background and DMA are M2, sprites M3. The registers they will use are stored
//! already.
//!
//! Instances, not module state: a SuperGrafx has two, a PC-FX may one day pin this package.
//!
//! Bus ($FF:0000-03FF, 4-byte mirror) [hw: pcetech 5]:
//!     $0  write: register select (low 5 bits); read: status
//!     $1  unused, reads 0
//!     $2  data LSB     $3  data MSB
//!
//! Provisional (U1, tier 5): the VDC's CPU access schedule in active display. Hudson's table and
//! MacDonald's give the mode-0 slot order (CPU, BAT, CPU, ?, CPU, CG0, CPU, CG1) but not the
//! phase against the line. A VRAM data access made during the display width is *accepted* at once
//! and *committed* at the next CPU slot of an 8-dot cycle counted from the display start; BSY
//! (status bit 6) is set until then; a second data access while one is pending holds the CPU
//! (RDY) until the first commits. Mednafen instead charges a statistical 1-cycle penalty (C4).
//!
//! Rust (issue 0004, the board's wait): the reference's `serve` is a callable that runs the whole
//! board from inside an access. Here it is a flag set by the board; an access that would wait with
//! it set *stops* (`serve_wanted`, answering $FF), the board serves the chip and makes the decode
//! again (sockets.rs, `VdcAccess`). An access stops before it changes anything the board can see:
//! `read` before the buffer is taken; `write` before the word is queued (VWR) or, for MARR's high
//! byte, after the byte is stored (stored again, the same byte, when the access is made again).
//! The `hold` callable is an accumulator (`take_hold`).
use serde::{Deserialize, Serialize};

use crate::machine::{py_floordiv, HSYNC_CLOCKS};
use crate::mt::PythonRandom;
use crate::vdc::dma::DmaEngine;
use crate::vdc::regs as r;
use crate::vdc::sequencer::{Event, Sequencer, VPhase};
use crate::vdc::sprites::SPRITE_LIMIT;
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
    /// The background's scroll [mfr: BXR, BYR; emu: MAME huc6270.cpp]: the vertical counter is
    /// loaded from BYR when the display period starts and when BYR is written (either byte),
    /// and counts one at every line step, so a write during the display shows as BYR + 1 on
    /// the next line (the manual's words); both offsets are taken 16 dots before the display
    /// start (vdc/sequencer.rs `SCROLL_LATCH_DOTS`, a closed-box fit).
    pub(crate) bg_y_counter: i64,
    /// the offsets the line being drawn was taken with
    pub(crate) bg_x_offset: i64,
    pub(crate) bg_y_offset: i64,
    /// MWR bits 6-0 as the last vertical blanking took them
    pub(crate) mwr_cache: u16,
    pub(crate) row: Option<Vec<u16>>,
    pub(crate) sprite_entries: Vec<sprites::Entry>,
    /// A pending CPU VRAM write: (address, word)
    pub(crate) pending_write: Option<(u16, u16)>,
    pub(crate) pending_read: Option<u16>,
    pub(crate) commit_at: i64,
    /// The board's wait (issue 0004; the reference's `serve`): set by the board on its VDCs.
    /// Unset (a VDC on its own, with no sync to deliver), a wait runs the chip's own events
    /// (`wait_alone`). Wiring, not state: not in a save state, and a loaded state keeps it.
    #[serde(skip)]
    pub serve: bool,
    /// An access stopped at its wait for the board to serve this chip (see the module's notes).
    #[serde(skip)]
    pub serve_wanted: bool,
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
    /// The reference's defaults: `vblank_dots = Some(-16)`, `dma_resumes = true`.
    pub fn new(
        power_on: Option<&mut PythonRandom>,
        vblank_dots: Option<i64>,
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
            seq: Sequencer::new(vblank_dots),
            dma: DmaEngine::new(dma_resumes),
            bg_y_counter: 0,
            bg_x_offset: 0,
            bg_y_offset: 0,
            mwr_cache: 0,
            row: None,
            sprite_entries: Vec::new(),
            pending_write: None,
            pending_read: None,
            commit_at: 0,
            serve: false,
            serve_wanted: false,
        }
    }

    /// The master clocks the accesses since the last call asked the CPU to wait (the
    /// reference's `hold` callable, summed).
    pub fn take_hold(&mut self) -> i64 {
        std::mem::take(&mut self.held)
    }

    /// The fork: a hold is waiting for `take_hold`.
    #[inline]
    pub(crate) fn holding(&self) -> bool {
        self.held != 0
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

    /// The VCE's vertical sync falls (after this line's `hsync`).
    pub fn vsync(&mut self) {
        self.seq_vsync();
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

    /// The line step (the end of the horizontal sync): the vertical scroll counts a line.
    pub(crate) fn line_begins(&mut self) {
        self.row = None; // the last line's row has been taken; this line's is drawn anew
        self.bg_y_counter = (self.bg_y_counter + 1) & 0x1FF;
    }

    /// The first line of the display period: the vertical scroll starts again at BYR.
    pub(crate) fn display_period_starts(&mut self) {
        self.bg_y_counter = (self.regs.value[r::BYR] & 0x1FF) as i64;
        self.display_started();
    }

    /// Shortly before the display start: the offsets this line is drawn with [mfr: BXR "valid
    /// with the next scanning line"; BYR as counted]. BXR has 10 bits and BYR 9 [mfr: register
    /// figures].
    pub(crate) fn latch_scroll(&mut self) {
        self.bg_x_offset = (self.regs.value[r::BXR] & 0x3FF) as i64;
        self.bg_y_offset = self.bg_y_counter;
    }

    /// The vertical blanking period begins: the VD flag, the MWR fields that change only now
    /// [mfr: MWR VM, SM, SCREEN "valid at the beginning of a vertical blanking period"; CM is
    /// taken at each line], and the sprite-table transfer [mfr: DVSSR note (b)].
    pub(crate) fn vblank_begins(&mut self) {
        self.vblank_test();
        self.mwr_cache = self.regs.mwr() & 0x7F;
        self.display_ended();
    }

    /// The sprites of display line `line`, fetched in the blanking before it [mfr: 2.4.1 (6),
    /// status OR]. The manual's table limits both steps by the horizontal registers:
    ///
    /// - SAT entries searched: all 64 when the display is 32 characters or wider (d = HDW + 1
    ///   >= 32), else 2d + 1 with sprite access width 1 or 2 (SM 0, 1), 2d with SM 2, 3;
    /// - patterns fetched: 2(e - 2) with SM 0 or 1, e - 2 with SM 2, (e - 2) / 2 with SM 3, at
    ///   most 16, where e = HDE + HSW + HDS + 3 is the blanking in characters.
    ///
    /// A sprite that is searched but not fetched sets OR (the manual's second case), as a 17th
    /// does (the first). Counted in 16-dot patterns (a 32-wide sprite is two), as `sprites::scan`
    /// counts them: a reading of "number of sprites", provisional.
    pub(crate) fn fetch_sprites(&mut self, line: i64) {
        let regs = &self.regs;
        let mwr = self.mwr_cache | (regs.mwr() & 0x80);
        let sm = ((mwr >> 2) & 3) as usize;
        let d = regs.hdw() + 1;
        let e = regs.hde() + (regs.value[r::HSR] & 0x1F) as i64 + regs.hds() + 3;
        let searched = if d >= 32 {
            64
        } else if sm < 2 {
            2 * d + 1
        } else {
            2 * d
        };
        let fetched = [2 * (e - 2), 2 * (e - 2), e - 2, py_floordiv(e - 2, 2)][sm];
        let fetched = fetched.min(SPRITE_LIMIT as i64).max(0) as usize;
        let (mut entries, mut overflow) = sprites::scan(
            &self.vram,
            VRAM_WORDS,
            &self.sat,
            line,
            mwr,
            SPRITE_LIMIT,
            searched as usize,
        );
        if entries.len() > fetched {
            entries.truncate(fetched);
            overflow = true;
        }
        self.sprite_entries = entries;
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

    /// The 9-bit pixels of this line, drawn as the display width begins (M2: background only;
    /// the sprites are M3). [hw: pcetech 5.2, 6] Backdrop (background colour 0, $000) inside the
    /// width if either layer is on, overscan (sprite colour 0, $100) otherwise and outside.
    pub(crate) fn render_line(&mut self) {
        let seq = &self.seq;
        let dots = (1365 / seq.divider) as usize;
        let mut row = vec![0x100u16; dots];
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
            // first <= dots: a display start past the row's end is never scheduled (the next line
            // step restarts the horizontal periods first: vdc/sequencer.rs). An empty width
            // draws nothing, as the reference's empty slices do; kept so the slices below cannot
            // panic.
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
                self.mwr_cache | (self.regs.mwr() & 0x80), // [mfr: MWR CM "with the next line"]
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
                    // U3/U26, a choice: the flag rises when the beam reaches the pixel (the
                    // emulators observed differ: some raise it as the line is drawn)
                    collision = Some(row_start + (first + hit) * divider);
                }
            }
        }
        self.row = Some(row);
        if let Some(when) = collision {
            self.seq_schedule(when.max(self.now), Event::Collision);
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

    pub fn read(&mut self, offset: u32, mut mclk: i64) -> u8 {
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
        if self.serve
            && (self.pending_read.is_some()
                || (msb && self.regs.select as usize == r::VRW && self.bsy()))
        {
            // one of the two `data_access` calls below would wait: the board serves it first
            self.serve_wanted = true;
            return 0xFF;
        }
        if self.pending_read.is_some() {
            // The word the last MARR write or VRR read asked for is not in the buffer yet (it
            // waits for a CPU slot or a DMA): the CPU waits for it rather than reading the old
            // word [mfr: the read sequence MARR, then VRR]; the model's slot wait made the case
            // possible (observed: Mednafen and Mesen2 wait too; D&D Order of the Griffon needs it).
            mclk = self.data_access(mclk);
        }
        let value = if msb {
            (self.regs.read_buffer >> 8) as u8
        } else {
            (self.regs.read_buffer & 0xFF) as u8
        };
        if msb && self.regs.select as usize == r::VRW {
            // [hw: pcetech 5.1 $02] after the MSB read MARR advances and the next word is read
            mclk = self.data_access(mclk);
            self.regs.value[r::MARR] = self.regs.value[r::MARR].wrapping_add(self.regs.increment());
            self.pending_read = Some(self.regs.value[r::MARR]);
            self.schedule_commit(mclk);
        }
        value
    }

    /// [hw: pcetech 5.1; mfr: Hudson HuC6270 manual, registers AR, VWR, MARR, BYR, LENR,
    /// DVSSR] Port 0 selects the register; ports 2/3 write its low/high byte. VWR's low byte goes
    /// to the write latch only; its high byte writes latch and byte to VRAM at MAWR, which then
    /// advances by CR's increment. MARR's high byte starts the read into the buffer. LENR's
    /// high byte starts the VRAM -> VRAM transfer, DVSSR's arms the sprite-table transfer.
    /// Registers outside the valid set ignore writes.
    pub fn write(&mut self, offset: u32, value: u8, mut mclk: i64) {
        self.catch_up(mclk);
        let port = offset & 3;
        if port == 0 {
            self.regs.select = value & 0x1F;
            return;
        }
        if port == 1 {
            return;
        }
        let msb = port == 3;
        let register = self.regs.select as usize;
        if register == r::VRW {
            if !msb {
                self.regs.write_latch = value;
                return;
            }
            if self.serve && self.bsy() {
                // `data_access` would wait: the board serves it first (module notes)
                self.serve_wanted = true;
                return;
            }
            // a second data access while one is pending holds the CPU until it commits
            mclk = self.data_access(mclk);
            let address = self.regs.value[r::MAWR];
            self.pending_write = Some((
                address,
                ((value as u16) << 8) | self.regs.write_latch as u16,
            ));
            self.regs.value[r::MAWR] = address.wrapping_add(self.regs.increment());
            self.schedule_commit(mclk);
            return;
        }
        if !r::valid_register(register as u8) {
            return;
        }
        self.regs.set_byte(register, value, msb);
        if register == r::BYR {
            // [emu: MAME huc6270.cpp] either byte reloads the vertical scroll counter
            self.bg_y_counter = (self.regs.value[r::BYR] & 0x1FF) as i64;
        } else if msb && register == r::MARR {
            if self.serve && self.bsy() {
                // `data_access` would wait: the board serves it first (module notes)
                self.serve_wanted = true;
                return;
            }
            mclk = self.data_access(mclk);
            self.pending_read = Some(self.regs.value[r::MARR]);
            self.schedule_commit(mclk);
        } else if msb && register == r::LENR {
            self.dma_start(mclk);
        } else if msb && register == r::SATB {
            self.dma_arm_satb();
        }
    }

    // -- VRAM access timing -----------------------------------------------------------------------

    /// A second data-port access while one is pending holds the CPU until it commits; the
    /// time at which the new access is accepted is returned.
    ///
    /// The hold is the /BUSY pin on the CPU's RDY [wiki: VDC pinout]: only the CPU stops. The
    /// VDC's syncs come from the VCE, so the display keeps its lines while the CPU waits, and a
    /// VRAM -> VRAM transfer the CPU waits on meets the display start like any other: paused
    /// (C7, the default) or cancelled (`dma_resumes=false`, which serves the access there).
    /// Issue 0004: the wait used to run this chip alone to the transfer's end, lines ahead of
    /// the board's line clock, so the display never started during it.
    fn data_access(&mut self, mclk: i64) -> i64 {
        if !self.bsy() {
            return mclk;
        }
        // With the board's wait wired the access stopped before this point (`serve_wanted`) and
        // is made again once the board has served it, so nothing is pending here.
        debug_assert!(!self.serve, "the board serves a wired VDC's wait");
        self.wait_alone(mclk)
    }

    /// `data_access` for a VDC with no board: no line clock reaches it during the wait.
    fn wait_alone(&mut self, mclk: i64) -> i64 {
        let mut now = mclk;
        while self.bsy() && self.dma.busy() {
            // the pending access is stuck behind a transfer
            let Some(due) = self.dma_next_event() else {
                // paused behind a display no sync will ever end
                let end = self.dma_finish_without_display(now);
                if end > now {
                    self.hold(end - now);
                    now = end;
                }
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
                now = self.commit_at;
            }
            self.commit_pending();
        }
        now
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
        if !(seq.active_display && (seq.active_start..seq.active_end).contains(&mclk)) {
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
