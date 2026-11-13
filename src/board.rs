//! The board: one timeline in master clocks, the page table, and the devices around the CPU
//! (`board.py`).
//!
//! The CPU core (huc6280-rust) calls `read`, `write` and `idle` once per CPU cycle with a
//! 21-bit physical address. **The board prices the cycle**: 12 master clocks at low speed, 3 at high
//! speed (`pins.high_speed`), plus the VDC/VCE stretch and any `hold`, all inside the callback; it
//! never adds `step()`'s return on top (the reference's docs/handoff-brief.md, the bus contract).
//!
//! Where inside a cycle a device sees the access is U21 (provisional): the base cycle elapses, then
//! the access happens at that instant, then the stretch cycle for a VDC or VCE address elapses.
//!
//! Differences forced by Rust:
//!
//! - **The CPU owns the board** (`cpu.bus`), so the board cannot hold the CPU. What the
//!   reference's board does to its CPU (`set_irq_lines`, `irq_disable`, reading `high_speed`)
//!   goes through the `Pins` each bus callback is handed, at the same points of the cycle and
//!   in the same order; `attach_cpu` is `System::new`'s. With no CPU (`advance` called on a
//!   bare board, as the reference's tests do) the lines are tracked and nothing is delivered.
//! - **The decode is here**: `page_read` and `page_write` turn the page table's `Handler` names
//!   into calls (pagetable.rs).
//! - **The constructor's keyword arguments** are the fields of `BoardOptions`, which also
//!   carries the switches the reference keeps as class attributes (`Psg`'s, `Timer`'s).
//! - **The pixel path** is the board's field (`pixel_path`); the capture is handed the mixed
//!   row. **The mixer** is handed the sources in the reference's order: the PSG, the port
//!   device's own, then `audio_sources`.
//! - **`bus_log`** is the tap the reference's scripts make by wrapping the three callables
//!   (`scripts/state_roundtrip.py`, `observe`): when it is `Some`, every cycle appends the same
//!   words.

use huc6280::{Bus, Pins};

use crate::cards::{PopulousCard, RomCard, Sf2Card, TennokoeCard};
use crate::cpu_io::CpuIo;
use crate::hardware_page::map_hardware_page;
use crate::machine::{
    Machine, ADDRESS_MASK, CPU_CLOCKS_HIGH, CPU_CLOCKS_LOW, HARDWARE_FIRST_PAGE, HSYNC_CLOCKS,
    LINE_CLOCKS, PAGE_BITS, PC_ENGINE,
};
use crate::mixer::Mixer;
use crate::mt::PythonRandom;
use crate::pagetable::{Handler, PageTable, Ram};
use crate::psg::{Psg, PsgSwitches};
use crate::sockets::{
    AudioSource, BoardPins, Card, CardSlot, ControllerPort, DirectPath, IrqLine, PixelPath,
    PortDevice, ResetKind, VideoTiming,
};
use crate::vce::Vce;
use crate::vdc::Vdc;
use crate::video::{Capture, Timeline};

/// The reference's `Board(...)` arguments, with its defaults.
pub struct BoardOptions {
    pub machine: Machine,
    pub card: Option<Box<dyn Card>>,
    pub port: Option<Box<dyn PortDevice>>,
    pub pad: Option<Box<dyn ControllerPort>>,
    pub pixel_path: Option<Box<dyn PixelPath>>,
    /// `None` powers up with zeros (U7, deterministic); a seed randomises RAM, VRAM, the
    /// sprite table and the colour table reproducibly (Python's `random.Random(seed)`).
    pub power_on_seed: Option<i64>,
    /// The raster line at time zero (U1: the real power-up phase is not published).
    pub start_line: i64,
    pub start_offset: i64,
    /// U19: see `vdc::sequencer::Sequencer::vblank_dots`.
    pub vblank_dots: Option<i64>,
    pub vertical_at_line_start: bool,
    /// C7: see vdc/dma.rs.
    pub dma_resumes: bool,
    /// The PSG's switches (class attributes of the reference's `Psg`).
    pub psg: PsgSwitches,
    /// C25 (the reference's `Timer.underflow_window_mclk`).
    pub timer_underflow_window_mclk: i64,
}

impl Default for BoardOptions {
    fn default() -> Self {
        BoardOptions {
            machine: PC_ENGINE,
            card: None,
            port: None,
            pad: None,
            pixel_path: None,
            power_on_seed: None,
            start_line: 0,
            start_offset: 0,
            vblank_dots: Some(-8),
            vertical_at_line_start: false,
            dma_resumes: true,
            psg: PsgSwitches::default(),
            timer_underflow_window_mclk: 15,
        }
    }
}

pub struct Board {
    pub machine: Machine,
    pub mclk: i64,
    pub(crate) held: i64,
    pub(crate) lines: (bool, bool, bool),
    /// set by the first new line
    pub(crate) row_end: Option<i64>,
    /// VideoTiming of the line whose row is still being drawn
    pub(crate) row_timing: Option<VideoTiming>,
    pub irq2: IrqLine,
    pub vce: Vce,
    /// One VDC instance per `machine.vdc_count` (docs/sockets.md socket 12). `vdcs[0]` is the
    /// first; the others are run, synced and read for pixels and IRQ1 by the board, but have
    /// no decode of their own: an add-on (the SuperGrafx's VPC) claims their pages.
    pub vdcs: Vec<Vdc>,
    pub psg: Psg,
    pub cpu_io: CpuIo,
    pub ram: Ram,
    pub slot: CardSlot,
    pub port: Option<Box<dyn PortDevice>>,
    /// Further mixer inputs, after the PSG and the port device's own.
    pub audio_sources: Vec<Box<dyn AudioSource>>,
    pub mixer: Mixer,
    pub timeline: Timeline,
    pub capture: Capture,
    pub pixel_path: Box<dyn PixelPath>,
    pub pages: PageTable,
    /// The opcode the CPU is running, as the last bus callback's `Pins` showed it.
    pub(crate) opcode: u8,
    /// When `Some`, every CPU cycle appends: for a read or a write `address << 8 | value`
    /// then `mclk << 2 | kind` (0 read, 1 write); for an idle cycle `mclk << 2 | 2` alone.
    pub bus_log: Option<Vec<u64>>,
    /// When `Some`, every CPU cycle appends (kind, address, value, mclk): kind 0 read, 1 write,
    /// 2 idle (address and value 0). The lockstep gate's per-cycle mode.
    pub bus_detail: Option<Vec<(u8, u32, u8, i64)>>,
    /// The fork's event horizon: no device has anything to do before this master clock
    /// (`horizon`), so a CPU cycle that ends before it only moves the clock. `i64::MIN` when
    /// it is not known (`touch`), which sends the next cycle through `run_to` as the base
    /// board does every cycle.
    pub(crate) wake: i64,
    /// The horizon proper (`horizon`): `wake` is this, or "every cycle" with a device on the
    /// expansion port, whose `catch_up` is then all a cycle before `events_at` runs.
    pub(crate) events_at: i64,
    /// Something outside a bus access may have left a hold or a line for the next access to
    /// apply (the host through `board_mut()`, a port device's `catch_up`): the next access
    /// finishes in full, whatever its page.
    pub(crate) unsettled: bool,
    /// The card is one of this crate's own (`card_is_plain`): its accesses finish like work
    /// RAM's.
    pub(crate) plain_card: bool,
    /// The pixel path is this crate's `DirectPath` (VDC 1 straight into the VCE): `end_row`
    /// hands the capture the VDC's row itself.
    pub(crate) direct_path: bool,
}

impl Board {
    pub fn new(options: BoardOptions) -> Result<Board, String> {
        let machine = options.machine;
        let mut rng = options.power_on_seed.map(PythonRandom::new);

        let vce = Vce::new(rng.as_mut());
        let vdcs: Vec<Vdc> = (0..machine.vdc_count)
            .map(|_| {
                Vdc::new(
                    rng.as_mut(),
                    options.vblank_dots,
                    options.vertical_at_line_start,
                    options.dma_resumes,
                )
            })
            .collect();
        if vdcs.is_empty() {
            return Err("a machine has at least one VDC".to_string());
        }
        let psg = Psg::new(44100.0, options.psg);
        let mut cpu_io = CpuIo::new(machine.country_pc_engine, options.pad);
        cpu_io.timer.underflow_window_mclk = options.timer_underflow_window_mclk;
        let mut ram = Ram::new(machine.work_ram_bytes);
        if let Some(rng) = rng.as_mut() {
            for byte in ram.data.iter_mut() {
                *byte = rng.getrandbits(8) as u8;
            }
        }
        let timeline = Timeline::new(&machine, &vce, options.start_line, options.start_offset)?;
        let capture = Capture::new(&vce);

        let mut pages = PageTable::new();
        // [hw: pcetech 3] banks $F8-$FB work RAM ($F9-$FB mirror $F8), $FF the hardware page.
        pages.set_base(0x1F00, 0x1F7F, Handler::Ram);
        map_hardware_page(&mut pages);
        if let Some(card) = options.card.as_ref() {
            for (first, last) in card.claims() {
                pages.claim(first, last, Handler::Card)?;
            }
        }
        if let Some(port) = options.port.as_ref() {
            for (first, last) in port.claims() {
                pages.claim(first, last, Handler::Port)?;
            }
        }
        let pixel_path = options.pixel_path.unwrap_or_else(|| Box::new(DirectPath));
        for (first, last) in pixel_path.claims() {
            pages.claim(first, last, Handler::PixelPath)?;
        }

        let mut board = Board {
            machine,
            mclk: 0,
            held: 0,
            lines: (false, false, false),
            row_end: None,
            row_timing: None,
            irq2: IrqLine::new("IRQ2"),
            vce,
            vdcs,
            psg,
            cpu_io,
            ram,
            slot: CardSlot::new(options.card),
            port: options.port,
            audio_sources: Vec::new(),
            mixer: Mixer::default(),
            timeline,
            capture,
            pixel_path,
            pages,
            opcode: 0,
            bus_log: None,
            bus_detail: None,
            wake: i64::MIN,
            events_at: i64::MIN,
            unsettled: true,
            plain_card: false,
            direct_path: false,
        };
        board.touch();
        let divider = board.vce.dot_divider();
        for vdc in board.vdcs.iter_mut() {
            vdc.hsync(board.timeline.line_start, divider);
            if board.timeline.line >= board.timeline.frame_lines - 3 {
                vdc.vsync();
            }
        }
        Ok(board)
    }

    /// The first VDC (the reference's `board.vdc`).
    pub fn vdc(&mut self) -> &mut Vdc {
        self.touch();
        &mut self.vdcs[0]
    }

    /// The board was reached from outside a bus access (the host, a loaded state): forget the
    /// event horizon, and let the next access finish in full. `System::board_mut`, `step`,
    /// `run_frame` and `run_until` call it; a host that changes the board through
    /// `system.cpu.bus` and then calls `system.cpu.step()` itself must too.
    #[inline]
    pub fn touch(&mut self) {
        self.wake = i64::MIN;
        self.events_at = i64::MIN;
        self.unsettled = true;
        self.plain_card = self.card_is_plain();
        self.direct_path = self.pixel_path.as_any_mut().is::<DirectPath>();
    }

    /// The fork: the card in the slot is this crate's own `RomCard`, `Sf2Card`, `PopulousCard`
    /// or `TennokoeCard`, whose accesses use nothing of the `BoardPins` they are handed: no
    /// hold, no interrupt line, no VDC. Any other card (an add-on's, a test's) may, and its
    /// accesses finish in full.
    fn card_is_plain(&mut self) -> bool {
        self.slot.card.as_mut().is_some_and(|card| {
            let card = card.as_any_mut();
            card.is::<RomCard>()
                || card.is::<Sf2Card>()
                || card.is::<PopulousCard>()
                || card.is::<TennokoeCard>()
        })
    }

    /// The earliest master clock at which `run_to` has anything to do but the port device's
    /// `catch_up`: the next line, the end of the row being drawn, each VDC's next event, the
    /// timer's next tick (every tick, not only the underflow: the count is state).
    /// `i64::MIN` (every cycle) while a VRAM DMA runs or waits, which moves words or its own
    /// clock at every catch-up.
    fn horizon(&self) -> i64 {
        let mut at = self.timeline.next_line_start();
        if let Some(row_at) = self.row_end {
            at = at.min(row_at);
        }
        for vdc in self.vdcs.iter() {
            if vdc.dma.running {
                return i64::MIN;
            }
            if let Some(vdc_at) = vdc.next_event() {
                at = at.min(vdc_at);
            }
        }
        if let Some(tick_at) = self.cpu_io.timer.next_tick() {
            at = at.min(tick_at);
        }
        at
    }

    /// Recompute the horizon. A device on the expansion port is promised a `catch_up` at
    /// every advance (socket 5), so with one every cycle wakes.
    #[inline]
    fn look_ahead(&mut self) {
        self.events_at = self.horizon();
        self.wake = if self.port.is_some() {
            i64::MIN
        } else {
            self.events_at
        };
    }

    /// Mixed stereo samples (44.1 kHz) of every audio source up to the clock now.
    pub fn take_audio(&mut self) -> Vec<(i16, i16)> {
        let mut sources: Vec<&mut dyn AudioSource> = vec![&mut self.psg];
        if let Some(port) = self.port.as_mut() {
            sources.extend(port.audio_sources());
        }
        for source in self.audio_sources.iter_mut() {
            sources.push(source.as_mut());
        }
        self.mixer.take(self.mclk, sources)
    }

    /// The buttons held on pad `port` (`ControllerPort::set_pressed`; the reference's host
    /// sets `pad.pressed` on the object it kept).
    pub fn set_pressed(&mut self, port: usize, pressed: u16) {
        if let Some(device) = self.cpu_io.port.device.as_mut() {
            device.set_pressed(port, pressed);
        }
    }

    // -- the decode -------------------------------------------------------------------------------

    fn cd_attached(&self) -> bool {
        self.port.as_ref().is_some_and(|port| port.present())
    }

    /// The reference's `pages.read`: one decode for every bus master.
    pub fn page_read(&mut self, address: u32, mclk: i64) -> u8 {
        let address = address & ADDRESS_MASK;
        match self.pages.handler(address) {
            Handler::Unmapped => 0xFF,
            Handler::Ram => self.ram.read(address),
            Handler::Vdc => self.vdcs[0].read(address & 3, mclk),
            Handler::Vce => self.vce.read(address & 7, mclk),
            Handler::CpuIo => {
                let cd_attached = self.cd_attached();
                self.cpu_io.read(address & 0x1FFF, mclk, cd_attached)
            }
            Handler::Card => {
                let Board {
                    slot,
                    irq2,
                    held,
                    vdcs,
                    opcode,
                    ..
                } = self;
                let mut pins = BoardPins {
                    irq2,
                    held,
                    vdcs,
                    opcode: *opcode,
                    card: None,
                };
                match slot.card.as_mut() {
                    Some(card) => card.read(address, mclk, &mut pins),
                    None => 0xFF,
                }
            }
            Handler::Port => {
                let Board {
                    slot,
                    port,
                    irq2,
                    held,
                    vdcs,
                    opcode,
                    ..
                } = self;
                let mut pins = BoardPins {
                    irq2,
                    held,
                    vdcs,
                    opcode: *opcode,
                    card: slot.card.as_deref_mut(),
                };
                match port.as_mut() {
                    Some(port) => port.read(address, mclk, &mut pins),
                    None => 0xFF,
                }
            }
            Handler::PixelPath => {
                let Board {
                    slot,
                    pixel_path,
                    irq2,
                    held,
                    vdcs,
                    opcode,
                    ..
                } = self;
                let mut pins = BoardPins {
                    irq2,
                    held,
                    vdcs,
                    opcode: *opcode,
                    card: slot.card.as_deref_mut(),
                };
                pixel_path.read(address, mclk, &mut pins)
            }
        }
    }

    /// The reference's `pages.write`.
    pub fn page_write(&mut self, address: u32, value: u8, mclk: i64) {
        let address = address & ADDRESS_MASK;
        match self.pages.handler(address) {
            Handler::Unmapped => {}
            Handler::Ram => self.ram.write(address, value),
            Handler::Vdc => self.vdcs[0].write(address & 3, value, mclk),
            Handler::Vce => self.vce.write(address & 7, value, mclk),
            Handler::CpuIo => self
                .cpu_io
                .write(address & 0x1FFF, value, mclk, &mut self.psg),
            Handler::Card => {
                let Board {
                    slot,
                    irq2,
                    held,
                    vdcs,
                    opcode,
                    ..
                } = self;
                let mut pins = BoardPins {
                    irq2,
                    held,
                    vdcs,
                    opcode: *opcode,
                    card: None,
                };
                if let Some(card) = slot.card.as_mut() {
                    card.write(address, value, mclk, &mut pins);
                }
            }
            Handler::Port => {
                let Board {
                    slot,
                    port,
                    irq2,
                    held,
                    vdcs,
                    opcode,
                    ..
                } = self;
                let mut pins = BoardPins {
                    irq2,
                    held,
                    vdcs,
                    opcode: *opcode,
                    card: slot.card.as_deref_mut(),
                };
                if let Some(port) = port.as_mut() {
                    port.write(address, value, mclk, &mut pins);
                }
            }
            Handler::PixelPath => {
                let Board {
                    slot,
                    pixel_path,
                    irq2,
                    held,
                    vdcs,
                    opcode,
                    ..
                } = self;
                let mut pins = BoardPins {
                    irq2,
                    held,
                    vdcs,
                    opcode: *opcode,
                    card: slot.card.as_deref_mut(),
                };
                pixel_path.write(address, value, mclk, &mut pins);
            }
        }
    }

    // -- the CPU ----------------------------------------------------------------------------------

    #[inline]
    fn cycle_clocks(pins: &Pins) -> i64 {
        if pins.high_speed {
            CPU_CLOCKS_HIGH
        } else {
            CPU_CLOCKS_LOW
        }
    }

    /// The fork's shortcut: an access to work RAM or to one of this crate's own cards, with
    /// nothing waiting (nothing left by the host, `Board::hold` included, or by a port
    /// device's `catch_up`), has nothing to finish: no stretch (the page is below the hardware
    /// page), no hold, no line or disable mask it could have changed, nothing scheduled.
    #[inline(always)]
    fn quiet_page(&self, address: u32) -> Option<Handler> {
        if self.unsettled || (address >> PAGE_BITS) as usize >= HARDWARE_FIRST_PAGE {
            return None;
        }
        match self.pages.handler(address) {
            Handler::Ram => Some(Handler::Ram),
            Handler::Card if self.plain_card => Some(Handler::Card),
            _ => None,
        }
    }

    /// The pins a card is handed (`page_read` and `page_write` make the same).
    #[inline(always)]
    fn card_pins(&mut self) -> (Option<&mut Box<dyn Card>>, BoardPins<'_>) {
        let Board {
            slot,
            irq2,
            held,
            vdcs,
            opcode,
            ..
        } = self;
        (
            slot.card.as_mut(),
            BoardPins {
                irq2,
                held,
                vdcs,
                opcode: *opcode,
                card: None,
            },
        )
    }

    fn finish_access(&mut self, address: u32, cycle: i64, pins: &mut Pins) {
        // The disable mask a write to $1402 left (the reference's `_irq_disable_changed`,
        // called from inside the write: nothing has touched the CPU since).
        if self.cpu_io.irq.mask_changed {
            self.cpu_io.irq.mask_changed = false;
            pins.set_irq_disable(self.cpu_io.irq.disable);
        }
        // [hw: pcetech 3 note 3] a VDC or VCE access takes one more cycle, reads and writes.
        // C4: ares charges it only outside burst mode and only for the VDC; Mednafen charges
        // a statistical VRAM penalty. MacDonald's rule, tier 2, is this model's.
        let page = (address >> PAGE_BITS) as usize;
        if (HARDWARE_FIRST_PAGE..HARDWARE_FIRST_PAGE + 8).contains(&page) {
            self.held += cycle;
        }
        for vdc in self.vdcs.iter_mut() {
            self.held += vdc.take_hold();
        }
        // The fork: the access may have scheduled something (a VRAM commit, the timer's start).
        self.wake = i64::MIN;
        self.events_at = i64::MIN;
        if self.held != 0 {
            let held = std::mem::take(&mut self.held);
            self.advance_with(held, Some(&mut *pins));
        }
        // the access itself (a status read) may have changed a line
        self.deliver_lines(Some(pins), false);
        if self.events_at == i64::MIN {
            self.look_ahead();
        }
        self.unsettled = self.port.is_some();
    }

    /// A device stretches the current cycle (RDY low); applied when the access returns.
    pub fn hold(&mut self, clocks: i64) {
        self.held += clocks;
        self.touch();
    }

    // -- time -------------------------------------------------------------------------------------

    /// Let `clocks` master clocks pass and run every device up to the new time, with no CPU
    /// to hand the interrupt lines to (the reference's `advance` on a board without one).
    pub fn advance(&mut self, clocks: i64) {
        self.touch();
        self.advance_with(clocks, None);
    }

    /// The fork's shortcut: a cycle that ends before the event horizon (`wake`) moves the
    /// clock and nothing else. The base board runs `run_to` and `deliver_lines` every cycle;
    /// before the horizon both find nothing to do (`horizon` says why).
    #[inline(always)]
    fn advance_with(&mut self, clocks: i64, pins: Option<&mut Pins>) {
        self.mclk += clocks;
        if self.mclk < self.wake {
            return;
        }
        self.advance_events(pins);
    }

    #[inline(never)]
    fn advance_events(&mut self, pins: Option<&mut Pins>) {
        if self.mclk < self.events_at {
            // Only with a port device: nothing is due but its `catch_up`, which may raise
            // IRQ2, hold the CPU or reach a VDC, so the lines and the horizon follow.
            self.port_catch_up(self.mclk);
        } else {
            self.run_to(self.mclk);
        }
        self.deliver_lines(pins, false);
        self.look_ahead();
    }

    fn run_to(&mut self, target: i64) {
        loop {
            let line_at = self.timeline.next_line_start();
            let row_at = self.row_end;
            let mut vdc_at = self.vdcs[0].next_event();
            for other in &self.vdcs[1..] {
                if let Some(at) = other.next_event() {
                    if vdc_at.is_none_or(|first| at < first) {
                        vdc_at = Some(at);
                    }
                }
            }
            let timer_at = self.cpu_io.next_event();
            if let Some(row_at) = row_at.filter(|&at| at <= target && at <= line_at) {
                self.end_row(row_at);
            } else if line_at <= target && vdc_at.is_none_or(|at| line_at <= at) {
                self.new_line(line_at);
            } else if let Some(vdc_at) = vdc_at.filter(|&at| at <= target) {
                for chip in self.vdcs.iter_mut() {
                    chip.catch_up(vdc_at);
                }
            } else if let Some(timer_at) = timer_at.filter(|&at| at <= target) {
                self.cpu_io.catch_up(timer_at);
            } else {
                break;
            }
        }
        for chip in self.vdcs.iter_mut() {
            chip.catch_up(target);
        }
        self.cpu_io.catch_up(target);
        self.port_catch_up(target);
    }

    /// socket 5, issue 0002: a port device acts by itself
    fn port_catch_up(&mut self, target: i64) {
        let Board {
            slot,
            port,
            irq2,
            held,
            vdcs,
            opcode,
            ..
        } = self;
        if let Some(port) = port.as_mut() {
            let mut pins = BoardPins {
                irq2,
                held,
                vdcs,
                opcode: *opcode,
                card: slot.card.as_deref_mut(),
            };
            port.catch_up(target, &mut pins);
        }
    }

    fn new_line(&mut self, at: i64) {
        for vdc in self.vdcs.iter_mut() {
            vdc.catch_up(at);
        }
        // the line that is ending; its row ends 237 clocks later
        self.row_timing = Some(self.timeline.timing(&self.vce));
        self.row_end = Some(at + HSYNC_CLOCKS);
        self.timeline.advance(&self.vce);
        let divider = self.vce.dot_divider();
        let vsync = self.timeline.line == self.timeline.frame_lines - 3;
        for vdc in self.vdcs.iter_mut() {
            vdc.hsync(at, divider);
            if vsync {
                vdc.vsync(); // [emu: Mednafen] after the hsync of the same line
            }
        }
    }

    /// The previous line's picture row ends as this line's sync does: hand it to the capture
    /// before the VDC starts the new line's geometry (events at `at` itself wait).
    fn end_row(&mut self, at: i64) {
        for vdc in self.vdcs.iter_mut() {
            vdc.catch_up(at - 1);
        }
        let timing = self.row_timing.expect("a row ends after a line began");
        let dots = (LINE_CLOCKS / timing.dot_divider) as usize;
        if self.direct_path {
            // The fork: `DirectPath::mix` returns the first VDC's row, so the capture reads
            // it where it lies (the base board copies every VDC's row for the path).
            match self.vdcs[0].drawn_row(dots) {
                Some(row) => self.capture.finish_line(&timing, row, &mut self.vce),
                None => {
                    let row = self.vdcs[0].pixel_row(dots);
                    self.capture.finish_line(&timing, &row, &mut self.vce);
                }
            }
            self.row_end = None;
            return;
        }
        let rows: Vec<Vec<u16>> = self.vdcs.iter().map(|vdc| vdc.pixel_row(dots)).collect();
        let row = self.pixel_path.mix(&timing, rows);
        self.capture.finish_line(&timing, &row, &mut self.vce);
        self.row_end = None;
    }

    // -- interrupts -------------------------------------------------------------------------------

    /// (IRQ1, IRQ2, TIQ) as the interrupt controller sees them, before the mask.
    pub fn lines(&self) -> (bool, bool, bool) {
        // IRQ1 is shared by every VDC (wired-OR; each is acknowledged by reading its status).
        (
            self.vdcs.iter().any(|vdc| vdc.irq1()),
            self.irq2.level(),
            self.cpu_io.tiq(),
        )
    }

    pub(crate) fn deliver_lines(&mut self, pins: Option<&mut Pins>, force: bool) {
        let lines = self.lines();
        self.cpu_io.irq.irq1 = lines.0;
        self.cpu_io.irq.irq2 = lines.1;
        if lines != self.lines || force {
            if let Some(pins) = pins {
                pins.set_irq_lines(lines.0, lines.1, lines.2);
            }
        }
        self.lines = lines;
    }

    // -- reset ------------------------------------------------------------------------------------

    pub fn reset(&mut self, kind: ResetKind) {
        self.touch();
        let Board {
            slot,
            port,
            pixel_path,
            irq2,
            held,
            vdcs,
            opcode,
            ..
        } = self;
        if let Some(card) = slot.card.as_mut() {
            let mut pins = BoardPins {
                irq2,
                held,
                vdcs,
                opcode: *opcode,
                card: None,
            };
            card.reset(kind, &mut pins);
        }
        if let Some(port) = port.as_mut() {
            let mut pins = BoardPins {
                irq2,
                held,
                vdcs,
                opcode: *opcode,
                card: slot.card.as_deref_mut(),
            };
            port.reset(kind, &mut pins);
        }
        let mut pins = BoardPins {
            irq2,
            held,
            vdcs,
            opcode: *opcode,
            card: slot.card.as_deref_mut(),
        };
        pixel_path.reset(kind, &mut pins);
        if kind == ResetKind::PowerOn {
            self.cpu_io.reset();
        }
    }
}

impl Bus for Board {
    fn read(&mut self, address: u32, pins: &mut Pins) -> u8 {
        self.opcode = pins.opcode();
        let cycle = Board::cycle_clocks(pins);
        self.advance_with(cycle, Some(pins));
        let address_in = address & ADDRESS_MASK;
        let value = match self.quiet_page(address_in) {
            Some(Handler::Ram) => self.ram.read(address_in),
            Some(_) => {
                let mclk = self.mclk;
                let (card, mut pins) = self.card_pins();
                match card {
                    Some(card) => card.read(address_in, mclk, &mut pins),
                    None => 0xFF,
                }
            }
            None => {
                let value = self.page_read(address, self.mclk);
                self.finish_access(address, cycle, pins);
                value
            }
        };
        if let Some(log) = self.bus_log.as_mut() {
            log.push((address as u64) << 8 | value as u64);
            log.push((self.mclk as u64) << 2);
        }
        if let Some(detail) = self.bus_detail.as_mut() {
            detail.push((0, address, value, self.mclk));
        }
        value
    }

    fn write(&mut self, address: u32, value: u8, pins: &mut Pins) {
        self.opcode = pins.opcode();
        let cycle = Board::cycle_clocks(pins);
        self.advance_with(cycle, Some(pins));
        let address_in = address & ADDRESS_MASK;
        match self.quiet_page(address_in) {
            Some(Handler::Ram) => self.ram.write(address_in, value),
            Some(_) => {
                let mclk = self.mclk;
                let (card, mut pins) = self.card_pins();
                if let Some(card) = card {
                    card.write(address_in, value, mclk, &mut pins);
                }
            }
            None => {
                self.page_write(address, value, self.mclk);
                self.finish_access(address, cycle, pins);
            }
        }
        if let Some(log) = self.bus_log.as_mut() {
            log.push((address as u64) << 8 | value as u64);
            log.push((self.mclk as u64) << 2 | 1);
        }
        if let Some(detail) = self.bus_detail.as_mut() {
            detail.push((1, address, value, self.mclk));
        }
    }

    /// A cycle with no bus transfer (internal cycle or dummy read: nothing reacts).
    fn idle(&mut self, _address: Option<u32>, pins: &mut Pins) {
        self.opcode = pins.opcode();
        let cycle = Board::cycle_clocks(pins);
        self.advance_with(cycle, Some(pins));
        if let Some(log) = self.bus_log.as_mut() {
            log.push((self.mclk as u64) << 2 | 2);
        }
        if let Some(detail) = self.bus_detail.as_mut() {
            detail.push((2, 0, 0, self.mclk));
        }
    }
}
