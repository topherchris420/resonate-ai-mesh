//! Seeded pseudo-random numbers with independent named streams.
//!
//! SplitMix64 is implemented here rather than taken from a crate so that the
//! sequence for a given seed can never change with a dependency update. Each
//! purpose ("sensor.agent_01", "human_state") gets its own stream derived from
//! the run seed, so adding or removing one agent does not perturb the noise of
//! any other component.

#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        Self { state: seed }
    }

    /// Independent stream for a named purpose.
    pub fn stream(seed: u64, name: &str) -> Self {
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for byte in name.as_bytes() {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
        let mut mixer = Rng::new(seed ^ hash);
        Rng::new(mixer.next_u64())
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// Uniform in [0, 1) with 53 bits of precision.
    pub fn unit(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 * (1.0 / (1u64 << 53) as f64)
    }

    /// Uniform in [-amplitude, amplitude).
    pub fn symmetric(&mut self, amplitude: f64) -> f64 {
        (self.unit() * 2.0 - 1.0) * amplitude
    }

    /// Uniform index in [0, n).
    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next_u64() % n
        }
    }
}

/// Seed for repetition `rep` of an experiment with base seed `base`.
pub fn repetition_seed(base: u64, rep: u32) -> u64 {
    let mut rng = Rng::stream(base, &format!("repetition.{rep}"));
    rng.next_u64() >> 16
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_sequence_is_stable() {
        // Pinned values: if these change, every recorded run changes.
        let mut rng = Rng::new(42);
        assert_eq!(rng.next_u64(), 13679457532755275413);
        assert_eq!(rng.next_u64(), 2949826092126892291);
    }

    #[test]
    fn streams_are_independent_and_reproducible() {
        let mut a = Rng::stream(7, "sensor.agent_01");
        let mut b = Rng::stream(7, "sensor.agent_02");
        let mut a2 = Rng::stream(7, "sensor.agent_01");
        let first = a.next_u64();
        assert_eq!(first, a2.next_u64());
        assert_ne!(first, b.next_u64());
    }

    #[test]
    fn unit_is_in_range() {
        let mut rng = Rng::new(1);
        for _ in 0..10_000 {
            let value = rng.unit();
            assert!((0.0..1.0).contains(&value));
        }
        assert_eq!(rng.below(0), 0);
    }

    #[test]
    fn repetition_seeds_differ() {
        assert_ne!(repetition_seed(42, 0), repetition_seed(42, 1));
        assert_eq!(repetition_seed(42, 3), repetition_seed(42, 3));
    }
}
