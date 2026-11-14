//! The HuC6280's programmable sound generator: six wavetable channels, noise, LFO, DDA
//! (`psg/chip.py`).
//!
//! Written (in the reference) from Hudson's *HuC6280 PSG Manual* (tier 3: "manual" below) and
//! MacDonald's pcetech §10 and §2.4 (tier 2); where those are silent the choice is named in a
//! comment with a U/C number (docs/m4-record.md) and may be changed by a switch. There is no
//! hardware audio reference anywhere: **every output of this module is provisional, tier 5**.
//!
//! Time: one *step* is one PSG clock, master/6 = 3.58 MHz [pcetech 2.4; the manual's "7.16 MHz" and
//! its formulas' factor 2 agree]. A channel's waveform index advances every F steps (F = the 12-bit
//! frequency, 0 meaning 4096) [pcetech 10: $000 is the lowest frequency; manual 2.2.1:
//! f = fmaster / (2 x 32 x F)]. The model is event-driven: output is constant between index changes.
//!
//! Within a step every channel's output is taken from the state at the start of the step and the
//! counters then advance together (the manual says nothing of an order): ch 1's modulation of ch 0
//! therefore reads ch 2's sample *before* ch 2 advances in the same step.
//!
//! Differences forced by Rust: the class attributes are the fields of `PsgSwitches`
//! (`Psg::switches`), with `variant("mame")` and `variant("mednafen")` for the reference's
//! `VARIANTS`; `keyon_mode` is an enum. The step-stream listeners (the reference's referee
//! comparison, Python-side tooling) are not carried over. The host-rate arithmetic is the
//! reference's, in `f64`, operation for operation.

use serde::{Deserialize, Serialize};

use crate::cpu_io::PsgPins;
use crate::machine::{py_floordiv, round_ties_even};
use crate::sockets::{AudioSource, DeviceState};

pub const PSG_CLOCK_DIVIDER: i64 = 6; // master clocks per step [pcetech 2.4]
pub const NOISE_TAPS: [u32; 5] = [0, 1, 11, 12, 17]; // [emu: MAME and Mednafen agree; MacDonald's research] U6
pub const SILENT_UNITS: usize = 30; // [mfr 2.3] about 45 dB below the maximum "none is output"; 1 unit = 1.5 dB

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Channel {
    pub frequency: i64,
    /// bit 7 ON, bit 6 DDA, bits 4-0 AL
    pub control: u8,
    pub balance: u8,
    pub wave: [u8; 32],
    pub index: usize,
    /// the DAC value held by DDA mode
    pub latch: u8,
    pub noise_control: u8,
    /// steps until the waveform index advances
    pub counter: i64,
    pub noise_counter: i64,
    pub lfsr: u32,
    /// C16: a sample forced by an R6 write until the next address step
    pub override_sample: Option<u8>,
}

impl Default for Channel {
    fn default() -> Self {
        Channel {
            frequency: 0,
            control: 0,
            balance: 0,
            wave: [0; 32],
            index: 0,
            latch: 0,
            noise_control: 0,
            counter: 1,
            noise_counter: 1,
            lfsr: 1,
            override_sample: None,
        }
    }
}

impl Channel {
    #[inline]
    pub fn on(&self) -> bool {
        self.control & 0x80 != 0
    }

    #[inline]
    pub fn dda(&self) -> bool {
        self.control & 0x40 != 0
    }
}

/// C22: what key-on (ON 0 -> 1) does to a waveform channel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum KeyOnMode {
    /// the counter restarts from F, the address stays (MAME)
    Reload,
    /// the counter resumes where it stopped and the address steps once (Mednafen)
    ResumeStep,
}

/// The points on which the manual and pcetech are silent or the two readable emulators differ
/// (docs/m4-record.md, C13-C23). Each default below follows the manual or pcetech where they
/// speak, else the larger share of the sources; `variant` flips them to one emulator's way.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct PsgSwitches {
    /// C13: data written to R6 while a channel is ON and not DDA. pcetech: ignored, the index does
    /// not move. MAME and Mednafen store it in the waveform. True follows the two emulators.
    pub write_wave_while_playing: bool,
    /// C16: R6 in DDA mode loads the DAC latch: the manual says whenever DDA is set (also with the
    /// channel off); Mednafen only with the channel on, and it also lets an R6 write while playing
    /// override the sample until the next address step ("tested on a SuperGrafx").
    pub dda_latch_needs_on: bool,
    pub r6_overrides_sample_while_on: bool,
    /// C22: neither reading is in the manual.
    pub keyon_mode: KeyOnMode,
    /// C18: the LFO acts only when channel 2 is on (manual); MAME modulates whatever channel 2 is.
    pub lfo_requires_ch2_on: bool,
    /// C19: the LFO frequency register 0: 256 (Mednafen) or 1 (MAME's zero-length counter).
    pub lfo_zero_multiplier: i64,
    /// U6: the modulated frequency wraps in 12 bits (Mednafen) or not (MAME).
    pub lfo_mask12: bool,
    /// C23: LF TRG (R9 bit 7) halts channel 2 whatever LF CTL holds (Mednafen); MAME only with LF
    /// CTL non-zero.
    pub lf_trg_halts_always: bool,
    /// C15: R1/R5's table: the manual says 3 dB a step; both emulators share a table in which the
    /// step from 7 to 8 is 1.5 dB and from 8 to 9 4.5 dB (index 8 costs 15 units, not 14). The
    /// sixteen values in `scale` are that table, as in MAME's c6280.cpp (BSD-3-Clause, Charles
    /// MacDonald; see NOTICE.md).
    pub scale_table_quirk: bool,
    /// C14: attenuation at which a channel goes silent, in 1.5 dB units. The manual: 45 dB (30).
    /// Both emulators keep a 31st step (46.5 dB) that is still audible.
    pub silent_units: usize,
    /// False skips the host-rate output.
    pub render_audio: bool,
    /// C17: noise frequency NF' = 0 ("undefined" in the manual). MAME clocks the shift register
    /// every step; Mednafen every 32. 32 here.
    pub noise_zero_period: i64,
    /// C20: the noise shift register runs whether or not noise is selected (Mednafen); MAME runs it
    /// only while a channel is playing noise.
    pub noise_free_running: bool,
}

impl Default for PsgSwitches {
    fn default() -> Self {
        PsgSwitches {
            write_wave_while_playing: false,
            dda_latch_needs_on: false,
            r6_overrides_sample_while_on: false,
            keyon_mode: KeyOnMode::Reload,
            lfo_requires_ch2_on: true,
            lfo_zero_multiplier: 256,
            lfo_mask12: true,
            lf_trg_halts_always: true,
            scale_table_quirk: false,
            silent_units: SILENT_UNITS,
            render_audio: true,
            noise_zero_period: 32,
            noise_free_running: true,
        }
    }
}

impl PsgSwitches {
    /// Each readable emulator's way on the disputed points (the reference's `VARIANTS`):
    /// `"manual"` (the defaults), `"mame"`, `"mednafen"`.
    pub fn variant(name: &str) -> Option<PsgSwitches> {
        let manual = PsgSwitches::default();
        match name {
            "manual" => Some(manual),
            "mame" => Some(PsgSwitches {
                write_wave_while_playing: true,
                noise_zero_period: 1,
                noise_free_running: false,
                keyon_mode: KeyOnMode::Reload,
                lfo_requires_ch2_on: false,
                lfo_zero_multiplier: 1,
                lfo_mask12: false,
                lf_trg_halts_always: false,
                scale_table_quirk: true,
                silent_units: 31,
                ..manual
            }),
            "mednafen" => Some(PsgSwitches {
                write_wave_while_playing: true,
                dda_latch_needs_on: true,
                r6_overrides_sample_while_on: true,
                noise_zero_period: 32,
                noise_free_running: true,
                keyon_mode: KeyOnMode::ResumeStep,
                lfo_requires_ch2_on: true,
                lfo_zero_multiplier: 256,
                lfo_mask12: true,
                lf_trg_halts_always: true,
                scale_table_quirk: true,
                silent_units: 31,
                ..manual
            }),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Psg {
    /// Switches, not state: a loaded state keeps the system's own.
    pub switches: PsgSwitches,
    pub channels: [Channel; 6],
    pub select: u8,
    /// R1
    pub balance: u8,
    /// R8
    pub lfo_frequency: u8,
    /// R9
    pub lfo_control: u8,
    /// steps run so far
    pub step: i64,
    // host-rate output (a box average of the step stream; outside the reference path)
    pub rate_hz: f64,
    pub gain: f64,
    /// up to 30 + 31 + 30; rebuilt when `switches.silent_units` changes (issue 0004: the
    /// reference's psg_compare.py sets the switches on an instance after construction)
    levels: Vec<f64>,
    /// the `silent_units` `levels` was built for (the reference's `_levels_units`; `None` is its
    /// -1, never built)
    levels_units: Option<usize>,
    acc: [f64; 2],
    phase: f64,
    out: Vec<(i32, i32)>,
    noise_period: [i64; 6],
}

impl Default for Psg {
    fn default() -> Self {
        Psg::new(44100.0, PsgSwitches::default())
    }
}

/// `10 ** (-1.5 * u / 20)` for u in 0..128, as the bits of each double: the reference's level curve
/// (psg/chip.py `_levels`). Written out because `powf` is the platform's and is not the same
/// everywhere: under wasm32 nine of these came out one unit in the last place away, so a state
/// saved in a browser would not have had a native build's bytes. A test checks this build's `powf`
/// against the table where the two are expected to agree.
pub const LEVEL_BITS: [u64; 128] = [
    0x3ff0000000000000,
    0x3feaecb5810c240d,
    0x3fe6a77deae8ab8a,
    0x3fe30faa0e3d83f5,
    0x3fe009b9cf334252,
    0x3fdafd1354c40d50,
    0x3fd6b54337bc3b65,
    0x3fd31b4039db843f,
    0x3fd0137987dd704c,
    0x3fcb0d7b1b53e057,
    0x3fc6c310e3769f3f,
    0x3fc326dd708071b0,
    0x3fc01d3f2d9684d0,
    0x3fbb1decdac7bf4d,
    0x3fb6d0e6f32e6ea2,
    0x3fb33281b6744ae1,
    0x3fb0270ac3f8a9fa,
    0x3fab2e68992f7968,
    0x3fa6dec56bfd58e5,
    0x3fa33e2d1001a8a5,
    0x3fa030dc4ea03a72,
    0x3f9b3eee5c9e8d27,
    0x3f96ecac53002712,
    0x3f9349df8175bf98,
    0x3f903ab3d12bc2c4,
    0x3f8b4f7e2b2c2a95,
    0x3f86fa9bad56bdb7,
    0x3f8355990f2061bf,
    0x3f8044914f3c02b0,
    0x3f7b60180af33584,
    0x3f77089380241edf,
    0x3f736159bd540009,
    0x3f704e74cc73ee88,
    0x3f6b70bc021247bd,
    0x3f671693d08e6bee,
    0x3f636d219065ac0c,
    0x3f60585e4c78b079,
    0x3f5b816a16abb36c,
    0x3f57249ca3bee77b,
    0x3f5378f08cad1974,
    0x3f50624dd2f1a9fc,
    0x3f4b92224ee58528,
    0x3f4732adfee1f757,
    0x3f4384c6b6849fb2,
    0x3f406c4363887513,
    0x3f3ba2e4b0e98677,
    0x3f3740c7e7272648,
    0x3f3390a412493ba6,
    0x3f30763f01e8e5ad,
    0x3f2bb3b142e53fda,
    0x3f274eea61c12623,
    0x3f239c88a45a9114,
    0x3f208040b1c10b13,
    0x3f1bc4880b09fb2e,
    0x3f175d1573e5d18e,
    0x3f13a874711aec6c,
    0x3f108a4876c1311e,
    0x3f0bd5690f8cc604,
    0x3f076b4922ce2e00,
    0x3f03b4677cef4441,
    0x3f009456549be1bd,
    0x3efbe65456a673da,
    0x3ef7798573b66d9e,
    0x3ef3c061cc3f3b00,
    0x3ef09e6a4f05e62b,
    0x3eebf749e693a057,
    0x3ee787ca6bddf15c,
    0x3ee3cc636375207c,
    0x3ee0a88469b64867,
    0x3edc0849c594b1ae,
    0x3ed7961810874a9f,
    0x3ed3d86c46fdf3c0,
    0x3ed0b2a4a866547e,
    0x3ecc1953f9eddae6,
    0x3ec7a46e66f83d5f,
    0x3ec3e47c7b496466,
    0x3ec0bccb0ed19a15,
    0x3ebc2a6889e71e19,
    0x3eb7b2cd7479c20e,
    0x3eb3f09404c9d46e,
    0x3eb0c6f7a0b5ed8d,
    0x3eac3b877bcc4f03,
    0x3ea7c1353e58077e,
    0x3ea3fcb2e7f459d2,
    0x3ea0d12a61d3698c,
    0x3e9c4cb0d5ed14f5,
    0x3e97cfa5c9e27506,
    0x3e9408d92940c022,
    0x3e90db6355ec7054,
    0x3e8c5de49e9ced6b,
    0x3e87de1f1c6bac2c,
    0x3e841506cd298a52,
    0x3e80e5a280c5ad1d,
    0x3e7c6f22dc332e4c,
    0x3e77eca13b498acf,
    0x3e74213bd82bf425,
    0x3e70efe7e62615a0,
    0x3e6c806b950b0836,
    0x3e67fb2c2bd52d0e,
    0x3e642d784ec7f3f9,
    0x3e60fa3389d6eb40,
    0x3e5c91becf838911,
    0x3e5809bff36aef39,
    0x3e5439bc35803c6a,
    0x3e5104856fa3bc97,
    0x3e4ca31c91ff9e0b,
    0x3e48185c976a6ffc,
    0x3e44460790da3df0,
    0x3e410edd9b5a66d0,
    0x3e3cb484e2e61623,
    0x3e3827021d36920b,
    0x3e34525a655e28c0,
    0x3e31193c10cb1708,
    0x3e2cc5f7c8a1a4c3,
    0x3e2835b08a357e47,
    0x3e245eb4b796edfa,
    0x3e2123a0d3c84be6,
    0x3e1cd77549a0e36d,
    0x3e184467e3d0a5f2,
    0x3e146b168c1241ee,
    0x3e112e0be826d695,
    0x3e0ce8fd6c565511,
    0x3e0853282f74c416,
    0x3e04777fe7609d84,
    0x3e01387d51bddcae,
    0x3dfcfa9037386761,
    0x3df861f17291e056,
    0x3df483f0ce153f9f,
];

impl Psg {
    pub fn new(rate_hz: f64, switches: PsgSwitches) -> Psg {
        let mut psg = Psg {
            switches,
            channels: Default::default(),
            select: 0,
            balance: 0,
            lfo_frequency: 0,
            lfo_control: 0,
            step: 0,
            rate_hz,
            gain: 1.0,
            levels: Vec::new(),
            levels_units: None,
            acc: [0.0, 0.0],
            phase: 0.0,
            out: Vec::new(),
            noise_period: [0; 6],
        };
        psg.level_table();
        for ch in 0..6 {
            psg.channels[ch].counter = psg.period(ch);
        }
        for ch in [4, 5] {
            psg.set_noise_period(ch);
            psg.channels[ch].noise_counter = 1;
        }
        psg
    }

    /// A loaded state keeps the system's own switches and the table made from them.
    pub(crate) fn keep_configuration(&mut self, live: &Psg) {
        self.switches = live.switches;
        self.levels = live.levels.clone();
        self.levels_units = live.levels_units;
    }

    // -- derived values -------------------------------------------------------------------------

    /// [mfr 2.1.10 note] the LFO acts only when channel 1 and 2 are both on; LF TRG halts it.
    fn lfo_modulating(&self) -> bool {
        if self.lfo_control & 3 == 0 || self.lfo_control & 0x80 != 0 {
            return false;
        }
        self.channels[1].on() || !self.switches.lfo_requires_ch2_on
    }

    fn mask12(&self, frequency: i64) -> i64 {
        if self.switches.lfo_mask12 {
            let masked = frequency & 0xFFF;
            if masked != 0 {
                masked
            } else {
                4096
            }
        } else {
            frequency.max(1)
        }
    }

    fn period(&self, ch: usize) -> i64 {
        let channel = &self.channels[ch];
        let mut frequency = if channel.frequency != 0 {
            channel.frequency
        } else {
            4096
        };
        if ch == 0 && self.lfo_modulating() {
            let shift = ((self.lfo_control & 3) as u32 - 1) * 2; // 0, 2, 4 [mfr 2.1.10]
            let modulation = self.channels[1].wave[self.channels[1].index] as i64 - 16; // [mfr table]
            frequency = self.mask12(channel.frequency + (modulation << shift));
        } else if ch == 1 && self.lfo_control & 3 != 0 {
            // [mfr 2.2.2] f_LFO = fmaster / (2x32xF2xFLF)
            frequency *= if self.lfo_frequency != 0 {
                self.lfo_frequency as i64
            } else {
                self.switches.lfo_zero_multiplier
            };
        }
        frequency
    }

    fn set_noise_period(&mut self, ch: usize) {
        let nf = 0x1F - (self.channels[ch].noise_control & 0x1F) as i64; // [mfr 2.3.3] NF is the complement
        self.noise_period[ch] = if nf != 0 {
            nf * 64
        } else {
            self.switches.noise_zero_period
        }; // 2 x 32 x 2 / 2 steps
    }

    /// Units of 1.5 dB below the maximum for a channel on one side (0 left, 1 right):
    /// R1 and R5 step 3 dB, R4 1.5 dB [mfr 2.1.2, 2.1.5, 2.1.6, 2.3]. 30 or more is silent.
    pub fn attenuation(&self, ch: usize, side: usize) -> usize {
        let channel = &self.channels[ch];
        let shift = if side == 0 { 4 } else { 0 };
        let main = (self.balance >> shift) & 0xF;
        let pan = (channel.balance >> shift) & 0xF;
        self.scale(main) + (31 - (channel.control & 0x1F) as usize) + self.scale(pan)
    }

    /// Units below the maximum for an R1/R5 nibble: 2 a step, 30 for 0 [mfr 2.1.2, 2.1.6].
    fn scale(&self, nibble: u8) -> usize {
        if self.switches.scale_table_quirk {
            const TABLE: [usize; 16] = [0, 3, 5, 7, 9, 11, 13, 15, 16, 19, 21, 23, 25, 27, 29, 31];
            return 31 - TABLE[nibble as usize];
        }
        2 * (15 - nibble as usize)
    }

    // -- the CPU's port --------------------------------------------------------------------------

    pub fn write(&mut self, offset: u32, value: u8, mclk: i64) {
        self.advance_to(py_floordiv(mclk, PSG_CLOCK_DIVIDER));
        self.write_register((offset & 0x0F) as u8, value);
    }

    fn write_register(&mut self, reg: u8, value: u8) {
        if reg == 0 {
            self.select = value & 7;
            return;
        }
        if reg == 1 {
            self.balance = value;
            return;
        }
        if reg == 8 {
            self.lfo_frequency = value; // counters pick up a changed period at their next reload
            return;
        }
        if reg == 9 {
            if value & 0x80 != 0 {
                // [mfr 2.1.10] LF TRG: ch 2's wave counter and address reset
                self.channels[1].index = 0;
                self.channels[1].counter = self.period(1);
            }
            self.lfo_control = value;
            return;
        }
        if reg > 7 || self.select > 5 {
            return; // "no more than 6 channels"
        }
        let select = self.select as usize;
        match reg {
            2 => {
                let channel = &mut self.channels[select];
                channel.frequency = (channel.frequency & 0xF00) | value as i64;
            }
            3 => {
                let channel = &mut self.channels[select];
                channel.frequency = (channel.frequency & 0x0FF) | (((value & 0x0F) as i64) << 8);
            }
            4 => self.write_control(select, value),
            5 => self.channels[select].balance = value,
            6 => self.write_wave(select, value & 0x1F),
            7 if select >= 4 => {
                self.channels[select].noise_control = value;
                self.set_noise_period(select);
            }
            _ => {}
        }
    }

    fn write_control(&mut self, ch: usize, value: u8) {
        let (was_dda, was_on) = (self.channels[ch].dda(), self.channels[ch].on());
        self.channels[ch].control = value;
        if value & 0x40 != 0 {
            self.channels[ch].index = 0; // [mfr 2.1.5] DDA: the address counter is reset
        } else if was_dda {
            self.channels[ch].index = 0; // [pcetech 10] reset when DDA goes 1 -> 0
            self.channels[ch].counter = self.period(ch);
        }
        if self.channels[ch].on() && !was_on && !self.channels[ch].dda() {
            if self.switches.keyon_mode == KeyOnMode::Reload {
                self.channels[ch].counter = self.period(ch);
            } else {
                self.channels[ch].index = (self.channels[ch].index + 1) & 31;
            }
        }
        self.channels[ch].override_sample = None;
    }

    fn write_wave(&mut self, ch: usize, value: u8) {
        let switches = self.switches;
        let channel = &mut self.channels[ch];
        if channel.dda() {
            if channel.on() || !switches.dda_latch_needs_on {
                channel.latch = value; // [mfr 2.1.7 (3)] "data is transferred directly to the D/A"
            }
        } else if !channel.on() {
            channel.wave[channel.index] = value; // [mfr 2.1.5] write data mode
            channel.index = (channel.index + 1) & 31;
        } else if switches.write_wave_while_playing {
            channel.wave[channel.index] = value;
        }
        if switches.r6_overrides_sample_while_on && channel.on() {
            channel.override_sample = Some(value);
        }
    }

    // -- running ----------------------------------------------------------------------------------

    /// Per channel (sample, units left, units right) for the step about to run; sample is
    /// -1 for a channel that is off (it contributes nothing).
    pub fn state(&self) -> [(i32, usize, usize); 6] {
        let mut out = [(0, 0, 0); 6];
        for (ch, channel) in self.channels.iter().enumerate() {
            let sample = if !channel.on() {
                -1
            } else if ch >= 4 && channel.noise_control & 0x80 != 0 {
                // [mfr 2.1.8] noise overrides the wave
                if channel.lfsr & 1 != 0 {
                    31
                } else {
                    0
                }
            } else if channel.dda() {
                channel.latch as i32
            } else if let Some(sample) = channel.override_sample {
                sample as i32
            } else {
                channel.wave[channel.index] as i32
            };
            out[ch] = (sample, self.attenuation(ch, 0), self.attenuation(ch, 1));
        }
        out
    }

    /// Run the steps up to (not including) step `target`.
    pub fn advance_to(&mut self, target: i64) {
        while self.step < target {
            let remaining = target - self.step;
            let mut n = remaining;
            let lfo = self.lfo_modulating();
            for (ch, channel) in self.channels.iter().enumerate() {
                if self.clocked(ch, channel) {
                    n = n.min(channel.counter);
                }
                if ch >= 4 && (self.switches.noise_free_running || plays_noise(channel)) {
                    n = n.min(channel.noise_counter);
                }
            }
            if n < 1 {
                n = 1;
            }
            let snapshot = self.state();
            self.emit(n, &snapshot);
            self.step += n;
            self.tick(n, lfo);
        }
    }

    /// [mfr 2.1.5] the waveform address advances only in mixing mode (ON, not DDA); LF TRG
    /// halts channel 2's counter (Mednafen: whatever the LF CTL bits).
    fn clocked(&self, ch: usize, channel: &Channel) -> bool {
        if !channel.on() || channel.dda() {
            return false;
        }
        if ch == 1 && self.lfo_control & 0x80 != 0 {
            return !self.switches.lf_trg_halts_always && self.lfo_control & 3 == 0;
        }
        true
    }

    fn tick(&mut self, n: i64, lfo: bool) {
        // the modulating sample is read before channel 2 moves in the same step
        let modulation = self.channels[1].wave[self.channels[1].index] as i64;
        for ch in 0..6 {
            if self.clocked(ch, &self.channels[ch]) {
                self.channels[ch].counter -= n;
                if self.channels[ch].counter <= 0 {
                    self.channels[ch].index = (self.channels[ch].index + 1) & 31;
                    self.channels[ch].override_sample = None;
                    if ch == 0 && lfo {
                        let shift = ((self.lfo_control & 3) as u32 - 1) * 2;
                        let frequency = self.channels[ch].frequency + ((modulation - 16) << shift);
                        self.channels[ch].counter = self.mask12(frequency);
                    } else {
                        self.channels[ch].counter = self.period(ch);
                    }
                }
            }
            if ch >= 4 && (self.switches.noise_free_running || plays_noise(&self.channels[ch])) {
                let period = self.noise_period[ch];
                let channel = &mut self.channels[ch];
                channel.noise_counter -= n;
                if channel.noise_counter <= 0 {
                    let mut bit = 0;
                    for tap in NOISE_TAPS {
                        bit ^= (channel.lfsr >> tap) & 1;
                    }
                    channel.lfsr = (channel.lfsr >> 1) | (bit << 17);
                    channel.noise_counter = period;
                }
            }
        }
    }

    // -- host-rate output ---------------------------------------------------------------------------

    fn emit(&mut self, n: i64, snapshot: &[(i32, usize, usize); 6]) {
        if !self.switches.render_audio {
            return;
        }
        let levels = self.level_table();
        let (mut left, mut right) = (0.0f64, 0.0f64);
        for &(sample, ul, ur) in snapshot {
            if sample >= 0 {
                left += levels[ul] * (sample - 16) as f64;
                right += levels[ur] * (sample - 16) as f64;
            }
        }
        let scale = self.gain / (6 * 16) as f64;
        let (left, right) = (left * scale, right * scale);
        let per_sample = 3_579_545.0 / self.rate_hz;
        let mut remaining = n as f64;
        while remaining > 0.0 {
            let room = per_sample - self.phase;
            let take = room.min(remaining);
            self.acc[0] += left * take;
            self.acc[1] += right * take;
            self.phase += take;
            remaining -= take;
            if self.phase >= per_sample - 1e-9 {
                self.out.push((
                    round_ties_even(self.acc[0] / per_sample * 32767.0) as i32,
                    round_ties_even(self.acc[1] / per_sample * 32767.0) as i32,
                ));
                self.acc = [0.0, 0.0];
                self.phase = 0.0;
            }
        }
    }

    /// Amplitude by attenuation units (1.5 dB each), silent from `silent_units` on (C14).
    fn level_table(&mut self) -> &[f64] {
        let units = self.switches.silent_units;
        if self.levels_units != Some(units) {
            self.levels_units = Some(units);
            self.levels = (0..128)
                .map(|u| {
                    if u < units {
                        f64::from_bits(LEVEL_BITS[u])
                    } else {
                        0.0
                    }
                })
                .collect();
        }
        &self.levels
    }

    /// Stereo samples at `rate_hz` up to `mclk` since the last call (a box average of the
    /// step stream; the reference check is the unresampled step stream).
    pub fn take_samples(&mut self, mclk: i64) -> Vec<(i32, i32)> {
        self.advance_to(py_floordiv(mclk, PSG_CLOCK_DIVIDER));
        std::mem::take(&mut self.out)
    }
}

fn plays_noise(channel: &Channel) -> bool {
    channel.on() && channel.noise_control & 0x80 != 0
}

impl PsgPins for Psg {
    fn write(&mut self, offset: u32, value: u8, mclk: i64) {
        Psg::write(self, offset, value, mclk);
    }
}

impl DeviceState for Psg {}

impl AudioSource for Psg {
    fn rate_hz(&self) -> f64 {
        self.rate_hz
    }

    fn gain(&self) -> f64 {
        self.gain
    }

    fn applies_gain(&self) -> bool {
        true // the mixer (mixer.rs) must not apply `gain` a second time
    }

    fn take_samples(&mut self, mclk: i64) -> Vec<(i32, i32)> {
        Psg::take_samples(self, mclk)
    }
}
