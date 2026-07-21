//! A tiny, dependency-free pseudo-random generator.
//!
//! We deliberately avoid the `rand` crate so the engine stays zero-dependency
//! and compiles cleanly to `wasm32` without pulling in `getrandom`/JS shims.
//! This is `xoshiro256**` seeded through `SplitMix64` — statistically strong
//! enough for shuffling and Monte-Carlo equity, and fully seedable so tests are
//! deterministic. The web layer seeds it from `Math.random()`/time.

pub struct Rng {
    s: [u64; 4],
}

impl Rng {
    /// Seed the generator. Any `u64` works; `SplitMix64` spreads it into the
    /// four-word state so even a small or zero seed produces good output.
    pub fn seed(mut seed: u64) -> Rng {
        let mut next = || {
            seed = seed.wrapping_add(0x9E37_79B9_7F4A_7C15);
            let mut z = seed;
            z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
            z ^ (z >> 31)
        };
        Rng {
            s: [next(), next(), next(), next()],
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        let result = self.s[1].wrapping_mul(5).rotate_left(7).wrapping_mul(9);
        let t = self.s[1] << 17;
        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];
        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);
        result
    }

    /// Uniform integer in `0..n` (unbiased via rejection). Panics if `n == 0`.
    pub fn below(&mut self, n: u32) -> u32 {
        assert!(n > 0, "below(0) is undefined");
        // Lemire-style rejection to remove modulo bias.
        let n = n as u64;
        loop {
            let x = self.next_u64() >> 32; // 32 random bits
            let m = x * n;
            let low = m & 0xFFFF_FFFF;
            if low >= n.wrapping_neg() % n {
                return (m >> 32) as u32;
            }
        }
    }

    /// In-place Fisher–Yates shuffle.
    pub fn shuffle<T>(&mut self, slice: &mut [T]) {
        let len = slice.len();
        for i in (1..len).rev() {
            let j = self.below(i as u32 + 1) as usize;
            slice.swap(i, j);
        }
    }
}
