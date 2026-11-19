// SPDX-License-Identifier: MIT
//! The pads: 2-button, 6-button and the 5-player multitap (the mouse is not modelled: U12)
//! (`pads.py`).
//!
//! All implement `sockets::ControllerPort`: the console drives SEL and CLR, the device answers a
//! nibble on D3-D0 (active low: 0 = pressed). Held-button state is a bit mask the host sets.
//!
//! Sources: [hw: pcetech 4] for the 2-button pad and the Turbo Tap; [doc: pce-devel
//! PCE_Controller_Info] for the 6-button scan counting and the multitap port table.
//!
//! Differences forced by Rust: `Pad6` holds a `Pad2`'s mask where the reference subclasses it;
//! the host sets `pressed` through `ControllerPort::set_pressed` (the board owns the device).

use std::any::Any;

use serde::{Deserialize, Serialize};

use crate::sockets::{ControllerPort, DeviceState};

// Button bits of the host-side state mask.
pub const UP: u16 = 0x001;
pub const RIGHT: u16 = 0x002;
pub const DOWN: u16 = 0x004;
pub const LEFT: u16 = 0x008;
pub const I: u16 = 0x010;
pub const II: u16 = 0x020;
pub const SELECT: u16 = 0x040;
pub const RUN: u16 = 0x080;
pub const III: u16 = 0x100;
pub const IV: u16 = 0x200;
pub const V: u16 = 0x400;
pub const VI: u16 = 0x800;

fn decode<T: for<'a> Deserialize<'a>>(data: &[u8]) -> Result<T, String> {
    bincode::deserialize(data).map_err(|error| format!("a pad's state: {error}"))
}

/// A 2-button pad: SEL high shows the directions, SEL low the buttons.
///
/// [hw: pcetech 4] While CLR is high the pad's multiplexer outputs 0 on D3-D0. [doc:
/// controller-info] the 74HC157 gates all four outputs low on CLR.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pad2 {
    pub pressed: u16,
}

impl Pad2 {
    pub fn new() -> Pad2 {
        Pad2 { pressed: 0 }
    }

    fn nibble(&self, sel: bool, clr: bool) -> u8 {
        if clr {
            return 0;
        }
        if sel {
            return (!self.pressed & 0x0F) as u8;
        }
        (!(self.pressed >> 4) & 0x0F) as u8
    }
}

impl DeviceState for Pad2 {
    fn save_state(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), String> {
        *self = decode(data)?;
        Ok(())
    }

    fn state_identity(&self) -> String {
        "Pad2".to_string()
    }
}

impl ControllerPort for Pad2 {
    fn write(&mut self, _sel: bool, _clr: bool) {}

    fn read(&mut self, sel: bool, clr: bool) -> u8 {
        self.nibble(sel, clr)
    }

    fn set_pressed(&mut self, _port: usize, pressed: u16) {
        self.pressed = pressed;
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// The Avenue Pad 6 shape: on alternating scans (counted by CLR pulses) the SEL-high read
/// shows 0000 and the SEL-low read shows III-VI instead of I, II, Select, Run. [doc:
/// controller-info] Which scan comes first at power-up is not stated (a game scans twice).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Pad6 {
    pub pressed: u16,
    pub second_scan: bool,
    clr: bool,
}

impl Pad6 {
    pub fn new() -> Pad6 {
        Pad6::default()
    }
}

impl DeviceState for Pad6 {
    fn save_state(&self) -> Vec<u8> {
        bincode::serialize(self).unwrap()
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), String> {
        *self = decode(data)?;
        Ok(())
    }

    fn state_identity(&self) -> String {
        "Pad6".to_string()
    }
}

impl ControllerPort for Pad6 {
    fn write(&mut self, _sel: bool, clr: bool) {
        if clr && !self.clr {
            self.second_scan = !self.second_scan;
        }
        self.clr = clr;
    }

    fn read(&mut self, sel: bool, clr: bool) -> u8 {
        if clr {
            return 0;
        }
        if !self.second_scan {
            return Pad2 {
                pressed: self.pressed,
            }
            .nibble(sel, clr);
        }
        if sel {
            return 0;
        }
        (!(self.pressed >> 8) & 0x0F) as u8
    }

    fn set_pressed(&mut self, _port: usize, pressed: u16) {
        self.pressed = pressed;
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}

/// The Turbo Tap: a counter picks which of five ports answers.
///
/// [hw: pcetech 4] The counter increments on each SEL 0 -> 1 transition while CLR is low and
/// resets on a CLR 0 -> 1 transition with SEL high; after the fifth it answers $0 until reset;
/// an empty port answers $F.
///
/// C10: pcetech says that right after the reset write ($03: SEL and CLR high) D3-D0 already
/// show pad 1's directions; the pce-devel table puts CLR high on every port at that moment,
/// which would read 0. `clr_high_reads_pad` selects pcetech's reading (tier 2, the default).
pub struct MultiTap {
    pub pads: Vec<Option<Box<dyn ControllerPort>>>,
    pub clr_high_reads_pad: bool,
    /// no port selected until the first reset sequence
    pub counter: usize,
    sel: bool,
    clr: bool,
}

impl MultiTap {
    pub const PORTS: usize = 5;

    pub fn new(pads: Vec<Option<Box<dyn ControllerPort>>>) -> Result<MultiTap, String> {
        if pads.len() != Self::PORTS {
            return Err("a multitap has five ports".to_string());
        }
        Ok(MultiTap {
            pads,
            clr_high_reads_pad: true,
            counter: Self::PORTS,
            sel: false,
            clr: false,
        })
    }
}

impl DeviceState for MultiTap {
    fn save_state(&self) -> Vec<u8> {
        let pads: Vec<Option<Vec<u8>>> = self
            .pads
            .iter()
            .map(|pad| pad.as_ref().map(|pad| pad.save_state()))
            .collect();
        bincode::serialize(&(self.counter, self.sel, self.clr, pads)).unwrap()
    }

    fn load_state(&mut self, data: &[u8]) -> Result<(), String> {
        let (counter, sel, clr, pads): (usize, bool, bool, Vec<Option<Vec<u8>>>) = decode(data)?;
        if pads.len() != self.pads.len() {
            return Err("a multitap with other ports".to_string());
        }
        for (pad, saved) in self.pads.iter_mut().zip(&pads) {
            match (pad, saved) {
                (Some(pad), Some(saved)) => pad.load_state(saved)?,
                (None, None) => {}
                _ => return Err("a multitap with other pads plugged in".to_string()),
            }
        }
        (self.counter, self.sel, self.clr) = (counter, sel, clr);
        Ok(())
    }

    fn state_identity(&self) -> String {
        let pads: Vec<String> = self
            .pads
            .iter()
            .map(|pad| pad.as_ref().map_or("-".to_string(), |p| p.state_identity()))
            .collect();
        format!("MultiTap({})", pads.join(","))
    }
}

impl ControllerPort for MultiTap {
    fn write(&mut self, sel: bool, clr: bool) {
        if clr && !self.clr && sel {
            self.counter = 0;
        } else if sel && !self.sel && !clr && self.counter < Self::PORTS {
            self.counter += 1;
        }
        self.sel = sel;
        self.clr = clr;
    }

    fn read(&mut self, sel: bool, clr: bool) -> u8 {
        if self.counter >= Self::PORTS {
            return 0;
        }
        if clr && !self.clr_high_reads_pad {
            return 0;
        }
        match self.pads[self.counter].as_mut() {
            None => 0x0F,
            Some(pad) => pad.read(sel, false),
        }
    }

    fn set_pressed(&mut self, port: usize, pressed: u16) {
        if let Some(Some(pad)) = self.pads.get_mut(port) {
            pad.set_pressed(0, pressed);
        }
    }

    fn as_any_mut(&mut self) -> &mut dyn Any {
        self
    }
}
