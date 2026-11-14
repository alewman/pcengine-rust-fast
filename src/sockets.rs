//! The sockets: narrow traits an add-on crate implements to plug into this
//! board (`sockets.py`; the reference's docs/sockets.md).
//!
//! `pcenginecd-rust` and `supergrafx-rust` depend on this crate and never copy
//! from it, as the reference's add-ons pin the Python board. Test-only fakes
//! for each socket live in `src/fake_devices.rs`; the base board implements
//! only what it uses.
//!
//! Numbering follows docs/sockets.md.
//!
//!  1. `CardSlot` / `Card`     the HuCard slot
//!  2. `PortDevice`            the expansion port (EXT BUS)
//!  3. `PageHandler`           one decode for every bus master (the page table is in pagetable.rs)
//!  4. `BoardPins::hold`       bus timing: wait states go through the board
//!  5. `PortDevice::catch_up`  clock domains: run up to `mclk`, callable inside an access
//!  6. `IrqLine`               interrupt levels, one bit per source, wired-OR
//!  7. `PixelPath`             per-line 9-bit pixels between the VDC(s) and the VCE
//!  8. `VideoTiming`           one record per line
//!  9. `AudioSource`           a source with a changeable rate and gain
//! 10. `ResetKind`             reset events
//! 11. `ControllerPort`        the pad port
//! 12. Instances               `Vdc`, `Vce`, `Psg` hold no module state
//!
//! Differences forced by Rust:
//!
//! - **`BoardPins` is handed to a device at every call**, not once by `bind`.
//!   The reference's devices keep the board's IRQ2 line and its `hold`
//!   callable; a Rust device cannot hold a reference into the board that owns
//!   it, so each access, `catch_up` and `reset` gets `&mut BoardPins`: the
//!   IRQ2 line, `hold`, the VDC instances (the SuperGrafx's VPC decodes their
//!   page and steers to either), the opcode of the instruction being run (the
//!   VPC steers only `ST0`-`ST2`) and, for the port device and the pixel
//!   path, the card in the slot (a CD unit answers a Super System Card's id
//!   registers from the card). The reference's add-ons reach the same things
//!   through references they were built with.
//! - **A device that is two things in the reference is one trait object
//!   here.** The reference's `VideoPage` and `Vpc` share state by reference
//!   (one claims the VDC pages, the other is the pixel path); here `PixelPath`
//!   may also claim pages. The CD unit's CD-DA and ADPCM players, appended to
//!   `board.audio_sources` in the reference, are answered by
//!   `PortDevice::audio_sources`, mixed after the PSG and before the board's
//!   own list, which is the reference's order (PSG, CD-DA, ADPCM).
//! - **Save states** cannot walk an object graph: every socket trait carries
//!   `DeviceState` (bytes out, bytes in, an identity for media), which the
//!   reference gets from `register_package`.
//! - **Pad input**: the reference's host keeps the pad object and sets
//!   `pad.pressed`; here the board owns the device, so `ControllerPort` has
//!   `set_pressed(port, mask)`.

use std::any::Any;
use std::collections::BTreeSet;

use serde::{Deserialize, Serialize};

use crate::vdc::Vdc;

// --- 10. Reset ---------------------------------------------------------------------------------

/// The PC Engine has no reset button or reset instruction on the console itself; a
/// CD unit is reset by a register write inside the add-on (docs/sockets.md). `Button` exists for
/// the Duo/Shuttle-class consoles (unverified).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResetKind {
    PowerOn,
    Button,
}

// --- 6. Interrupts -----------------------------------------------------------------------------

/// A wired-OR level with one bit per source (never a max).
///
/// The line is high while any source holds it; a source lowers its own bit when acknowledged.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IrqLine {
    pub name: String,
    sources: BTreeSet<String>,
}

impl IrqLine {
    pub fn new(name: &str) -> IrqLine {
        IrqLine {
            name: name.to_string(),
            sources: BTreeSet::new(),
        }
    }

    pub fn set(&mut self, source: &str, level: bool) {
        if level {
            if !self.sources.contains(source) {
                self.sources.insert(source.to_string());
            }
        } else {
            self.sources.remove(source);
        }
    }

    pub fn level(&self) -> bool {
        !self.sources.is_empty()
    }

    pub fn sources(&self) -> &BTreeSet<String> {
        &self.sources
    }
}

// --- 3 and 4. The decode and bus timing, seen from a device --------------------------------------

/// What the board hands a card, a port device or the pixel path with every call.
pub struct BoardPins<'a> {
    /// The shared IRQ2 level (HuCard, expansion port, CD ADPCM): the device sets its own bit.
    pub irq2: &'a mut IrqLine,
    pub(crate) held: &'a mut i64,
    /// The board's VDC instances (`machine.vdc_count` of them): the SuperGrafx's VPC claims
    /// their pages and steers each access to one. A device makes its accesses of a VDC's port
    /// with `vdc_read` and `vdc_write`, not by calling `Vdc::read` and `write` here, so that the
    /// board's wait reaches them (issue 0004, `VdcAccess`).
    pub vdcs: &'a mut [Vdc],
    pub(crate) access: &'a mut VdcAccess,
    /// The opcode of the instruction the CPU is running (`Pins::opcode`; 0 with no CPU): the
    /// VPC steers only `ST0`-`ST2`, which the bus cannot tell from stores to `$FF:0000/2/3`.
    pub opcode: u8,
    /// The card in the slot, for the port device and the pixel path (`None` for the card's own
    /// calls and for an empty slot).
    pub card: Option<&'a mut (dyn Card + 'static)>,
}

impl BoardPins<'_> {
    /// Stretch the current CPU cycle by this many master clocks (RDY held low), applied by the
    /// board once the access returns.
    pub fn hold(&mut self, clocks: i64) {
        *self.held += clocks;
    }

    /// VDC `index`'s `read(offset, mclk)`, through the board's wait (`VdcAccess`).
    pub fn vdc_read(&mut self, index: usize, offset: u32, mclk: i64) -> u8 {
        self.access.read(self.vdcs, index, offset, mclk)
    }

    /// VDC `index`'s `write(offset, value, mclk)`, through the board's wait (`VdcAccess`).
    pub fn vdc_write(&mut self, index: usize, offset: u32, value: u8, mclk: i64) {
        self.access.write(self.vdcs, index, offset, value, mclk);
    }
}

/// The VDC accesses of one decode, for the board's wait (issue 0004).
///
/// In the reference a VDC access that has to wait calls the board (`Board._serve_vram`), which
/// runs every device event by event, the line clock and the pixel path included, until the
/// access is served, and the access then goes on; a device that touches two VDCs in one access
/// (the lockstep pixel path writes both on `ST0`-`ST2`) touches the second after the first's
/// wait. A Rust device cannot be called again by the board while it is in the middle of an
/// access, so the access *stops* there instead (vdc/chip.rs, `Vdc::serve_wanted`): every later
/// VDC access of the decode is skipped (it answers $FF to a device whose access is thrown
/// away), the board serves the chip, and the board makes the whole decode again. In the new
/// pass the accesses made before the stop are not made twice: their results are handed back
/// from the log, in order; the stopped access is made at the time the wait ended; the later
/// ones are made as the reference makes them after its wait, with the device's own `mclk`.
/// This asks of a device that an access whose VDC access stops changes nothing of its own
/// (the VPC's and the lockstep devices' do not); a device's decode is deterministic.
#[derive(Clone, Debug, Default)]
pub struct VdcAccess {
    /// The results of the VDC accesses of this decode, in order (0 for a write).
    log: Vec<u8>,
    /// How many of `log` this pass has handed back.
    cursor: usize,
    /// The VDC whose access stopped in this pass.
    stopped: Option<usize>,
    /// The VDC served and the time its stopped access is made at in the next pass.
    again: Option<(usize, i64)>,
}

impl VdcAccess {
    /// A new decode.
    pub(crate) fn begin(&mut self) {
        self.log.clear();
        self.cursor = 0;
        self.stopped = None;
        self.again = None;
    }

    /// The VDC whose access stopped to wait in the pass just made, if any.
    #[inline]
    pub(crate) fn stopped(&self) -> Option<usize> {
        self.stopped
    }

    /// The next pass of the decode, after VDC `index` was served at `at`.
    pub(crate) fn again(&mut self, index: usize, at: i64) {
        self.cursor = 0;
        self.stopped = None;
        self.again = Some((index, at));
    }

    /// `None` to make the access now (at the time it should be made at), or the answer to hand
    /// back without making it.
    #[inline]
    fn enter(&mut self, index: usize, mclk: &mut i64) -> Option<u8> {
        if self.stopped.is_some() {
            return Some(0xFF); // after a stop: made in the next pass
        }
        if self.cursor < self.log.len() {
            let value = self.log[self.cursor];
            self.cursor += 1;
            return Some(value); // made before a stop in an earlier pass
        }
        if let Some((served, at)) = self.again.take() {
            debug_assert_eq!(
                served, index,
                "the stopped access comes first after the log"
            );
            *mclk = at;
        }
        None
    }

    #[inline]
    fn leave(&mut self, vdc: &Vdc, index: usize, value: u8) -> u8 {
        if vdc.serve_wanted {
            self.stopped = Some(index);
            return 0xFF;
        }
        self.log.push(value);
        self.cursor += 1;
        value
    }

    pub(crate) fn read(
        &mut self,
        vdcs: &mut [Vdc],
        index: usize,
        offset: u32,
        mut mclk: i64,
    ) -> u8 {
        if let Some(value) = self.enter(index, &mut mclk) {
            return value;
        }
        let value = vdcs[index].read(offset, mclk);
        self.leave(&vdcs[index], index, value)
    }

    pub(crate) fn write(
        &mut self,
        vdcs: &mut [Vdc],
        index: usize,
        offset: u32,
        value: u8,
        mut mclk: i64,
    ) {
        if self.enter(index, &mut mclk).is_some() {
            return;
        }
        vdcs[index].write(offset, value, mclk);
        self.leave(&vdcs[index], index, 0);
    }
}

/// A `BoardPins` with no board behind it: for a device's own tests (this crate's and an
/// add-on's), which call its `read`, `write` and `catch_up` directly.
#[derive(Default)]
pub struct DetachedPins {
    pub irq2: IrqLine,
    pub held: i64,
    pub vdcs: Vec<Vdc>,
    pub opcode: u8,
    pub access: VdcAccess,
}

impl DetachedPins {
    pub fn pins(&mut self) -> BoardPins<'_> {
        BoardPins {
            irq2: &mut self.irq2,
            held: &mut self.held,
            vdcs: &mut self.vdcs,
            access: &mut self.access,
            opcode: self.opcode,
            card: None,
        }
    }
}

/// What a save state needs of a socket's device (the reference walks the object graph).
pub trait DeviceState {
    /// Everything of the device that changes as the console runs.
    fn save_state(&self) -> Vec<u8> {
        Vec::new()
    }

    /// Put the device in a saved state. A device without state accepts only an empty one.
    fn load_state(&mut self, data: &[u8]) -> Result<(), String> {
        if data.is_empty() {
            Ok(())
        } else {
            Err("a state for a device that keeps none".to_string())
        }
    }

    /// What the device is and which media it holds (a card's ROM by its SHA-1, a disc by its
    /// table of contents): a state is refused by a system whose device answers otherwise.
    fn state_identity(&self) -> String {
        String::new()
    }
}

/// Anything a range of 256-byte pages can be claimed by.
///
/// `address` is the 21-bit physical address after the MMU; `mclk` is the master-clock time of
/// the access, so a device may run itself up to it inside the access (the CD unit's registers
/// need that).
pub trait PageHandler {
    fn read(&mut self, address: u32, mclk: i64, pins: &mut BoardPins) -> u8;

    fn write(&mut self, address: u32, value: u8, mclk: i64, pins: &mut BoardPins);
}

// --- 1. The HuCard slot --------------------------------------------------------------------------

/// A HuCard: standard ROM, SF2, Populous, Tennokoe, a System Card, ...
///
/// A card is not confined to the ROM window (a Super System Card claims banks $68-$7F as RAM
/// and the $18C1-$18C6 id registers), and a claim replaces the page's handler.
pub trait Card: PageHandler + DeviceState {
    /// The /CD line: a card grounds it so a CD unit can tell one is inserted.
    /// [doc: wiki HuCard pinout]
    fn grounds_cd(&self) -> bool;

    /// Inclusive (first_page, last_page) ranges over the 8192 pages of 256 bytes.
    fn claims(&self) -> Vec<(usize, usize)>;

    fn reset(&mut self, kind: ResetKind, pins: &mut BoardPins);

    /// For an add-on that knows which card it built (the CD unit and its System Card).
    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// The slot. Empty is a real state: it answers $FF, does not ground /CD, and the I/O
/// port's CD-sense bit follows the expansion port, not the slot (docs/sockets.md).
#[derive(Default)]
pub struct CardSlot {
    pub card: Option<Box<dyn Card>>,
}

impl CardSlot {
    pub fn new(card: Option<Box<dyn Card>>) -> CardSlot {
        CardSlot { card }
    }

    pub fn grounds_cd(&self) -> bool {
        self.card.as_ref().is_some_and(|card| card.grounds_cd())
    }
}

// --- 2. The expansion port ---------------------------------------------------------------------

/// The EXT BUS device: A0-A20, D0-D7, /RD, /WR, IRQ2, audio in, K7 (I/O port bit 7).
///
/// The video-bus pins (VD0-8, HSYNC, VSYNC, DCK, VCESEL) are not a socket until a device that
/// uses them is found (U13).
pub trait PortDevice: PageHandler + DeviceState {
    /// K7: a device on the port pulls the I/O port's D7 low ("CD unit attached", C6 polarity).
    fn present(&self) -> bool;

    fn claims(&self) -> Vec<(usize, usize)>;

    /// Socket 5: the device keeps its own rational ratio and remainder and runs up to `mclk`
    /// when asked; the board asks at every advance (the reference's issue 0002).
    fn catch_up(&mut self, mclk: i64, pins: &mut BoardPins);

    fn reset(&mut self, kind: ResetKind, pins: &mut BoardPins);

    /// Socket 9: the device's own mixer inputs (CD-DA, ADPCM), in a fixed order; mixed after
    /// the PSG.
    fn audio_sources(&mut self) -> Vec<&mut dyn AudioSource> {
        Vec::new()
    }

    fn as_any_mut(&mut self) -> &mut dyn Any;
}

// --- 7 and 8. Pixels and per-line timing ---------------------------------------------------------

/// One record per line, handed to anything that mixes or captures video.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct VideoTiming {
    /// frame counter since power-on
    pub frame: i64,
    /// line index within the frame, 0 = first line after the vertical sync
    pub line: i64,
    /// master-clock time of the line's start (the VCE's hsync)
    pub start_mclk: i64,
    /// master clocks per dot: 4, 3 or 2 (VCE dot clock at the line's start)
    pub dot_divider: i64,
    /// 262 or 263
    pub frame_lines: i64,
    pub in_vsync: bool,
}

/// Where the VDC(s) meet the VCE. The default is VDC 1 straight into the VCE; the
/// SuperGrafx's VPC replaces it, mixing two VDC lines. Line granularity, not dot.
///
/// Each row is the VDC's 9-bit pixel values for one line: bit 8 sprite/background, bits 7-4
/// palette, bits 3-0 colour [hw: sgxtech], with palette index 0x000/0x100 for the
/// backdrop/overscan colour. Returns the row handed to the VCE.
///
/// A path may also claim pages (the VPC decodes `$FF:0000-03FF`): `claims`, `read` and `write`
/// default to none.
pub trait PixelPath: DeviceState {
    fn mix(&mut self, timing: &VideoTiming, rows: Vec<Vec<u16>>) -> Vec<u16>;

    fn claims(&self) -> Vec<(usize, usize)> {
        Vec::new()
    }

    fn read(&mut self, address: u32, mclk: i64, pins: &mut BoardPins) -> u8 {
        let _ = (address, mclk, pins);
        0xFF
    }

    fn write(&mut self, address: u32, value: u8, mclk: i64, pins: &mut BoardPins) {
        let _ = (address, value, mclk, pins);
    }

    /// Called by `Board::reset` after the card and the port device (the reference's add-on
    /// wrapper resets its VPC itself).
    fn reset(&mut self, kind: ResetKind, pins: &mut BoardPins) {
        let _ = (kind, pins);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any;
}

/// VDC 1 straight into the VCE.
#[derive(Default)]
pub struct DirectPath;

impl DeviceState for DirectPath {}

impl PixelPath for DirectPath {
    fn mix(&mut self, _timing: &VideoTiming, mut rows: Vec<Vec<u16>>) -> Vec<u16> {
        rows.swap_remove(0)
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// --- 9. Audio ----------------------------------------------------------------------------------

/// A mixer input with a changeable rate and gain. The PSG is board-owned and first in the
/// list; the CD adds CD-DA (44.1 kHz) and ADPCM (variable).
pub trait AudioSource: DeviceState {
    fn rate_hz(&self) -> f64;

    fn gain(&self) -> f64;

    /// True when the source's samples already carry its gain (the PSG): the mixer must not
    /// apply it a second time.
    fn applies_gain(&self) -> bool {
        false
    }

    /// Stereo samples produced up to `mclk` since the last call.
    fn take_samples(&mut self, mclk: i64) -> Vec<(i32, i32)>;
}

// --- 11. Controllers ---------------------------------------------------------------------------

/// What plugs into the pad port: a 2-button pad, a multitap, a 6-button pad, a mouse.
///
/// SEL and CLR are the two lines the console drives ($1000 D0 and D1); the device answers a
/// 4-bit nibble on D3-D0. The SuperGrafx S-EXP port shares the nibble through a mux and exposes
/// output bits 2-7 (add-on).
pub trait ControllerPort: DeviceState {
    fn write(&mut self, sel: bool, clr: bool);

    fn read(&mut self, sel: bool, clr: bool) -> u8;

    /// The host's side: the buttons held on pad `port` (0 for a pad plugged straight in, 0-4
    /// behind a multitap), as the bit mask of pads.rs.
    fn set_pressed(&mut self, port: usize, pressed: u16) {
        let _ = (port, pressed);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any;
}
