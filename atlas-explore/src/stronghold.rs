//! Strongholds (`minecraft:concentric_rings`, distance 32, spread 3, count 128): approximate.
//!
//! Port of Horizon's `ring_searches` (crates/horizon-world/src/worldgen/structure/stronghold.rs),
//! i.e. vanilla `ChunkGeneratorStructureState.generateRingPositions` up to the biome search. The
//! ring chunk computed here is exact; the stronghold then moves to a random
//! `#minecraft:stronghold_biased_to` biome within 112 blocks of that chunk's centre
//! (`BiomeSource.findBiomeHorizontal`), which needs the biome source. So the real start chunk is
//! within [`RING_ERROR_CHUNKS`] chunks of [`RingGuess::chunk`] (on each axis), and equal to it
//! when no preferred biome is nearby. In singleplayer, `level_find_nearest_structure` from the
//! guess gives the exact position (see [`crate::verify`]).

use alloc::vec::Vec;

use crate::java_random::JavaRandom;

/// Max distance, per axis in chunks, between a ring guess and the real start chunk:
/// the 112-block search square around the chunk centre (+8), rounded up.
pub const RING_ERROR_CHUNKS: i32 = (112 + 8 + 15) / 16;

/// One stronghold's ring position before the biome search.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct RingGuess {
    /// 0-based stronghold index in generation order.
    pub index: u8,
    /// 0-based ring (3 strongholds on ring 0, 6 on ring 1, 10, 15, 21, 28, 36, 9).
    pub ring: u8,
    /// The ring chunk; the real start is within [`RING_ERROR_CHUNKS`] of it.
    pub chunk: [i32; 2],
}

impl RingGuess {
    /// The block `/locate` would show if the stronghold stayed on its ring chunk (chunk corner).
    pub fn pos(&self) -> [i32; 3] {
        [self.chunk[0] * 16, 0, self.chunk[1] * 16]
    }
}

/// The 128 vanilla ring guesses for `seed`.
pub fn vanilla_rings(seed: i64) -> Vec<RingGuess> {
    rings(seed, 32, 3, 128)
}

/// `generateRingPositions` without the biome search.
pub fn rings(seed: i64, distance: i32, spread: i32, count: i32) -> Vec<RingGuess> {
    use core::f64::consts::PI;
    let mut random = JavaRandom::new(seed);
    let mut angle = random.next_double() * PI * 2.0;
    let (mut in_circle, mut circle, mut spread) = (0, 0, spread);
    let mut out = Vec::with_capacity(count.max(0) as usize);
    for i in 0..count {
        let dist = f64::from(4 * distance + distance * circle * 6)
            + (random.next_double() - 0.5) * (f64::from(distance) * 2.5);
        let (s, c) = sin_cos(angle);
        out.push(RingGuess {
            index: i as u8,
            ring: circle as u8,
            chunk: [round(c * dist), round(s * dist)],
        });
        // The biome search's own random is forked off here (one nextLong).
        let _ = random.fork();
        angle += PI * 2.0 / f64::from(spread);
        in_circle += 1;
        if in_circle == spread {
            circle += 1;
            in_circle = 0;
            spread += 2 * spread / (circle + 1);
            spread = spread.min(count - i);
            angle += random.next_double() * PI * 2.0;
        }
    }
    out
}

/// `Math.round`-like `floor(v + 0.5)` to i32 (no_std: no `f64::floor`).
fn round(v: f64) -> i32 {
    let v = v + 0.5;
    let t = v as i64;
    (if (t as f64) > v { t - 1 } else { t }) as i32
}

/// sin and cos to ~1e-15 without libm: reduce to [-π/4, π/4] and use Taylor series.
/// (Horizon uses fdlibm's StrictMath; a last-bit difference cannot move a chunk that is only
/// a guess anyway, and agrees with it on every ring of the tested seeds.)
fn sin_cos(x: f64) -> (f64, f64) {
    use core::f64::consts::FRAC_PI_2;
    let q = round(x / FRAC_PI_2);
    // Two-part π/2 for a more accurate reduction.
    const PIO2_HI: f64 = FRAC_PI_2;
    const PIO2_LO: f64 = 6.123_233_995_736_766e-17;
    let y = (x - f64::from(q) * PIO2_HI) - f64::from(q) * PIO2_LO;
    let y2 = y * y;
    let mut s = 0.0;
    let mut c = 0.0;
    // Horner, highest term first.
    let mut k = 21;
    while k >= 1 {
        // sin: y - y^3/3! + ...  (odd k), cos: 1 - y^2/2! + ... (even k)
        if k % 2 == 1 {
            s = 1.0 - y2 * s / f64::from((k + 1) * (k + 2));
        } else {
            c = 1.0 - y2 * c / f64::from((k + 1) * (k + 2));
        }
        k -= 1;
    }
    let sin = y * s;
    let cos = 1.0 - y2 * c / 2.0;
    match q.rem_euclid(4) {
        0 => (sin, cos),
        1 => (cos, -sin),
        2 => (-sin, -cos),
        _ => (-cos, sin),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trig_is_accurate() {
        let mut x = -40.0;
        while x < 40.0 {
            let (s, c) = sin_cos(x);
            assert!((s - x.sin()).abs() < 1e-13, "sin {x}");
            assert!((c - x.cos()).abs() < 1e-13, "cos {x}");
            x += 0.0137;
        }
    }

    #[test]
    fn ring_layout_is_vanilla() {
        let r = vanilla_rings(42);
        assert_eq!(r.len(), 128);
        let per_ring: Vec<usize> = (0..8)
            .map(|k| r.iter().filter(|g| g.ring == k).count())
            .collect();
        assert_eq!(per_ring, [3, 6, 10, 15, 21, 28, 36, 9]);
        // Ring k's distance is (4 + 6k) * 32 chunks ± 40.
        for g in &r {
            let d = f64::from(g.chunk[0]).hypot(f64::from(g.chunk[1]));
            let mid = f64::from((4 + 6 * i32::from(g.ring)) * 32);
            assert!((d - mid).abs() <= 41.0, "{g:?} {d}");
        }
    }

    #[test]
    fn round_matches_floor_half_up() {
        assert_eq!(round(1.5), 2);
        assert_eq!(round(-1.5), -1);
        assert_eq!(round(-1.6), -2);
        assert_eq!(round(0.49), 0);
    }
}
