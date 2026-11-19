// SPDX-License-Identifier: MIT
//! Rung 6: save states proven by equivalence, and the snapshot pair in a run-ahead pass.
//!
//!     pce-state-roundtrip IMAGE [options of trace::Setup] [--at 300] [--to 360] [--extra 777]
//!                         [--ahead 2] [--ahead-frames 120]
//!
//! `trace::roundtrip` (the reference's scripts/state_roundtrip.py) and `trace::runahead`.

use pcengine_rust::trace::{roundtrip, runahead, Setup};

fn run() -> Result<String, String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let setup = Setup::parse(&args)?;
    let (mut at, mut to, mut extra, mut ahead, mut ahead_frames) = (300, 360, 777, 2, 120);
    let mut rest = setup.rest.iter();
    while let Some(arg) = rest.next() {
        let value: i64 = rest
            .next()
            .and_then(|text| text.parse().ok())
            .ok_or_else(|| format!("{arg} needs a number"))?;
        match arg.as_str() {
            "--at" => at = value,
            "--to" => to = value,
            "--extra" => extra = value,
            "--ahead" => ahead = value,
            "--ahead-frames" => ahead_frames = value,
            _ => return Err(format!("unknown argument {arg}")),
        }
    }
    let saved = roundtrip(&setup, at, to, extra)?;
    let looked = runahead(&setup, ahead_frames, ahead)?;
    Ok(format!("{saved}; run-ahead: {looked}"))
}

fn main() {
    match run() {
        Ok(what) => println!("ok: {what}"),
        Err(error) => {
            println!("FAIL: {error}");
            std::process::exit(1);
        }
    }
}
