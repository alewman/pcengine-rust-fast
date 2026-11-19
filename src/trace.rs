// SPDX-License-Identifier: MIT
//! The lockstep gate's side of this board: per-frame digests of everything the gate compares,
//! in the spelling `scripts/pce_trace.py` writes for the reference.
//!
//! Not a module of the reference: its counterpart there is `scripts/state_roundtrip.py`'s
//! `observe` and `digest` and `scripts/presses.py`. A frame's line holds the frame counter, the
//! master clock, the count and SHA-1 of the CPU bus log (`Board::bus_log`: physical address,
//! value, kind, master clock of every cycle), the SHA-1 of the picture (`capture.last_frame`:
//! the rows as little-endian u16, then the dividers), the count and SHA-1 of the mixed audio
//! (`take_audio`, little-endian i16 pairs) and the CPU's whole `CpuState`.

use huc6280::CpuState;
use sha1::{Digest, Sha1};

use crate::pads;
use crate::system::System;

fn hex(digest: &[u8]) -> String {
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn option(value: Option<bool>) -> &'static str {
    match value {
        None => "-",
        Some(false) => "0",
        Some(true) => "1",
    }
}

/// Every field of the CPU's state, in `CpuState`'s order.
pub fn cpu_text(s: &CpuState) -> String {
    let mpr: String = s.mpr.iter().map(|byte| format!("{byte:02x}")).collect();
    format!(
        "{:02x},{:02x},{:02x},{:02x},{:04x},{:02x},{},{},{:02x},{}{}{},{},{}{}{}{},{},{},{},{},{}",
        s.a,
        s.x,
        s.y,
        s.s,
        s.pc,
        s.p,
        mpr,
        s.high_speed as u8,
        s.mpr_latch,
        s.irq1 as u8,
        s.irq2 as u8,
        s.tiq as u8,
        s.irq_disable,
        s.halted as u8,
        s.reset_pending as u8,
        s.maskable_interrupt_pending as u8,
        s.non_maskable_interrupt_pending as u8,
        option(s.polled_i),
        option(s.polled_maskable_interrupt),
        option(s.polled_non_maskable_interrupt),
        s.skipped_poll as u8,
        s.polled_irq_lines
            .map_or_else(|| "-".to_string(), |lines| lines.to_string()),
    )
}

/// The line of the frame that just ran. Empties the bus log and takes the audio.
pub fn frame_line(system: &mut System) -> String {
    let cpu = cpu_text(&system.cpu.capture_state());
    let frame = system.frame();
    let board = system.board_mut();
    let mut bus = Sha1::new();
    let mut cycles = 0;
    if let Some(log) = board.bus_log.as_mut() {
        cycles = log.len();
        for word in log.iter() {
            bus.update(word.to_le_bytes());
        }
        log.clear();
    }
    let picture = match board.capture.last_frame.as_ref() {
        None => "-".to_string(),
        Some(picture) => {
            let mut out = Sha1::new();
            for row in &picture.rows {
                for value in row {
                    out.update(value.to_le_bytes());
                }
            }
            out.update(&picture.dividers);
            format!("{}:{}", picture.number, hex(&out.finalize()))
        }
    };
    let samples = board.take_audio();
    let mut audio = Sha1::new();
    for (left, right) in &samples {
        audio.update(left.to_le_bytes());
        audio.update(right.to_le_bytes());
    }
    format!(
        "{} mclk={} bus={}:{} pic={} audio={}:{} cpu={}",
        frame,
        board.mclk,
        cycles,
        hex(&bus.finalize()),
        picture,
        samples.len(),
        hex(&audio.finalize()),
        cpu
    )
}

/// The per-cycle lines of `Board::bus_detail` (the gate's narrowing mode), emptied.
pub fn detail_lines(system: &mut System) -> Vec<String> {
    let Some(detail) = system.board_mut().bus_detail.as_mut() else {
        return Vec::new();
    };
    detail
        .drain(..)
        .map(|(kind, address, value, mclk)| match kind {
            0 => format!("r {address:06x} {value:02x} {mclk}"),
            1 => format!("w {address:06x} {value:02x} {mclk}"),
            _ => format!("i {mclk}"),
        })
        .collect()
}

/// A press `BUTTONS@FRAME+HOLD` (the reference's `scripts/presses.py`): the buttons, joined
/// with `-`, are held while the count of completed frames is in FRAME .. FRAME+HOLD-1.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Press {
    pub mask: u16,
    pub start: i64,
    pub hold: i64,
}

pub fn parse_press(spec: &str) -> Result<Press, String> {
    let bad = || format!("bad press {spec:?}: BUTTONS@FRAME+HOLD");
    let (buttons, when) = spec.split_once('@').ok_or_else(bad)?;
    let (start, hold) = match when.split_once('+') {
        Some((start, hold)) => (start, hold),
        None => (when, "1"),
    };
    let mut mask = 0;
    for name in buttons.to_lowercase().split('-') {
        mask |= match name {
            "up" => pads::UP,
            "down" => pads::DOWN,
            "left" => pads::LEFT,
            "right" => pads::RIGHT,
            "i" => pads::I,
            "ii" => pads::II,
            "select" => pads::SELECT,
            "run" => pads::RUN,
            "iii" => pads::III,
            "iv" => pads::IV,
            "v" => pads::V,
            "vi" => pads::VI,
            _ => return Err(bad()),
        };
    }
    Ok(Press {
        mask,
        start: start.parse().map_err(|_| bad())?,
        hold: hold.parse().map_err(|_| bad())?,
    })
}

/// The buttons down while `frame` frames have completed.
pub fn held(presses: &[Press], frame: i64) -> u16 {
    presses
        .iter()
        .filter(|press| (press.start..press.start + press.hold).contains(&frame))
        .fold(0, |mask, press| mask | press.mask)
}

/// How a traced machine is built: the command line shared by `pce-trace`,
/// `pce-state-roundtrip` and `scripts/pce_trace.py` (which builds the reference the same way).
#[derive(Clone, Debug)]
pub struct Setup {
    pub image: Vec<u8>,
    pub machine: String,
    pub frames: i64,
    pub presses: Vec<Press>,
    pub seed: Option<i64>,
    pub start_line: i64,
    pub start_offset: i64,
    pub vblank_dots: Option<i64>,
    pub dma_resumes: bool,
    pub psg: crate::psg::PsgSwitches,
    pub timer_window: i64,
    pub lines_follow_vce_bit2: bool,
    /// `pad2` (default), `pad6`, `multitap`, `multitap-clr-low`, `none`, `fake-controller`,
    /// `lockstep-card`, `lockstep-port`, `lockstep-path`, `tennokoe`
    pub device: String,
    pub detail: Option<i64>,
    pub out: Option<String>,
    /// Arguments this parser does not know (a binary's own).
    pub rest: Vec<String>,
}

impl Setup {
    /// `IMAGE [--machine pce|tg16] [--frames N] [--press SPEC]... [--seed N] [--start-line N]
    /// [--start-offset N] [--vblank-dots N|none] [--no-dma-resume]
    /// [--psg NAME=VALUE]... [--psg-variant NAME] [--timer-window N] [--no-bit2-lines]
    /// [--device NAME] [--detail FRAME] [--out FILE]`
    /// The reference's defaults around a card image.
    pub fn for_image(image: Vec<u8>) -> Setup {
        Setup {
            image,
            machine: "pce".to_string(),
            frames: 60,
            presses: Vec::new(),
            seed: None,
            start_line: 0,
            start_offset: 0,
            vblank_dots: Some(-16),
            dma_resumes: true,
            psg: crate::psg::PsgSwitches::default(),
            timer_window: 3,
            lines_follow_vce_bit2: true,
            device: "pad2".to_string(),
            detail: None,
            out: None,
            rest: Vec::new(),
        }
    }

    pub fn parse(args: &[String]) -> Result<Setup, String> {
        let mut setup = Setup::for_image(Vec::new());
        let mut path = None;
        let mut iter = args.iter();
        let number = |text: &String| -> Result<i64, String> {
            text.parse().map_err(|_| format!("not a number: {text}"))
        };
        while let Some(arg) = iter.next() {
            let mut value = || iter.next().ok_or_else(|| format!("{arg} needs a value"));
            match arg.as_str() {
                "--machine" => setup.machine = value()?.clone(),
                "--frames" => setup.frames = number(value()?)?,
                "--press" => setup.presses.push(parse_press(value()?)?),
                "--seed" => setup.seed = Some(number(value()?)?),
                "--start-line" => setup.start_line = number(value()?)?,
                "--start-offset" => setup.start_offset = number(value()?)?,
                "--vblank-dots" => {
                    let text = value()?;
                    setup.vblank_dots = if text == "none" {
                        None
                    } else {
                        Some(number(text)?)
                    };
                }
                "--no-dma-resume" => setup.dma_resumes = false,
                "--psg-variant" => {
                    let name = value()?;
                    setup.psg = crate::psg::PsgSwitches::variant(name)
                        .ok_or_else(|| format!("no PSG variant {name}"))?;
                }
                "--psg" => {
                    let text = value()?;
                    let (name, to) = text
                        .split_once('=')
                        .ok_or_else(|| format!("--psg NAME=VALUE, not {text}"))?;
                    set_psg_switch(&mut setup.psg, name, to)?;
                }
                "--timer-window" => setup.timer_window = number(value()?)?,
                "--no-bit2-lines" => setup.lines_follow_vce_bit2 = false,
                "--device" => setup.device = value()?.clone(),
                "--detail" => setup.detail = Some(number(value()?)?),
                "--out" => setup.out = Some(value()?.clone()),
                _ if path.is_none() && !arg.starts_with("--") => path = Some(arg.clone()),
                _ => setup.rest.push(arg.clone()),
            }
        }
        let path = path.ok_or("no card image given")?;
        setup.image = std::fs::read(&path).map_err(|error| format!("{path}: {error}"))?;
        Ok(setup)
    }

    pub fn machine(&self) -> Result<crate::machine::Machine, String> {
        let mut machine = match self.machine.as_str() {
            "pce" => crate::machine::PC_ENGINE,
            "tg16" => crate::machine::TURBOGRAFX_16,
            other => return Err(format!("no machine {other}: pce or tg16")),
        };
        machine.lines_follow_vce_bit2 = self.lines_follow_vce_bit2;
        if self.device == "lockstep-path" {
            machine.vdc_count = 2;
        }
        Ok(machine)
    }

    /// One more machine, built the same way each time.
    pub fn build(&self) -> Result<System, String> {
        use crate::board::{Board, BoardOptions};
        use crate::fake_devices::{FakeController, LockstepCard, LockstepPath, LockstepPort};
        use crate::sockets::{Card, ControllerPort, PixelPath, PortDevice};

        let device = self.device.as_str();
        let card: Box<dyn Card> = match device {
            "lockstep-card" => Box::new(LockstepCard::new(&self.image)?),
            "tennokoe" => Box::new(crate::cards::TennokoeCard::new(&self.image)?),
            _ => crate::cards::make_card(&self.image)?,
        };
        let pad: Option<Box<dyn ControllerPort>> = match device {
            "none" => None,
            "pad6" => Some(Box::new(pads::Pad6::new())),
            "fake-controller" => Some(Box::new(FakeController::new(0x05))),
            "multitap" | "multitap-clr-low" => {
                let mut tap = pads::MultiTap::new(vec![
                    Some(Box::new(pads::Pad2::new())),
                    None,
                    Some(Box::new(pads::Pad6::new())),
                    None,
                    Some(Box::new(pads::Pad2::new())),
                ])?;
                tap.clr_high_reads_pad = device == "multitap";
                Some(Box::new(tap))
            }
            _ => Some(Box::new(pads::Pad2::new())),
        };
        let port: Option<Box<dyn PortDevice>> = match device {
            "lockstep-port" => Some(Box::new(LockstepPort::new())),
            _ => None,
        };
        let pixel_path: Option<Box<dyn PixelPath>> = match device {
            "lockstep-path" => Some(Box::new(LockstepPath::default())),
            _ => None,
        };
        let board = Board::new(BoardOptions {
            machine: self.machine()?,
            card: Some(card),
            port,
            pad,
            pixel_path,
            power_on_seed: self.seed,
            start_line: self.start_line,
            start_offset: self.start_offset,
            vblank_dots: self.vblank_dots,
            dma_resumes: self.dma_resumes,
            psg: self.psg,
            timer_underflow_window_mclk: self.timer_window,
        })?;
        Ok(System::new(board))
    }

    /// Hold the buttons of the presses for the frame about to run. Behind a multitap the same
    /// buttons go to ports 0 and 2.
    pub fn apply_presses(&self, system: &mut System) {
        let mask = held(&self.presses, system.frame());
        let board = system.board_mut();
        board.set_pressed(0, mask);
        if self.device.starts_with("multitap") {
            board.set_pressed(2, mask);
        }
    }
}

/// One of the PSG's switches by the reference's attribute name.
pub fn set_psg_switch(
    switches: &mut crate::psg::PsgSwitches,
    name: &str,
    value: &str,
) -> Result<(), String> {
    use crate::psg::KeyOnMode;
    let flag = || match value {
        "true" | "True" | "1" => Ok(true),
        "false" | "False" | "0" => Ok(false),
        _ => Err(format!("{name}: true or false, not {value}")),
    };
    let number = || {
        value
            .parse::<i64>()
            .map_err(|_| format!("{name}: a number, not {value}"))
    };
    match name {
        "write_wave_while_playing" => switches.write_wave_while_playing = flag()?,
        "dda_latch_needs_on" => switches.dda_latch_needs_on = flag()?,
        "r6_overrides_sample_while_on" => switches.r6_overrides_sample_while_on = flag()?,
        "keyon_mode" => {
            switches.keyon_mode = match value {
                "reload" => KeyOnMode::Reload,
                "resume_step" => KeyOnMode::ResumeStep,
                _ => return Err(format!("keyon_mode: reload or resume_step, not {value}")),
            }
        }
        "lfo_requires_ch2_on" => switches.lfo_requires_ch2_on = flag()?,
        "lfo_zero_multiplier" => switches.lfo_zero_multiplier = number()?,
        "lfo_mask12" => switches.lfo_mask12 = flag()?,
        "lf_trg_halts_always" => switches.lf_trg_halts_always = flag()?,
        "scale_table_quirk" => switches.scale_table_quirk = flag()?,
        "silent_units" => switches.silent_units = number()? as usize,
        "render_audio" => switches.render_audio = flag()?,
        "noise_zero_period" => switches.noise_zero_period = number()?,
        "noise_free_running" => switches.noise_free_running = flag()?,
        _ => return Err(format!("no PSG switch {name}")),
    }
    Ok(())
}

/// Run `frames` frames with the setup's presses; a line a frame.
pub fn run_frames(setup: &Setup, system: &mut System, frames: i64) -> Vec<String> {
    (0..frames)
        .map(|_| {
            setup.apply_presses(system);
            system.run_frame();
            frame_line(system)
        })
        .collect()
}

fn started(setup: &Setup) -> Result<System, String> {
    let mut system = setup.build()?;
    system.board_mut().bus_log = Some(Vec::new());
    Ok(system)
}

fn settle(system: &mut System) {
    if let Some(log) = system.board_mut().bus_log.as_mut() {
        log.clear();
    }
    system.board_mut().take_audio();
}

/// Save states, proven complete (the reference's `scripts/state_roundtrip.py` `roundtrip`):
/// power on, run to frame `at` plus `extra` instructions (so the save falls anywhere in a
/// frame, between two instructions), save, then build a second system the same way, load the
/// state into it, and run both to frame `to`. Every frame after the save must be the same on
/// both, by the gate's own lines. A third run loads the state into a system that has run
/// elsewhere first (to frame `at` / 2); and the loaded system, saved at once, must give the
/// same bytes back. `Ok` says what was compared, `Err` what differed.
pub fn roundtrip(setup: &Setup, at: i64, to: i64, extra: i64) -> Result<String, String> {
    use crate::state::{load_state, save_state};
    let mut first = started(setup)?;
    run_frames(setup, &mut first, at);
    for _ in 0..extra {
        first.step();
    }
    let state = save_state(&first);
    settle(&mut first);
    let expected = run_frames(setup, &mut first, to - at);
    for (label, warm) in [
        ("a fresh system", 0),
        ("a system that had run", 1.max(at / 2)),
    ] {
        let mut second = started(setup)?;
        run_frames(setup, &mut second, warm);
        load_state(&mut second, &state).map_err(|error| format!("{label}: {error}"))?;
        if save_state(&second) != state {
            return Err(format!(
                "{label}: saved again at once, the state is not the same bytes"
            ));
        }
        settle(&mut second);
        let got = run_frames(setup, &mut second, to - at);
        for (n, (a, b)) in expected.iter().zip(&got).enumerate() {
            if a != b {
                return Err(format!(
                    "{label}: differs in frame {} after the save\n  expected {a}\n  got      {b}",
                    n + 1
                ));
            }
        }
    }
    Ok(format!(
        "{} bytes; {} frames after the save equal, twice",
        state.len(),
        to - at
    ))
}

/// The snapshot pair in a run-ahead pass: every frame is run, a snapshot taken, `ahead` more
/// frames run with other buttons held, and the snapshot put back. Each frame's line, and the
/// state at the end, must equal those of a run that never looked ahead.
pub fn runahead(setup: &Setup, frames: i64, ahead: i64) -> Result<String, String> {
    use crate::state::{load_snapshot, save_snapshot, save_state};
    let mut plain = started(setup)?;
    let expected = run_frames(setup, &mut plain, frames);
    let mut system = started(setup)?;
    let mut bytes = 0;
    for (n, wanted) in expected.iter().enumerate() {
        setup.apply_presses(&mut system);
        system.run_frame();
        let line = frame_line(&mut system);
        if &line != wanted {
            return Err(format!(
                "frame {} differs after a snapshot was put back\n  expected {wanted}\n  got      {line}",
                n + 1
            ));
        }
        let snapshot = save_snapshot(&system);
        bytes = snapshot.len();
        for _ in 0..ahead {
            system
                .board_mut()
                .set_pressed(0, pads::RUN | pads::I | pads::RIGHT);
            system.run_frame();
        }
        load_snapshot(&mut system, &snapshot)?;
        if let Some(log) = system.board_mut().bus_log.as_mut() {
            log.clear();
        }
    }
    if save_state(&system) != save_state(&plain) {
        return Err("the end state is not the plain run's".to_string());
    }
    Ok(format!(
        "{frames} frames, {ahead} ahead of each, equal to the plain run; a snapshot is {bytes} bytes"
    ))
}
