// SPDX-License-Identifier: MIT
//! A sampling profiler for the board, since `perf` is not allowed here: SIGPROF 1,000 times a
//! second of CPU time, the interrupted instruction's address recorded, then
//! `scripts/profile.py` maps the addresses to source lines (inlined frames included) with
//! `addr2line`.
//!
//!     cargo run --release --features profile --bin pce-profile -- IMAGE [--frames N]
//!         [options of trace::Setup] [--out FILE]

use std::sync::atomic::{AtomicUsize, Ordering};

use pcengine_rust::trace::Setup;

const CAPACITY: usize = 1 << 20;
static mut SAMPLES: [usize; CAPACITY] = [0; CAPACITY];
static COUNT: AtomicUsize = AtomicUsize::new(0);

extern "C" fn on_prof(_: libc::c_int, _: *mut libc::siginfo_t, context: *mut libc::c_void) {
    // SAFETY: the kernel passes a ucontext_t; only the RIP is read, and each slot is written
    // once, by this handler, on this thread.
    unsafe {
        let ucontext = &*(context as *const libc::ucontext_t);
        let rip = ucontext.uc_mcontext.gregs[libc::REG_RIP as usize] as usize;
        let n = COUNT.fetch_add(1, Ordering::Relaxed);
        if n < CAPACITY {
            let samples = &raw mut SAMPLES;
            (*samples)[n] = rip;
        }
    }
}

fn set_timer(usec: libc::suseconds_t) {
    let tick = libc::timeval {
        tv_sec: 0,
        tv_usec: usec,
    };
    let timer = libc::itimerval {
        it_interval: tick,
        it_value: tick,
    };
    // SAFETY: an interval timer for this process.
    unsafe {
        libc::setitimer(libc::ITIMER_PROF, &timer, std::ptr::null_mut());
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let setup = Setup::parse(&args).unwrap_or_else(|error| panic!("{error}"));
    let out = setup
        .out
        .clone()
        .unwrap_or_else(|| "external/profile.txt".to_owned());
    let mut system = setup.build().expect("a board");
    // SAFETY: installing a handler for this process.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = on_prof as *const () as usize;
        action.sa_flags = libc::SA_SIGINFO | libc::SA_RESTART;
        libc::sigaction(libc::SIGPROF, &action, std::ptr::null_mut());
    }
    set_timer(1000);
    for _ in 0..setup.frames {
        setup.apply_presses(&mut system);
        system.run_frame();
        system.board_mut().take_audio();
    }
    set_timer(0);
    let maps = std::fs::read_to_string("/proc/self/maps").unwrap();
    let exe = std::fs::read_link("/proc/self/exe").unwrap();
    let base = maps
        .lines()
        .find(|line| line.ends_with(exe.to_str().unwrap()))
        .and_then(|line| usize::from_str_radix(line.split('-').next().unwrap(), 16).ok())
        .expect("the executable's mapping");
    let n = COUNT.load(Ordering::Relaxed).min(CAPACITY);
    let mut text = format!("{}\n", exe.display());
    let samples = &raw const SAMPLES;
    for i in 0..n {
        // SAFETY: the timer is stopped, nothing writes SAMPLES any more.
        let rip = unsafe { (*samples)[i] };
        text.push_str(&format!("{:x}\n", rip.wrapping_sub(base)));
    }
    std::fs::write(&out, text).unwrap();
    eprintln!("{n} samples to {out}");
}
