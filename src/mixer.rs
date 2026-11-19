// SPDX-License-Identifier: MIT
//! The audio mixer: sums the board's audio sources (socket 9) into one stereo stream
//! (`mixer.py`).
//!
//! A source answers `take_samples(mclk)` with the stereo samples it produced since the last
//! call, one per 1/`rate_hz` second (zeros when silent, so the mixer keeps its place). The mixer
//! keeps one queue per source, turns each source's stream into the output rate, applies `gain`
//! (unless the source applies its own: `applies_gain`), sums sample for sample and clips to 16
//! bits. It emits only as many samples as every source has produced, so the sources need not
//! agree on how many each call returns.
//!
//! A source at another rate is held (zero-order) up to the output rate: no filter, no interpolation.
//! The reference for a source is its own stream at its own rate; this is a host-side convenience
//! (docs/psg.md says the same of the PSG).
//!
//! Differences forced by Rust: the sources are handed to `take` (the reference's mixer holds
//! the board's list itself); the arithmetic is the reference's, in `f64`, in the same order
//! (the sources in list order, `left * gain` added to a sum that starts at 0.0, `round` to
//! even, then the clip).

use serde::{Deserialize, Serialize};

use crate::machine::round_ties_even;
use crate::sockets::AudioSource;

pub const OUTPUT_HZ: f64 = 44_100.0;

/// One source's samples at the output rate, waiting for the others.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Lane {
    pub queue: Vec<(i32, i32)>,
    pub carry: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Mixer {
    pub output_hz: f64,
    /// one per source, in the sources' order (sources only grow)
    lanes: Vec<Lane>,
}

impl Default for Mixer {
    fn default() -> Self {
        Mixer::new(OUTPUT_HZ)
    }
}

impl Mixer {
    pub fn new(output_hz: f64) -> Mixer {
        Mixer {
            output_hz,
            lanes: Vec::new(),
        }
    }

    /// Mixed stereo samples at `output_hz` that every source has produced up to `mclk`.
    pub fn take(&mut self, mclk: i64, mut sources: Vec<&mut dyn AudioSource>) -> Vec<(i16, i16)> {
        while self.lanes.len() < sources.len() {
            self.lanes.push(Lane::default());
        }
        let output_hz = self.output_hz;
        for (source, lane) in sources.iter_mut().zip(self.lanes.iter_mut()) {
            let samples = source.take_samples(mclk);
            feed(output_hz, &**source, lane, samples);
        }
        if sources.is_empty() {
            return Vec::new();
        }
        let lanes = &mut self.lanes[..sources.len()];
        let count = lanes.iter().map(|lane| lane.queue.len()).min().unwrap_or(0);
        let mut out = vec![[0.0f64, 0.0f64]; count];
        for (source, lane) in sources.iter().zip(lanes.iter_mut()) {
            let gain = if source.applies_gain() {
                1.0
            } else {
                source.gain()
            };
            for (mix, &(left, right)) in out.iter_mut().zip(lane.queue.iter()) {
                mix[0] += left as f64 * gain;
                mix[1] += right as f64 * gain;
            }
            lane.queue.drain(..count);
        }
        out.iter().map(|mix| (clip(mix[0]), clip(mix[1]))).collect()
    }
}

fn feed(output_hz: f64, source: &dyn AudioSource, lane: &mut Lane, samples: Vec<(i32, i32)>) {
    if source.rate_hz() == output_hz {
        lane.queue.extend(samples);
        return;
    }
    let step = output_hz / source.rate_hz();
    for sample in samples {
        lane.carry += step;
        let repeats = lane.carry as i64; // Python's int(): toward zero
        lane.carry -= repeats as f64;
        for _ in 0..repeats {
            lane.queue.push(sample);
        }
    }
}

fn clip(value: f64) -> i16 {
    round_ties_even(value).clamp(-32768, 32767) as i16
}
