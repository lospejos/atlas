//! `java.util.Random` / Minecraft's `LegacyRandomSource` + `WorldgenRandom` seeding helpers.
//!
//! Bit-for-bit the same as Horizon's `horizon_core::random::LegacyRandom` and
//! `WorldgenRandom::set_large_feature_*` (crates/horizon-core/src/random{.rs,/worldgen.rs}).

const MULTIPLIER: u64 = 0x5_DEEC_E66D;
const ADDEND: u64 = 0xB;
const MASK: u64 = (1 << 48) - 1;

/// A 48-bit linear congruential generator, exactly `java.util.Random`.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct JavaRandom {
    state: u64,
}

impl JavaRandom {
    /// `new Random(seed)` (the seed is scrambled, as Java does).
    pub fn new(seed: i64) -> Self {
        let mut r = Self { state: 0 };
        r.set_seed(seed);
        r
    }

    /// `Random.setSeed`.
    pub fn set_seed(&mut self, seed: i64) {
        self.state = (seed as u64 ^ MULTIPLIER) & MASK;
    }

    /// `Random.next(bits)`.
    pub fn next_bits(&mut self, bits: u32) -> i32 {
        self.state = self.state.wrapping_mul(MULTIPLIER).wrapping_add(ADDEND) & MASK;
        (self.state >> (48 - bits)) as u32 as i32
    }

    /// `Random.nextInt()`.
    pub fn next_int(&mut self) -> i32 {
        self.next_bits(32)
    }

    /// `Random.nextInt(bound)`; `bound` must be positive (0 is returned otherwise, without a draw).
    pub fn next_int_bound(&mut self, bound: i32) -> i32 {
        if bound <= 0 {
            return 0;
        }
        if bound & (bound - 1) == 0 {
            return ((i64::from(bound) * i64::from(self.next_bits(31))) >> 31) as i32;
        }
        loop {
            let bits = self.next_bits(31);
            let value = bits % bound;
            if bits.wrapping_sub(value).wrapping_add(bound - 1) >= 0 {
                return value;
            }
        }
    }

    /// `Random.nextLong()`.
    pub fn next_long(&mut self) -> i64 {
        let high = i64::from(self.next_bits(32)) << 32;
        high.wrapping_add(i64::from(self.next_bits(32)))
    }

    /// `Random.nextFloat()`.
    pub fn next_float(&mut self) -> f32 {
        self.next_bits(24) as f32 * (1.0 / 16_777_216.0)
    }

    /// `Random.nextDouble()`.
    pub fn next_double(&mut self) -> f64 {
        let value = (i64::from(self.next_bits(26)) << 27) + i64::from(self.next_bits(27));
        value as f64 * (1.0 / 9_007_199_254_740_992.0)
    }

    /// `LegacyRandomSource.fork()`: a new generator seeded with this one's next long.
    pub fn fork(&mut self) -> Self {
        Self::new(self.next_long())
    }

    /// `WorldgenRandom.setLargeFeatureWithSalt(seed, x, z, salt)`.
    pub fn set_large_feature_with_salt(&mut self, seed: i64, x: i32, z: i32, salt: i32) {
        self.set_seed(
            i64::from(x)
                .wrapping_mul(341_873_128_712)
                .wrapping_add(i64::from(z).wrapping_mul(132_897_987_541))
                .wrapping_add(seed)
                .wrapping_add(i64::from(salt)),
        );
    }

    /// `WorldgenRandom.setLargeFeatureSeed(seed, x, z)`.
    pub fn set_large_feature_seed(&mut self, seed: i64, x: i32, z: i32) {
        self.set_seed(seed);
        let a = self.next_long();
        let b = self.next_long();
        self.set_seed(i64::from(x).wrapping_mul(a) ^ i64::from(z).wrapping_mul(b) ^ seed);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // Reference values printed by a JVM (`new java.util.Random(s)`), widely published.
    #[test]
    fn matches_java_util_random() {
        let mut r = JavaRandom::new(0);
        assert_eq!(r.next_int(), -1_155_484_576);
        let mut r = JavaRandom::new(42);
        assert_eq!(r.next_int(), -1_170_105_035);
        let mut r = JavaRandom::new(0);
        assert_eq!(r.next_long(), -4_962_768_465_676_381_896);
        let mut r = JavaRandom::new(0);
        assert_eq!(r.next_double(), 0.730_967_787_376_657);
        let mut r = JavaRandom::new(42);
        assert_eq!(r.next_int_bound(10), 0);
        assert_eq!(r.next_int_bound(10), 3);
    }

    // Horizon's oracle test (crates/horizon-core/src/random/worldgen/oracle_tests.rs):
    // new LegacyRandomSource(0): nextInt(1)=0, then nextInt(16)=13,3.
    #[test]
    fn matches_horizon_oracle() {
        let mut r = JavaRandom::new(0);
        assert_eq!(r.next_int_bound(1), 0);
        assert_eq!(r.next_int_bound(16), 13);
        assert_eq!(r.next_int_bound(16), 3);
    }

    #[test]
    fn non_positive_bound_does_not_panic() {
        let mut r = JavaRandom::new(1);
        assert_eq!(r.next_int_bound(0), 0);
        assert_eq!(r.next_int_bound(-5), 0);
    }
}
