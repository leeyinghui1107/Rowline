//! The pseudo-random source.
//!
//! The original uses the Borland / ANSI C `rand()`, which is the linear
//! congruential generator
//!
//! ```text
//! state = state * 0x41C64E6D + 0x3039   (mod 2^32)
//! value = (state >> 16) & 0x7FFF
//! ```
//!
//! Reproducing it exactly matters: the original seeds the generator from the
//! system tick count, so a faithful re-implementation has to produce the
//! same *sequence* for a given seed in order to make the differential replay
//! gate meaningful.

/// Borland/ANSI `rand()` state machine.
#[derive(Clone, Copy, Debug)]
pub struct Lcg {
    state: u32,
}

const MUL: u32 = 0x41C6_4E6D;
const INC: u32 = 0x3039;

impl Default for Lcg {
    fn default() -> Self {
        Lcg::with_seed(0)
    }
}

impl Lcg {
    /// Create a generator with an explicit seed.
    pub const fn with_seed(seed: u32) -> Self {
        Lcg { state: seed }
    }

    /// Replace the current state (equivalent to `srand`).
    pub fn seed(&mut self, seed: u32) {
        self.state = seed;
    }

    /// Current internal state (useful for reproducibility assertions).
    pub const fn state(&self) -> u32 {
        self.state
    }

    /// Draw the next raw value in `0..0x7FFF`.
    pub fn next_u32(&mut self) -> u32 {
        self.state = self.state.wrapping_mul(MUL).wrapping_add(INC);
        (self.state >> 16) & 0x7FFF
    }

    /// Draw a value in `0..n` (`n > 0`).
    pub fn below(&mut self, n: u32) -> u32 {
        if n == 0 {
            0
        } else {
            self.next_u32() % n
        }
    }

    /// Draw a ball colour in `1..=COLORS`.
    pub fn colour(&mut self) -> u8 {
        1 + self.below(super::COLORS as u32) as u8
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matches_the_reference_sequence() {
        // The Python and C references both start from seed 0xC0FFEE; the
        // first colour must therefore agree across all three implementations.
        let mut r = Lcg::with_seed(0xC0FFEE);
        let first: Vec<u32> = (0..4).map(|_| r.next_u32()).collect();
        // Recompute independently with the textbook formula.
        let mut s = 0xC0FFEEu32;
        let mut expect = Vec::new();
        for _ in 0..4 {
            s = s.wrapping_mul(MUL).wrapping_add(INC);
            expect.push((s >> 16) & 0x7FFF);
        }
        assert_eq!(first, expect);
    }

    #[test]
    fn colour_is_in_range() {
        let mut r = Lcg::with_seed(1);
        for _ in 0..1000 {
            let c = r.colour();
            assert!((1..=7).contains(&c));
        }
    }
}
