// SPDX-License-Identifier: MIT
//! Test devices for the sockets (the reference's docs/sockets.md): every trait is exercised
//! without any add-on.
//!
//! The first group is the reference's `tests/fake_devices.py`, name for name (`FakeCard`,
//! `FakePort`, `FakeAudioSource`, `FakeController`, `FakePixelPath`, `ScriptedCpu`), used by
//! tests/sockets.rs as the reference's `tests/test_sockets.py` uses them.
//!
//! The second group (`LockstepCard`, `LockstepPort`, `LockstepPath`) are devices a real game
//! can run on, so that each socket is driven in lockstep with the same device on the
//! reference's side (`scripts/pce_trace.py` defines them again in Python, line for line): a
//! card whose RAM replaces the work RAM and that holds the CPU now and then, a port device
//! with an IRQ2 source of its own and an audio source at another rate, and a pixel path that
//! decodes the VDC page for two VDCs and mixes their rows. They are not models of any hardware.
//!
//! Differences forced by Rust: a device gets `BoardPins` with each call (sockets.rs), so
//! `FakePort::set_irq2` takes them; the reference's `Alarm` subclass of `FakePort` is its
//! `alarm_due` field; `ScriptedCpu` owns the board and a `Pins` of its own, and what it was
//! told of the interrupt lines is `pins.irq_lines()`.

use std::any::Any;

use huc6280::{Bus, Pins};
use serde::{Deserialize, Serialize};

use crate::board::Board;
use crate::cards::RomCard;
use crate::machine::{py_floordiv, HARDWARE_FIRST_PAGE, MASTER_HZ};
use crate::sockets::{
    AudioSource, BoardPins, Card, ControllerPort, DeviceState, PageHandler, PixelPath, PortDevice,
    ResetKind, VideoTiming,
};

// --- the reference's fakes ------------------------------------------------------------------------

/// A card claiming arbitrary page ranges; every access is recorded.
pub struct FakeCard {
    ranges: Vec<(usize, usize)>,
    pub fill: u8,
    pub grounds_cd: bool,
    pub reads: Vec<(u32, i64)>,
    pub writes: Vec<(u32, u8, i64)>,
    pub resets: Vec<ResetKind>,
}

impl FakeCard {
    pub fn new(ranges: Vec<(usize, usize)>) -> FakeCard {
        FakeCard {
            ranges,
            fill: 0xAA,
            grounds_cd: true,
            reads: Vec::new(),
            writes: Vec::new(),
            resets: Vec::new(),
        }
    }
}

impl DeviceState for FakeCard {}

impl PageHandler for FakeCard {
    fn read(&mut self, address: u32, mclk: i64, _pins: &mut BoardPins) -> u8 {
        self.reads.push((address, mclk));
        self.fill
    }

    fn write(&mut self, address: u32, value: u8, mclk: i64, _pins: &mut BoardPins) {
        self.writes.push((address, value, mclk));
    }
}

impl Card for FakeCard {
    fn grounds_cd(&self) -> bool {
        self.grounds_cd
    }

    fn claims(&self) -> Vec<(usize, usize)> {
        self.ranges.clone()
    }

    fn reset(&mut self, kind: ResetKind, _pins: &mut BoardPins) {
        self.resets.push(kind);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// An expansion-port device: grounds nothing, but its presence pulls the I/O port's D7 low
/// and it can hold IRQ2.
pub struct FakePort {
    ranges: Vec<(usize, usize)>,
    pub fill: u8,
    pub present: bool,
    pub reads: Vec<(u32, i64)>,
    pub writes: Vec<(u32, u8, i64)>,
    pub resets: Vec<ResetKind>,
    pub caught_up: Vec<i64>,
    /// The reference's `Alarm` subclass: IRQ2 is raised once `catch_up` reaches this time.
    pub alarm_due: Option<i64>,
}

impl FakePort {
    pub fn new(ranges: Vec<(usize, usize)>) -> FakePort {
        FakePort {
            ranges,
            fill: 0xCC,
            present: true,
            reads: Vec::new(),
            writes: Vec::new(),
            resets: Vec::new(),
            caught_up: Vec::new(),
            alarm_due: None,
        }
    }

    pub fn set_irq2(&mut self, level: bool, pins: &mut BoardPins) {
        pins.irq2.set("fake-port", level);
    }
}

impl DeviceState for FakePort {}

impl PageHandler for FakePort {
    fn read(&mut self, address: u32, mclk: i64, _pins: &mut BoardPins) -> u8 {
        self.reads.push((address, mclk));
        self.fill
    }

    fn write(&mut self, address: u32, value: u8, mclk: i64, _pins: &mut BoardPins) {
        self.writes.push((address, value, mclk));
    }
}

impl PortDevice for FakePort {
    fn present(&self) -> bool {
        self.present
    }

    fn claims(&self) -> Vec<(usize, usize)> {
        self.ranges.clone()
    }

    fn catch_up(&mut self, mclk: i64, pins: &mut BoardPins) {
        self.caught_up.push(mclk);
        if self.alarm_due.is_some_and(|due| mclk >= due) {
            self.set_irq2(true, pins);
        }
    }

    fn reset(&mut self, kind: ResetKind, _pins: &mut BoardPins) {
        self.resets.push(kind);
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

#[derive(Default)]
pub struct FakeAudioSource;

impl DeviceState for FakeAudioSource {}

impl AudioSource for FakeAudioSource {
    fn rate_hz(&self) -> f64 {
        44100.0
    }

    fn gain(&self) -> f64 {
        0.5
    }

    fn take_samples(&mut self, _mclk: i64) -> Vec<(i32, i32)> {
        vec![(1, -1)]
    }
}

/// Records SEL/CLR and answers a fixed nibble.
pub struct FakeController {
    pub nibble: u8,
    pub writes: Vec<(bool, bool)>,
}

impl FakeController {
    pub fn new(nibble: u8) -> FakeController {
        FakeController {
            nibble,
            writes: Vec::new(),
        }
    }
}

impl DeviceState for FakeController {}

impl ControllerPort for FakeController {
    fn write(&mut self, sel: bool, clr: bool) {
        self.writes.push((sel, clr));
    }

    fn read(&mut self, _sel: bool, _clr: bool) -> u8 {
        self.nibble
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Replaces 'VDC 1 straight into the VCE' by mixing two rows: the second wins where its
/// pixel is non-zero (a stand-in for the SuperGrafx's VPC).
#[derive(Default)]
pub struct FakePixelPath {
    pub lines_seen: usize,
    pub timings: Vec<VideoTiming>,
    pub row_counts: Vec<usize>,
}

fn second_wins(mut rows: Vec<Vec<u16>>) -> Vec<u16> {
    if rows.len() == 1 {
        return rows.swap_remove(0);
    }
    rows[0]
        .iter()
        .zip(rows[1].iter())
        .map(|(&a, &b)| if b != 0 { b } else { a })
        .collect()
}

impl DeviceState for FakePixelPath {}

impl PixelPath for FakePixelPath {
    fn mix(&mut self, timing: &VideoTiming, rows: Vec<Vec<u16>>) -> Vec<u16> {
        self.lines_seen += 1;
        self.timings.push(*timing);
        self.row_counts.push(rows.len());
        second_wins(rows)
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Drives a board's bus the way the core does: one callback per CPU cycle.
pub struct ScriptedCpu {
    pub board: Board,
    pub pins: Pins,
}

impl ScriptedCpu {
    #[allow(clippy::field_reassign_with_default)] // `Pins` has private fields
    pub fn new(mut board: Board, high_speed: bool) -> ScriptedCpu {
        let mut pins = Pins::default();
        pins.high_speed = high_speed;
        pins.set_irq_disable(board.cpu_io.irq.disable);
        board.deliver_lines(Some(&mut pins), true);
        ScriptedCpu { board, pins }
    }

    pub fn read(&mut self, address: u32) -> u8 {
        self.board.read(address, &mut self.pins)
    }

    pub fn write(&mut self, address: u32, value: u8) {
        self.board.write(address, value, &mut self.pins);
    }

    pub fn idle(&mut self, cycles: usize) {
        for _ in 0..cycles {
            self.board.idle(None, &mut self.pins);
        }
    }
}

// --- devices a game can run on, for the lockstep gate -----------------------------------------------

fn decode<T: for<'a> Deserialize<'a>>(data: &[u8]) -> Result<T, String> {
    bincode::deserialize(data).map_err(|error| format!("a test device's state: {error}"))
}

/// Socket 1 (and 3, 4): a ROM card that also claims banks $F8-$FB, so the work RAM is the
/// card's, and that holds the CPU for 3 master clocks on every 4096th write to it.
pub struct LockstepCard {
    rom: RomCard,
    pub ram: Vec<u8>,
    pub writes: u64,
}

impl LockstepCard {
    pub fn new(data: &[u8]) -> Result<LockstepCard, String> {
        Ok(LockstepCard {
            rom: RomCard::new(data)?,
            ram: vec![0; 0x2000],
            writes: 0,
        })
    }
}

impl DeviceState for LockstepCard {
    fn save_state(&self) -> Vec<u8> {
        bincode::serialize(&(&self.ram, self.writes)).unwrap()
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), String> {
        (self.ram, self.writes) = decode(data)?;
        Ok(())
    }

    fn state_identity(&self) -> String {
        format!("LockstepCard {}", self.rom.state_identity())
    }
}

impl PageHandler for LockstepCard {
    fn read(&mut self, address: u32, mclk: i64, pins: &mut BoardPins) -> u8 {
        if address >= 0x1F0000 {
            return self.ram[(address & 0x1FFF) as usize];
        }
        self.rom.read(address, mclk, pins)
    }

    fn write(&mut self, address: u32, value: u8, _mclk: i64, pins: &mut BoardPins) {
        if address >= 0x1F0000 {
            self.ram[(address & 0x1FFF) as usize] = value;
            self.writes += 1;
            if self.writes % 4096 == 0 {
                pins.hold(3);
            }
        }
    }
}

impl Card for LockstepCard {
    fn grounds_cd(&self) -> bool {
        true
    }

    fn claims(&self) -> Vec<(usize, usize)> {
        vec![(0x0000, 0x0FFF), (0x1F00, 0x1F7F)]
    }

    fn reset(&mut self, _kind: ResetKind, _pins: &mut BoardPins) {
        self.writes = 0;
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Socket 9: a source at 32 kHz with a gain the mixer applies, a fixed pattern.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct LockstepTone {
    pub produced: i64,
}

impl DeviceState for LockstepTone {}

impl AudioSource for LockstepTone {
    fn rate_hz(&self) -> f64 {
        32000.0
    }

    fn gain(&self) -> f64 {
        0.25
    }

    fn take_samples(&mut self, mclk: i64) -> Vec<(i32, i32)> {
        let total = py_floordiv(mclk * 32000, MASTER_HZ);
        let out = (self.produced..total)
            .map(|i| {
                (
                    ((i * 37) % 2001 - 1000) as i32,
                    ((i * 53) % 1501 - 750) as i32,
                )
            })
            .collect();
        self.produced = self.produced.max(total);
        out
    }
}

/// Sockets 2, 5, 6 and 9: a port device that claims `$FF:1800-18FF` (one register), is
/// present (the I/O port's D7 reads low), raises IRQ2 by itself for `WIDTH` master clocks every
/// `PERIOD`, and owns an audio source.
pub struct LockstepPort {
    pub register: u8,
    pub tone: LockstepTone,
}

impl LockstepPort {
    pub const PERIOD: i64 = 300_000;
    pub const WIDTH: i64 = 700;

    pub fn new() -> LockstepPort {
        LockstepPort {
            register: 0,
            tone: LockstepTone::default(),
        }
    }
}

impl Default for LockstepPort {
    fn default() -> Self {
        LockstepPort::new()
    }
}

impl DeviceState for LockstepPort {
    fn save_state(&self) -> Vec<u8> {
        bincode::serialize(&(self.register, &self.tone)).unwrap()
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), String> {
        (self.register, self.tone) = decode(data)?;
        Ok(())
    }

    fn state_identity(&self) -> String {
        "LockstepPort".to_string()
    }
}

impl PageHandler for LockstepPort {
    fn read(&mut self, _address: u32, _mclk: i64, _pins: &mut BoardPins) -> u8 {
        self.register
    }

    fn write(&mut self, _address: u32, value: u8, _mclk: i64, _pins: &mut BoardPins) {
        self.register = value;
    }
}

impl PortDevice for LockstepPort {
    fn present(&self) -> bool {
        true
    }

    fn claims(&self) -> Vec<(usize, usize)> {
        vec![(0x1FF8, 0x1FF8)]
    }

    fn catch_up(&mut self, mclk: i64, pins: &mut BoardPins) {
        let level = mclk >= Self::PERIOD && mclk % Self::PERIOD < Self::WIDTH;
        pins.irq2.set("lockstep-port", level);
    }

    fn reset(&mut self, _kind: ResetKind, _pins: &mut BoardPins) {
        self.register = 0;
    }

    fn audio_sources(&mut self) -> Vec<&mut dyn AudioSource> {
        vec![&mut self.tone]
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// [hw: HuC6280 hardware manual] ST0, ST1, ST2 are opcodes $03, $13, $23.
pub const ST_OPCODES: [u8; 3] = [0x03, 0x13, 0x23];

/// Sockets 3, 7, 8 and 12, on a machine row with two VDCs: the path claims the VDC pages; a
/// write made by `ST0`-`ST2` goes to both VDCs, any other write to the first only; a read is
/// the first VDC's, and a status read also reads (and so acknowledges) the second's. The mix
/// is `FakePixelPath`'s: the second VDC's pixel wins where it is not zero.
#[derive(Default)]
pub struct LockstepPath {
    pub lines_seen: u64,
}

impl DeviceState for LockstepPath {
    fn save_state(&self) -> Vec<u8> {
        bincode::serialize(&self.lines_seen).unwrap()
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), String> {
        self.lines_seen = decode(data)?;
        Ok(())
    }

    fn state_identity(&self) -> String {
        "LockstepPath".to_string()
    }
}

impl PixelPath for LockstepPath {
    fn mix(&mut self, _timing: &VideoTiming, rows: Vec<Vec<u16>>) -> Vec<u16> {
        self.lines_seen += 1;
        second_wins(rows)
    }

    fn claims(&self) -> Vec<(usize, usize)> {
        vec![(HARDWARE_FIRST_PAGE, HARDWARE_FIRST_PAGE + 3)]
    }

    fn read(&mut self, address: u32, mclk: i64, pins: &mut BoardPins) -> u8 {
        let port = address & 3;
        let value = pins.vdc_read(0, port, mclk);
        if port == 0 {
            pins.vdc_read(1, port, mclk);
        }
        value
    }

    fn write(&mut self, address: u32, value: u8, mclk: i64, pins: &mut BoardPins) {
        let port = address & 3;
        pins.vdc_write(0, port, value, mclk);
        if ST_OPCODES.contains(&pins.opcode) {
            pins.vdc_write(1, port, value, mclk);
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
