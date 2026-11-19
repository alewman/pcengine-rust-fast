// SPDX-License-Identifier: MIT
//! The board behind four plain exports, for timing under wasm (no wasm-bindgen: Node's own
//! `WebAssembly` runs it) and for the same run natively (`src/main.rs`).
//!
//! `alloc` a buffer, write the card image into it, `start` a machine on it, `run` frames (the
//! picture gate's input: RUN at frame 250, I at 400, right with II from 480), then
//! `output_check` (every picture, every sample, the master clock) and `check` (pce-bench's:
//! the same and the snapshot at the end), so the wasm build is held to the native one.
//! `level_bits(u)` is the PSG's level table entry as the platform's `powf` computes it.

use pcengine_rust::system::System;
use pcengine_rust::trace::{parse_press, Setup};

pub struct Run {
    setup: Setup,
    system: System,
    check: u64,
}

fn fnv(hash: &mut u64, value: u64) {
    *hash = (*hash ^ value).wrapping_mul(0x0000_0100_0000_01B3);
}

impl Run {
    pub fn new(image: &[u8], tg16: bool) -> Run {
        let mut setup = Setup::for_image(image.to_vec());
        if tg16 {
            setup.machine = "tg16".to_string();
        }
        for press in ["run@250+10", "i@400+6", "right-ii@480+40"] {
            setup.presses.push(parse_press(press).unwrap());
        }
        let system = setup.build().expect("a board");
        Run {
            setup,
            system,
            check: 0xCBF2_9CE4_8422_2325,
        }
    }

    pub fn run(&mut self, frames: u32) {
        for _ in 0..frames {
            self.setup.apply_presses(&mut self.system);
            self.system.run_frame();
            for (left, right) in self.system.board_mut().take_audio() {
                fnv(
                    &mut self.check,
                    (left as u16 as u64) << 16 | right as u16 as u64,
                );
            }
            if let Some(frame) = self.system.board().capture.last_frame.as_ref() {
                for (row, divider) in frame.rows.iter().zip(&frame.dividers) {
                    fnv(&mut self.check, *divider as u64);
                    for value in row {
                        fnv(&mut self.check, *value as u64);
                    }
                }
            }
        }
    }

    /// The checksum of what the run has shown and played so far: every picture, every sample,
    /// the master clock.
    pub fn output_check(&self) -> u32 {
        let mut check = self.check;
        fnv(&mut check, self.system.board().mclk as u64);
        (check ^ (check >> 32)) as u32
    }

    /// pce-bench's checksum of the run so far: the output, then the snapshot's bytes (which
    /// hold the PSG's level table, so this one differs wherever `powf` does).
    pub fn check(&self) -> u32 {
        let mut check = self.check;
        fnv(&mut check, self.system.board().mclk as u64);
        for byte in pcengine_rust::state::save_snapshot(&self.system) {
            fnv(&mut check, byte as u64);
        }
        (check ^ (check >> 32)) as u32
    }
}

/// The PSG's level for `u` attenuation units (psg/chip.rs: `10f64.powf(-1.5 * u / 20.0)`).
pub fn level(u: u32) -> f64 {
    10f64.powf(-1.5 * u as f64 / 20.0)
}

static mut RUN: Option<Run> = None;

#[no_mangle]
pub extern "C" fn alloc(len: usize) -> *mut u8 {
    Box::leak(vec![0u8; len].into_boxed_slice()).as_mut_ptr()
}

/// # Safety
/// `image` and `len` are a buffer `alloc` returned, filled by the caller.
#[no_mangle]
pub unsafe extern "C" fn start(image: *const u8, len: usize, tg16: u32) {
    let image = std::slice::from_raw_parts(image, len);
    let run = &raw mut RUN;
    *run = Some(Run::new(image, tg16 != 0));
}

#[no_mangle]
pub extern "C" fn run(frames: u32) {
    // SAFETY: one thread; `start` came first.
    unsafe {
        let run = &raw mut RUN;
        (*run).as_mut().expect("start first").run(frames);
    }
}

#[no_mangle]
pub extern "C" fn check() -> u32 {
    // SAFETY: one thread; `start` came first.
    unsafe {
        let run = &raw const RUN;
        (*run).as_ref().expect("start first").check()
    }
}

#[no_mangle]
pub extern "C" fn output_check() -> u32 {
    // SAFETY: one thread; `start` came first.
    unsafe {
        let run = &raw const RUN;
        (*run).as_ref().expect("start first").output_check()
    }
}

#[no_mangle]
pub extern "C" fn level_bits(u: u32) -> u64 {
    level(u).to_bits()
}
