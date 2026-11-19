// SPDX-License-Identifier: MIT
//! Python's `random.Random(seed)` for an integer seed, as far as the reference's
//! `power_on_seed` uses it: the Mersenne Twister MT19937 seeded by `init_by_array` over the
//! seed's 32-bit words, and `getrandbits(k)` for k <= 32 (the top k bits of one output).
//!
//! Not a module of the reference: Rust has no `random`, and a seeded power-on must give the
//! same RAM, VRAM, sprite table and colour table on both boards. Written from the algorithm's
//! published description (Matsumoto and Nishimura, 1998, and the 2002 `init_by_array`).

const N: usize = 624;
const M: usize = 397;

pub struct PythonRandom {
    state: [u32; N],
    index: usize,
}

impl PythonRandom {
    /// `random.Random(seed)`: the seed's absolute value, split into 32-bit words from the
    /// least significant, is the key of `init_by_array`.
    pub fn new(seed: i64) -> PythonRandom {
        let mut magnitude = seed.unsigned_abs();
        let mut key = vec![magnitude as u32];
        magnitude >>= 32;
        if magnitude != 0 {
            key.push(magnitude as u32);
        }
        let mut rng = PythonRandom {
            state: [0; N],
            index: N,
        };
        rng.init_genrand(19650218);
        let (mut i, mut j) = (1usize, 0usize);
        for _ in 0..N.max(key.len()) {
            let previous = rng.state[i - 1];
            rng.state[i] = (rng.state[i] ^ (previous ^ (previous >> 30)).wrapping_mul(1664525))
                .wrapping_add(key[j])
                .wrapping_add(j as u32);
            i += 1;
            j += 1;
            if i >= N {
                rng.state[0] = rng.state[N - 1];
                i = 1;
            }
            if j >= key.len() {
                j = 0;
            }
        }
        for _ in 0..N - 1 {
            let previous = rng.state[i - 1];
            rng.state[i] = (rng.state[i] ^ (previous ^ (previous >> 30)).wrapping_mul(1566083941))
                .wrapping_sub(i as u32);
            i += 1;
            if i >= N {
                rng.state[0] = rng.state[N - 1];
                i = 1;
            }
        }
        rng.state[0] = 0x8000_0000;
        rng
    }

    fn init_genrand(&mut self, seed: u32) {
        self.state[0] = seed;
        for i in 1..N {
            let previous = self.state[i - 1];
            self.state[i] = 1812433253u32
                .wrapping_mul(previous ^ (previous >> 30))
                .wrapping_add(i as u32);
        }
        self.index = N;
    }

    fn next_u32(&mut self) -> u32 {
        if self.index >= N {
            for k in 0..N {
                let y = (self.state[k] & 0x8000_0000) | (self.state[(k + 1) % N] & 0x7FFF_FFFF);
                let mag = if y & 1 != 0 { 0x9908_B0DF } else { 0 };
                self.state[k] = self.state[(k + M) % N] ^ (y >> 1) ^ mag;
            }
            self.index = 0;
        }
        let mut y = self.state[self.index];
        self.index += 1;
        y ^= y >> 11;
        y ^= (y << 7) & 0x9D2C_5680;
        y ^= (y << 15) & 0xEFC6_0000;
        y ^ (y >> 18)
    }

    /// `getrandbits(k)` for 1 <= k <= 32.
    pub fn getrandbits(&mut self, k: u32) -> u32 {
        self.next_u32() >> (32 - k)
    }
}

#[cfg(test)]
mod tests {
    use super::PythonRandom;

    #[test]
    fn matches_cpython() {
        // random.Random(1).getrandbits(32) three times, then getrandbits(9), getrandbits(16)
        let mut rng = PythonRandom::new(1);
        assert_eq!(rng.getrandbits(32), 577090037);
        assert_eq!(rng.getrandbits(32), 2444712010);
        assert_eq!(rng.getrandbits(32), 3639700191);
        let mut rng = PythonRandom::new(12345);
        assert_eq!(rng.getrandbits(9), 213);
        assert_eq!(rng.getrandbits(16), 48017);
    }
}
