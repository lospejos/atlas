//! Slime chunks: exact for any Java/Horizon world once the seed is known.
//!
//! Same formula as Horizon's `slime_chunk_random(x, z, seed, 987_234_911).next_int_bound(10) == 0`
//! (crates/horizon-core/src/random/worldgen.rs, used by crates/horizon-client/src/mobs/spawn_rules.rs),
//! which is vanilla's `WorldgenRandom.seedSlimeChunk`. It does not depend on biome or dimension, but
//! slimes only use it in the Overworld (below y 40); in other dimensions draw nothing.

use crate::java_random::JavaRandom;

/// Vanilla's slime-chunk salt.
pub const SLIME_SALT: i64 = 987_234_911;

/// Whether chunk (`chunk_x`, `chunk_z`) is a slime chunk in a world with `seed`.
pub fn is_slime_chunk(seed: i64, chunk_x: i32, chunk_z: i32) -> bool {
    let (x, z) = (chunk_x, chunk_z);
    // Java int arithmetic (wrapping) for each product, then long sums, as vanilla.
    let mixed = seed
        .wrapping_add(i64::from(x.wrapping_mul(x).wrapping_mul(4_987_142)))
        .wrapping_add(i64::from(x.wrapping_mul(5_947_611)))
        .wrapping_add(i64::from(z.wrapping_mul(z)).wrapping_mul(4_392_871))
        .wrapping_add(i64::from(z.wrapping_mul(389_711)))
        ^ SLIME_SALT;
    JavaRandom::new(mixed).next_int_bound(10) == 0
}

/// Calls `out(chunk_x, chunk_z)` for every slime chunk in the inclusive chunk rectangle.
/// About one chunk in ten is a slime chunk. Cost: one LCG seed + one or two steps per chunk,
/// so a 64×64-chunk view (4096 chunks) is well under a millisecond; cache per region anyway.
pub fn slime_chunks_in(
    seed: i64,
    min_chunk: [i32; 2],
    max_chunk: [i32; 2],
    mut out: impl FnMut(i32, i32),
) {
    for z in min_chunk[1]..=max_chunk[1] {
        for x in min_chunk[0]..=max_chunk[0] {
            if is_slime_chunk(seed, x, z) {
                out(x, z);
            }
        }
    }
}

/// One 32×32-chunk region (a `.mca` region, 512×512 blocks) of slime flags as a bitmap:
/// bit `(z - rz*32) * 32 + (x - rx*32)` of the 1024-bit array, i.e. `rows[z_in_region] >> x_in_region & 1`.
/// Handy as the per-region cache entry for the map overlay.
pub fn slime_region_bitmap(seed: i64, region_x: i32, region_z: i32) -> [u32; 32] {
    let mut rows = [0u32; 32];
    for (dz, row) in rows.iter_mut().enumerate() {
        for dx in 0..32 {
            if is_slime_chunk(seed, region_x * 32 + dx, region_z * 32 + dz as i32) {
                *row |= 1 << dx;
            }
        }
    }
    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roughly_one_in_ten() {
        let mut n = 0;
        slime_chunks_in(42, [-50, -50], [49, 49], |_, _| n += 1);
        // 10_000 chunks: expect ~1000.
        assert!((900..1100).contains(&n), "{n}");
    }

    #[test]
    fn bitmap_matches_point_queries() {
        let rows = slime_region_bitmap(12345, -1, 2);
        for dz in 0..32 {
            for dx in 0..32 {
                let want = is_slime_chunk(12345, -32 + dx, 64 + dz);
                assert_eq!(rows[dz as usize] >> dx & 1 == 1, want);
            }
        }
    }
}
