//! Save states (`state.py`): the whole console between two instructions, as bytes, and back.
//!
//! As in the reference, a state is everything that changes as the console runs: the CPU
//! (huc6280-rust's `CpuState`), the board's clock and lines, the VDCs, the VCE, the PSG, the
//! timer and the interrupt registers, work RAM, the card's mapper and its RAM, the capture and
//! the mixer, and whatever an add-on has plugged in. It is *not* the wiring: a state is loaded
//! into a `System` built the same way (the same machine row, the same card, the same add-on),
//! which keeps its configuration, its page table and its devices.
//!
//! **Equivalence** is the test (tests/state.rs, `pce-state-roundtrip`): a run saved at some
//! instruction boundary and loaded into another system continues exactly as the run that was
//! never interrupted, bus cycle for bus cycle; and the loaded system, saved at once, gives the
//! same bytes back.
//!
//! **Refused**, with an error and the system untouched: bytes that are not a state of this
//! format and version, another machine row, another card (its identity is the SHA-1 and length
//! of the image: no ROM byte is in a state), a system built another way (a device in a socket
//! the state has empty, or another device there, or other media in it). A state refused further
//! in (a device that cannot read its part) may have written part of itself, and the system
//! should be rebuilt.
//!
//! Differences forced by Rust:
//!
//! - **The format is this crate's own**: a magic and a version, then zlib of the bincode of a
//!   header and the body. The reference's is tagged JSON of a walk of the object graph, which
//!   Rust cannot make; a state is not portable to the reference or back. What holds is the
//!   reference's property.
//! - **Devices behind a socket trait save themselves** (`sockets::DeviceState`): bytes out,
//!   bytes in, and an identity. The reference reaches them by the same walk.
//! - **The named switches**: the reference saves instance attributes (the VDC's `vblank_dots`,
//!   `vertical_at_line_start`, the DMA's `resume`) and so does this; its class attributes (the
//!   PSG's switches, the timer's window) are not in a state there, and a loaded state keeps
//!   the system's own here too, with the VCE's host-side `rgb_table`.
//! - **Old states are refused, not converted.** The reference's JSON names each field, so it
//!   loads a state of 0.1.0 with the CPU's new `polled_irq_lines` as `None` (pcengine-python
//!   0.2.0). bincode names nothing: a body of version 1 does not decode as version 2, and the
//!   version check refuses it first, with the system untouched (tests/state.rs). A converter
//!   would carry the old layouts of the CPU and the PSG for states nobody keeps (the crate has
//!   no users who save yet); a state is for the build that made it.
//! - **A snapshot pair** the reference does not have: `save_snapshot` / `load_snapshot`, the
//!   body alone with no header, no check and no compression, for run-ahead (a state taken and
//!   put back within one run of one system). Loading a snapshot into a system built another
//!   way is not detected.

use std::borrow::Cow;
use std::io::{Read, Write};

use huc6280::CpuState;
use serde::{Deserialize, Serialize};

use crate::cpu_io::{IoBuffer, IrqController, Timer};
use crate::mixer::Mixer;
use crate::pagetable::Ram;
use crate::psg::Psg;
use crate::sockets::{IrqLine, VideoTiming};
use crate::system::System;
use crate::vce::Vce;
use crate::vdc::Vdc;
use crate::video::{Capture, Timeline};

pub const MAGIC: &[u8; 8] = b"PCERSTAT";
/// 2 since 0.2.0: the CPU's state gained `polled_irq_lines` (huc6280-rust 0.2.0), the PSG lost
/// `acc_steps` and gained `levels_units`. A state of version 1 is refused, not converted (see the
/// module's notes).
pub const VERSION: u32 = 2;

/// huc6280-rust's `CpuState`, field for field (it does not derive serde).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Cpu {
    a: u8,
    x: u8,
    y: u8,
    s: u8,
    pc: u16,
    p: u8,
    mpr: [u8; 8],
    high_speed: bool,
    mpr_latch: u8,
    irq1: bool,
    irq2: bool,
    tiq: bool,
    irq_disable: u8,
    halted: bool,
    reset_pending: bool,
    maskable_interrupt_pending: bool,
    non_maskable_interrupt_pending: bool,
    polled_i: Option<bool>,
    polled_maskable_interrupt: Option<bool>,
    polled_non_maskable_interrupt: Option<bool>,
    skipped_poll: bool,
    /// huc6280-rust 0.2.0 (huc6280-python 0.4.0, its issue 0005)
    polled_irq_lines: Option<u8>,
}

impl From<CpuState> for Cpu {
    fn from(s: CpuState) -> Cpu {
        Cpu {
            a: s.a,
            x: s.x,
            y: s.y,
            s: s.s,
            pc: s.pc,
            p: s.p,
            mpr: s.mpr,
            high_speed: s.high_speed,
            mpr_latch: s.mpr_latch,
            irq1: s.irq1,
            irq2: s.irq2,
            tiq: s.tiq,
            irq_disable: s.irq_disable,
            halted: s.halted,
            reset_pending: s.reset_pending,
            maskable_interrupt_pending: s.maskable_interrupt_pending,
            non_maskable_interrupt_pending: s.non_maskable_interrupt_pending,
            polled_i: s.polled_i,
            polled_maskable_interrupt: s.polled_maskable_interrupt,
            polled_non_maskable_interrupt: s.polled_non_maskable_interrupt,
            skipped_poll: s.skipped_poll,
            polled_irq_lines: s.polled_irq_lines, // saved
        }
    }
}

impl From<&Cpu> for CpuState {
    fn from(s: &Cpu) -> CpuState {
        CpuState {
            a: s.a,
            x: s.x,
            y: s.y,
            s: s.s,
            pc: s.pc,
            p: s.p,
            mpr: s.mpr,
            high_speed: s.high_speed,
            mpr_latch: s.mpr_latch,
            irq1: s.irq1,
            irq2: s.irq2,
            tiq: s.tiq,
            irq_disable: s.irq_disable,
            halted: s.halted,
            reset_pending: s.reset_pending,
            maskable_interrupt_pending: s.maskable_interrupt_pending,
            non_maskable_interrupt_pending: s.non_maskable_interrupt_pending,
            polled_i: s.polled_i,
            polled_maskable_interrupt: s.polled_maskable_interrupt,
            polled_non_maskable_interrupt: s.polled_non_maskable_interrupt,
            skipped_poll: s.skipped_poll,
            polled_irq_lines: s.polled_irq_lines,
        }
    }
}

/// What a state is a state of: checked before anything is written.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
struct Header {
    machine: String,
    card: Option<String>,
    port: Option<String>,
    pad: Option<String>,
    pixel_path: String,
    audio_sources: Vec<String>,
}

fn header(system: &System) -> Header {
    let board = system.board();
    Header {
        machine: board.machine.name.to_string(),
        card: board.slot.card.as_ref().map(|card| card.state_identity()),
        port: board.port.as_ref().map(|port| port.state_identity()),
        pad: board
            .cpu_io
            .port
            .device
            .as_ref()
            .map(|pad| pad.state_identity()),
        pixel_path: board.pixel_path.state_identity(),
        audio_sources: board
            .audio_sources
            .iter()
            .map(|source| source.state_identity())
            .collect(),
    }
}

#[derive(Serialize, Deserialize)]
struct Body<'a> {
    cpu: Cpu,
    mclk: i64,
    held: i64,
    lines: (bool, bool, bool),
    row_end: Option<i64>,
    row_timing: Option<VideoTiming>,
    irq2: Cow<'a, IrqLine>,
    vce: Cow<'a, Vce>,
    vdcs: Cow<'a, [Vdc]>,
    psg: Cow<'a, Psg>,
    buffer: IoBuffer,
    timer: Timer,
    irq: IrqController,
    latch: u8,
    ram: Cow<'a, Ram>,
    mixer: Cow<'a, Mixer>,
    timeline: Cow<'a, Timeline>,
    capture: Cow<'a, Capture>,
    card: Option<Vec<u8>>,
    port: Option<Vec<u8>>,
    pad: Option<Vec<u8>>,
    pixel_path: Vec<u8>,
    audio_sources: Vec<Vec<u8>>,
}

fn body(system: &System) -> Body<'_> {
    let board = system.board();
    Body {
        cpu: system.cpu.capture_state().into(),
        mclk: board.mclk,
        held: board.held,
        lines: board.lines,
        row_end: board.row_end,
        row_timing: board.row_timing,
        irq2: Cow::Borrowed(&board.irq2),
        vce: Cow::Borrowed(&board.vce),
        vdcs: Cow::Borrowed(&board.vdcs[..]),
        psg: Cow::Borrowed(&board.psg),
        buffer: board.cpu_io.buffer,
        timer: board.cpu_io.timer,
        irq: board.cpu_io.irq,
        latch: board.cpu_io.port.latch,
        ram: Cow::Borrowed(&board.ram),
        mixer: Cow::Borrowed(&board.mixer),
        timeline: Cow::Borrowed(&board.timeline),
        capture: Cow::Borrowed(&board.capture),
        card: board.slot.card.as_ref().map(|card| card.save_state()),
        port: board.port.as_ref().map(|port| port.save_state()),
        pad: board
            .cpu_io
            .port
            .device
            .as_ref()
            .map(|pad| pad.save_state()),
        pixel_path: board.pixel_path.save_state(),
        audio_sources: board
            .audio_sources
            .iter()
            .map(|source| source.save_state())
            .collect(),
    }
}

/// Write a decoded body into the system. The sockets were checked (or, for a snapshot, are
/// the system's own).
fn apply(system: &mut System, body: Body) -> Result<(), String> {
    let board = system.board();
    if body.vdcs.len() != board.vdcs.len() || body.ram.data.len() != board.ram.data.len() {
        return Err("a state of a machine built another way (VDCs, work RAM)".to_string());
    }
    if body.audio_sources.len() != board.audio_sources.len()
        || body.card.is_some() != board.slot.card.is_some()
        || body.port.is_some() != board.port.is_some()
        || body.pad.is_some() != board.cpu_io.port.device.is_some()
    {
        return Err("a state of a machine built another way (the sockets)".to_string());
    }
    system.cpu.restore_state(&CpuState::from(&body.cpu))?;
    let board = system.board_mut();
    board.mclk = body.mclk;
    board.held = body.held;
    board.lines = body.lines;
    board.row_end = body.row_end;
    board.row_timing = body.row_timing;
    board.irq2 = body.irq2.into_owned();
    let rgb_table = std::mem::take(&mut board.vce.rgb_table);
    board.vce = body.vce.into_owned();
    board.vce.rgb_table = rgb_table;
    let wiring: Vec<bool> = board.vdcs.iter().map(|vdc| vdc.serve).collect();
    board.vdcs = body.vdcs.into_owned();
    for (vdc, serve) in board.vdcs.iter_mut().zip(wiring) {
        vdc.serve = serve; // wiring, not state
    }
    let mut psg = body.psg.into_owned();
    psg.keep_configuration(&board.psg);
    board.psg = psg;
    board.cpu_io.buffer = body.buffer;
    let window = board.cpu_io.timer.underflow_window_mclk;
    board.cpu_io.timer = body.timer;
    board.cpu_io.timer.underflow_window_mclk = window;
    board.cpu_io.irq = body.irq;
    board.cpu_io.port.latch = body.latch;
    board.ram = body.ram.into_owned();
    board.mixer = body.mixer.into_owned();
    board.timeline = body.timeline.into_owned();
    board.capture = body.capture.into_owned();
    if let (Some(card), Some(data)) = (board.slot.card.as_mut(), body.card.as_ref()) {
        card.load_state(data)?;
    }
    if let (Some(port), Some(data)) = (board.port.as_mut(), body.port.as_ref()) {
        port.load_state(data)?;
    }
    if let (Some(pad), Some(data)) = (board.cpu_io.port.device.as_mut(), body.pad.as_ref()) {
        pad.load_state(data)?;
    }
    board.pixel_path.load_state(&body.pixel_path)?;
    for (source, data) in board.audio_sources.iter_mut().zip(&body.audio_sources) {
        source.load_state(data)?;
    }
    Ok(())
}

/// The state of `system` as bytes (between two of its steps).
pub fn save_state(system: &System) -> Vec<u8> {
    let mut out = MAGIC.to_vec();
    out.extend_from_slice(&VERSION.to_le_bytes());
    let mut encoder = flate2::write::ZlibEncoder::new(out, flate2::Compression::new(6));
    let header = bincode::serialize(&header(system)).expect("a header serialises");
    encoder
        .write_all(&(header.len() as u32).to_le_bytes())
        .and_then(|_| encoder.write_all(&header))
        .expect("writing to memory");
    bincode::serialize_into(&mut encoder, &body(system)).expect("a state serialises");
    encoder.finish().expect("writing to memory")
}

/// Put `system` in the state `data` was saved from (the same machine, card and add-on).
pub fn load_state(system: &mut System, data: &[u8]) -> Result<(), String> {
    if data.len() < MAGIC.len() + 4 || &data[..MAGIC.len()] != MAGIC {
        return Err("not a pcengine-rust state".to_string());
    }
    let version = u32::from_le_bytes(data[MAGIC.len()..MAGIC.len() + 4].try_into().unwrap());
    if version != VERSION {
        return Err(format!(
            "a pcengine-rust state of version {version}, not {VERSION}"
        ));
    }
    let mut plain = Vec::new();
    flate2::read::ZlibDecoder::new(&data[MAGIC.len() + 4..])
        .read_to_end(&mut plain)
        .map_err(|error| format!("not a save state: {error}"))?;
    if plain.len() < 4 {
        return Err("not a save state: too short".to_string());
    }
    let length = u32::from_le_bytes(plain[..4].try_into().unwrap()) as usize;
    if plain.len() < 4 + length {
        return Err("not a save state: too short".to_string());
    }
    let saved: Header = bincode::deserialize(&plain[4..4 + length])
        .map_err(|error| format!("not a save state: {error}"))?;
    let ours = header(system);
    if saved.machine != ours.machine {
        return Err(format!(
            "a state of the {}, not the {}",
            saved.machine, ours.machine
        ));
    }
    if saved.card != ours.card {
        return Err("a state of another ROM image (another card?)".to_string());
    }
    if saved.port != ours.port {
        return Err(
            "a state made with another expansion device or other media (another disc?)".to_string(),
        );
    }
    if saved != ours {
        return Err("a state of a machine built another way".to_string());
    }
    let body: Body = bincode::deserialize(&plain[4 + length..])
        .map_err(|error| format!("not a save state: {error}"))?;
    apply(system, body)
}

/// The state of `system` for `load_snapshot` on the same system later: no header, no
/// compression, nothing hashed.
pub fn save_snapshot(system: &System) -> Vec<u8> {
    bincode::serialize(&body(system)).expect("a state serialises")
}

/// Put `system` back where `save_snapshot` found it (the system the snapshot was taken from,
/// or one built the same way: nothing is checked).
pub fn load_snapshot(system: &mut System, data: &[u8]) -> Result<(), String> {
    let body: Body =
        bincode::deserialize(data).map_err(|error| format!("not a snapshot: {error}"))?;
    apply(system, body)
}
