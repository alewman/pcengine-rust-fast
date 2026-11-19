// SPDX-License-Identifier: MIT
//! The benchmark: frames per second of this board on a card, with a checksum of every picture,
//! every audio sample and the snapshot at the end, so a faster build that changes the output
//! or the state is caught here too.
//!
//!     pce-bench IMAGE [--frames N] [options of trace::Setup]
//!
//! The time is this thread's time on the CPU (`/proc/thread-self/schedstat`, nanoseconds run),
//! not the wall clock: on a shared machine the wall clock counts the time other processes had
//! the CPU. Only `run_frame` and `take_audio` are timed; the checksum is taken outside.
//! `scripts/bench.py` times the reference; `scripts/ab.py` compares two builds of this.

use pcengine_rust::trace::Setup;

/// Nanoseconds this thread has run on a CPU (the wall clock where there is no /proc).
fn cpu_ns(began: &std::time::Instant) -> u64 {
    match std::fs::read_to_string("/proc/thread-self/schedstat") {
        Ok(text) => text.split_whitespace().next().unwrap().parse().unwrap(),
        Err(_) => began.elapsed().as_nanos() as u64,
    }
}

fn fnv(hash: &mut u64, value: u64) {
    *hash = (*hash ^ value).wrapping_mul(0x0000_0100_0000_01B3);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let setup = match Setup::parse(&args) {
        Ok(setup) => setup,
        Err(error) => {
            eprintln!("pce-bench: {error}");
            std::process::exit(1);
        }
    };
    let name = args
        .iter()
        .find(|arg| !arg.starts_with("--"))
        .map(|path| {
            std::path::Path::new(path)
                .file_stem()
                .unwrap()
                .to_string_lossy()
                .into_owned()
        })
        .unwrap_or_default();
    let mut system = setup.build().expect("a board");
    let clock = std::time::Instant::now();
    let mut check = 0xCBF2_9CE4_8422_2325u64;
    let mut spent = 0u64;
    for _ in 0..setup.frames {
        setup.apply_presses(&mut system);
        let began = cpu_ns(&clock);
        system.run_frame();
        let samples = system.board_mut().take_audio();
        spent += cpu_ns(&clock) - began;
        for (left, right) in samples {
            fnv(&mut check, (left as u16 as u64) << 16 | right as u16 as u64);
        }
        if let Some(frame) = system.board().capture.last_frame.as_ref() {
            for (row, divider) in frame.rows.iter().zip(&frame.dividers) {
                fnv(&mut check, *divider as u64);
                for value in row {
                    fnv(&mut check, *value as u64);
                }
            }
        }
    }
    fnv(&mut check, system.board().mclk as u64);
    // the whole machine as a snapshot's bytes: a shortcut may not leave the state another
    // one than the base board's, even where no picture or sample shows it yet
    for byte in pcengine_rust::state::save_snapshot(&system) {
        fnv(&mut check, byte as u64);
    }
    let seconds = spent as f64 / 1e9;
    println!(
        "{name}: {} frames in {seconds:.3} s, {:.1} frames/s, check {:08X}",
        setup.frames,
        setup.frames as f64 / seconds,
        (check ^ (check >> 32)) as u32
    );
}
