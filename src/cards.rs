//! HuCards: the standard ROM card, Street Fighter II', Populous and Tennokoe (`cards.py`).
//!
//! A card is on the CPU's physical bus: PA20 is its chip enable, so it answers banks $00-$7F (the
//! 1 MiB window = pages 0x0000-0x0FFF); there is no header (docs/cartridge.md). Mirroring of
//! smaller images:
//!
//! - 384 KiB: the first 256 KiB fills banks $00-$3F twice, the last 128 KiB fills $40-$7F four
//!   times [emu: MAME `pce_slot.cpp` rom_map_setup `0 1 0 1 2 2 2 2`; Mednafen `huc.cpp`].
//! - 512 KiB: $00-$3F straight, $40-$7F the upper 256 KiB twice [emu: Mednafen; MAME generic].
//! - other power-of-two sizes: mirrored to fill the window.
//! - anything else (768 KiB, odd homebrew sizes): U18, provisional: banks repeat modulo the image.
//!
//! All tier 3-4; confirmed only by games running.
//!
//! Differences forced by Rust: the mapper cards hold a `RomCard` where the reference subclasses
//! it; a card's identity in a save state is the SHA-1 and length of its image, computed once
//! when it is built (the reference hashes the bytes each time it saves); `from_path` is left to
//! the caller (`std::fs::read`).

use std::any::Any;

use serde::Deserialize;
use sha1::{Digest, Sha1};

use crate::sockets::{BoardPins, Card, DeviceState, PageHandler, ResetKind};

pub const BANK: usize = 8 * 1024;
pub const WINDOW_BANKS: usize = 128; // $00-$7F
pub const COPIER_HEADER: usize = 512;

/// A 512-byte copier header is the only thing to strip (size mod 8192 = 512).
pub fn strip_copier_header(data: &[u8]) -> &[u8] {
    if data.len() % BANK == COPIER_HEADER {
        &data[COPIER_HEADER..]
    } else {
        data
    }
}

/// For each of the 128 banks of the window, which 8 KiB bank of the image answers.
pub fn bank_map(rom_banks: usize) -> Vec<usize> {
    if rom_banks == 48 {
        // 384 KiB
        return (0..WINDOW_BANKS)
            .map(|b| if b < 64 { b % 32 } else { 32 + (b % 16) })
            .collect();
    }
    if rom_banks == 64 {
        // 512 KiB
        return (0..WINDOW_BANKS)
            .map(|b| if b < 64 { b } else { 32 + (b % 32) })
            .collect();
    }
    (0..WINDOW_BANKS).map(|b| b % rom_banks).collect()
}

fn rom_identity(kind: &str, data: &[u8]) -> String {
    let digest = Sha1::digest(data);
    let hex: String = digest.iter().map(|byte| format!("{byte:02x}")).collect();
    format!("{kind} rom {hex} length {}", data.len())
}

fn decode<T: for<'a> Deserialize<'a>>(data: &[u8]) -> Result<T, String> {
    bincode::deserialize(data).map_err(|error| format!("a card's state: {error}"))
}

/// The standard HuCard: ROM only, mirrored to fill the window.
pub struct RomCard {
    pub data: Vec<u8>,
    map: Vec<usize>,
    identity: String,
}

impl RomCard {
    pub fn new(data: &[u8]) -> Result<RomCard, String> {
        let mut data = strip_copier_header(data).to_vec();
        if data.is_empty() {
            return Err("an empty HuCard image; an empty slot is no card".to_string());
        }
        if data.len() % BANK != 0 {
            // U18: pad a ragged image with zeros
            data.resize(data.len() + BANK - data.len() % BANK, 0);
        }
        let map = bank_map(data.len() / BANK);
        let identity = rom_identity("RomCard", &data);
        Ok(RomCard {
            data,
            map,
            identity,
        })
    }

    /// The fork: for each of the 128 banks of the window, where in `data` it starts (the
    /// board reads a lent image by it: board.rs, `lend_rom`).
    pub(crate) fn bank_offsets(&self) -> [u32; WINDOW_BANKS] {
        let mut offsets = [0; WINDOW_BANKS];
        for (offset, bank) in offsets.iter_mut().zip(&self.map) {
            *offset = (bank * BANK) as u32;
        }
        offsets
    }

    #[inline]
    fn rom_read(&self, address: u32) -> u8 {
        let bank = ((address >> 13) & 0x7F) as usize;
        self.data[self.map[bank] * BANK + (address & 0x1FFF) as usize]
    }
}

impl DeviceState for RomCard {
    fn state_identity(&self) -> String {
        self.identity.clone()
    }
}

impl PageHandler for RomCard {
    fn read(&mut self, address: u32, _mclk: i64, _pins: &mut BoardPins) -> u8 {
        self.rom_read(address)
    }

    fn write(&mut self, _address: u32, _value: u8, _mclk: i64, _pins: &mut BoardPins) {
        // a plain ROM ignores writes
    }
}

impl Card for RomCard {
    fn grounds_cd(&self) -> bool {
        true // [doc: wiki HuCard pinout] a card grounds /CD
    }

    fn claims(&self) -> Vec<(usize, usize)> {
        vec![(0x0000, 0x0FFF)]
    }

    fn reset(&mut self, _kind: ResetKind, _pins: &mut BoardPins) {}

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

// --- the mappers beyond plain ROM (M5) ----------------------------------------------------------

/// [emu: Mednafen huc.cpp sf2_threshold] an image this big has the mapper
pub const SF2_THRESHOLD: usize = 2048 * 1024;
pub const SF2_FIXED: usize = 512 * 1024;

/// Street Fighter II' Champion Edition (2.5 MiB) and the larger homebrew form.
///
/// The first 512 KiB answer banks $00-$3F; banks $40-$7F show one of the further 512 KiB pages,
/// chosen by a write to bank $00 at $1FF0-$1FFF (the page is the low bits of the address). MAME's
/// `pce_sf2` decodes $1FF0-$1FF3 and four pages; Mednafen decodes $1FF0-$1FFF and masks by the
/// page count: the same for the real card. [emu: both; no manufacturer document]
/// (docs/cartridge.md)
pub struct Sf2Card {
    pub data: Vec<u8>,
    pub page_mask: usize,
    pub page: usize,
    identity: String,
}

impl Sf2Card {
    pub fn new(data: &[u8]) -> Result<Sf2Card, String> {
        let mut data = strip_copier_header(data).to_vec();
        if data.len() < SF2_THRESHOLD {
            return Err("an SF2 image is at least 2 MiB".to_string());
        }
        // whole 512 KiB pages after the fixed part
        let pages = (data.len() - SF2_FIXED).div_ceil(SF2_FIXED);
        // a power of two, as Mednafen rounds the image up
        let pages = pages.next_power_of_two();
        data.resize(SF2_FIXED * (1 + pages), 0);
        let identity = rom_identity("Sf2Card", &data);
        Ok(Sf2Card {
            data,
            page_mask: pages - 1,
            page: 0,
            identity,
        })
    }
}

impl DeviceState for Sf2Card {
    fn save_state(&self) -> Vec<u8> {
        bincode::serialize(&self.page).unwrap()
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), String> {
        self.page = decode(data)?;
        Ok(())
    }

    fn state_identity(&self) -> String {
        self.identity.clone()
    }
}

impl PageHandler for Sf2Card {
    fn read(&mut self, address: u32, _mclk: i64, _pins: &mut BoardPins) -> u8 {
        let offset = (address & 0xFFFFF) as usize;
        if offset < SF2_FIXED {
            return self.data[offset];
        }
        self.data[offset + self.page * SF2_FIXED] // banks $40-$7F: the chosen page
    }

    fn write(&mut self, address: u32, _value: u8, _mclk: i64, _pins: &mut BoardPins) {
        if address & 0x1FFFF0 == 0x1FF0 {
            // bank $00, $1FF0-$1FFF
            self.page = (address & 0xF) as usize & self.page_mask;
        }
    }
}

impl Card for Sf2Card {
    fn grounds_cd(&self) -> bool {
        true
    }

    fn claims(&self) -> Vec<(usize, usize)> {
        vec![(0x0000, 0x0FFF)]
    }

    fn reset(&mut self, _kind: ResetKind, _pins: &mut BoardPins) {
        self.page = 0;
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Populous: 32 KiB of battery RAM in banks $40-$43 (four 8 KiB banks, not mirrored);
/// the rest as a plain card. [emu: Mednafen: detected by "POPULOUS" at $1F26; MAME `pce_populous`
/// at $80000-$87FFF.] Power-up contents are Mednafen's $FF (U7: arbitrary for a battery RAM).
pub struct PopulousCard {
    pub rom: RomCard,
    pub ram: Vec<u8>,
    identity: String,
}

impl PopulousCard {
    pub const RAM_BANKS: [u32; 4] = [0x40, 0x41, 0x42, 0x43];

    pub fn new(data: &[u8]) -> Result<PopulousCard, String> {
        let rom = RomCard::new(data)?;
        let identity = rom_identity("PopulousCard", &rom.data);
        Ok(PopulousCard {
            rom,
            ram: vec![0xFF; 0x8000],
            identity,
        })
    }
}

impl DeviceState for PopulousCard {
    fn save_state(&self) -> Vec<u8> {
        self.ram.clone()
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), String> {
        if data.len() != self.ram.len() {
            return Err("a Populous card's state of another size".to_string());
        }
        self.ram.copy_from_slice(data);
        Ok(())
    }

    fn state_identity(&self) -> String {
        self.identity.clone()
    }
}

impl PageHandler for PopulousCard {
    fn read(&mut self, address: u32, _mclk: i64, _pins: &mut BoardPins) -> u8 {
        if (0x40..=0x43).contains(&(address >> 13)) {
            return self.ram[(address & 0x7FFF) as usize];
        }
        self.rom.rom_read(address)
    }

    fn write(&mut self, address: u32, value: u8, _mclk: i64, _pins: &mut BoardPins) {
        if (0x40..=0x43).contains(&(address >> 13)) {
            self.ram[(address & 0x7FFF) as usize] = value;
        }
    }
}

impl Card for PopulousCard {
    fn grounds_cd(&self) -> bool {
        true
    }

    fn claims(&self) -> Vec<(usize, usize)> {
        vec![(0x0000, 0x0FFF)]
    }

    fn reset(&mut self, _kind: ResetKind, _pins: &mut BoardPins) {}

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// Tennokoe Bank: a 512 KiB HuCard with 8 KiB of backup RAM (four 2 KiB banks) at $80000-$81FFF
/// of the card's space (banks $40, mirrored through the 64 KiB up to $8FFFF), locked at power-up
/// (reads $FF, writes ignored); a write of a non-zero value anywhere in $F0000-$FFFFF unlocks it, a
/// zero locks it. [emu: MAME `pce_tennokoe`, whose own comment calls the lock "a complete guess"
/// (U29, tier 5); none in the library, so only unit-tested.]
pub struct TennokoeCard {
    pub rom: RomCard,
    pub ram: Vec<u8>,
    pub locked: bool,
    identity: String,
}

impl TennokoeCard {
    pub fn new(data: &[u8]) -> Result<TennokoeCard, String> {
        let rom = RomCard::new(data)?;
        let identity = rom_identity("TennokoeCard", &rom.data);
        Ok(TennokoeCard {
            rom,
            ram: vec![0xFF; 0x2000],
            locked: true,
            identity,
        })
    }
}

impl DeviceState for TennokoeCard {
    fn save_state(&self) -> Vec<u8> {
        bincode::serialize(&(&self.ram, self.locked)).unwrap()
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), String> {
        let (ram, locked): (Vec<u8>, bool) = decode(data)?;
        if ram.len() != self.ram.len() {
            return Err("a Tennokoe card's state of another size".to_string());
        }
        self.ram = ram;
        self.locked = locked;
        Ok(())
    }

    fn state_identity(&self) -> String {
        self.identity.clone()
    }
}

impl PageHandler for TennokoeCard {
    fn read(&mut self, address: u32, _mclk: i64, _pins: &mut BoardPins) -> u8 {
        let bank = (address >> 13) & 0x7F;
        if bank < 0x40 {
            return self.rom.rom_read(address);
        }
        if bank < 0x48 {
            return if self.locked {
                0xFF
            } else {
                self.ram[(address & 0x1FFF) as usize]
            };
        }
        0xFF // nothing else is decoded above the ROM
    }

    fn write(&mut self, address: u32, value: u8, _mclk: i64, _pins: &mut BoardPins) {
        let offset = address & 0xFFFFF;
        if (0x80000..0x90000).contains(&offset) {
            if !self.locked {
                self.ram[(address & 0x1FFF) as usize] = value;
            }
        } else if offset >= 0xF0000 {
            self.locked = value == 0;
        }
    }
}

impl Card for TennokoeCard {
    fn grounds_cd(&self) -> bool {
        true
    }

    fn claims(&self) -> Vec<(usize, usize)> {
        vec![(0x0000, 0x0FFF)]
    }

    fn reset(&mut self, _kind: ResetKind, _pins: &mut BoardPins) {
        self.locked = true;
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// The card an image is: SF2 from 2 MiB up (Mednafen's threshold), Populous by its title string
/// at $1F26, else a plain ROM. Tennokoe has no detection (no header): build it explicitly.
pub fn make_card(data: &[u8]) -> Result<Box<dyn Card>, String> {
    let body = strip_copier_header(data);
    if body.len() >= SF2_THRESHOLD {
        return Ok(Box::new(Sf2Card::new(data)?));
    }
    if body.len() >= 0x1F26 + 8 && &body[0x1F26..0x1F26 + 8] == b"POPULOUS" {
        return Ok(Box::new(PopulousCard::new(data)?));
    }
    Ok(Box::new(RomCard::new(data)?))
}
