//! Ground truth from hidden Horizon singleplayer runs (nightly-2026-11-19-f00125f, seed 42):
//! `/locate structure …` chat output and `level_find_nearest_structure` answers. Every position
//! the game reported must be a seed candidate of the right set, and every stronghold must be
//! within the documented error of its ring guess. (Recorded 2026-11-19 by the atlas-probe
//! scratch mod; see INTEGRATION.md "Evidence".)

use atlas_explore::stronghold::{RING_ERROR_CHUNKS, vanilla_rings};
use atlas_explore::structures::{Placement, SetId, cell_of, is_placement_chunk, nearest_candidate};

const SEED: i64 = 42;

/// (set, block x, block z) as `/locate` printed it.
const FOUND: &[(SetId, i32, i32)] = &[
    // level_find_nearest_structure from (0, 64, 0), radius 100.
    (SetId::Villages, 656, -304),
    (SetId::PillagerOutposts, 1376, -736),
    (SetId::DesertPyramids, -1376, -1760),
    (SetId::JungleTemples, -1360, 2880),
    (SetId::SwampHuts, -272, 352),
    (SetId::Igloos, 4896, -416),
    (SetId::OceanMonuments, 784, -176),
    (SetId::WoodlandMansions, -4608, 6976),
    (SetId::OceanRuins, 48, 160),
    (SetId::Shipwrecks, 192, 448),
    (SetId::RuinedPortals, 256, 352),
    (SetId::AncientCities, 816, -752),
    (SetId::TrailRuins, 32, -544),
    (SetId::TrialChambers, 272, 112),
    (SetId::AbandonedCamps, -224, 336),
    (SetId::BuriedTreasures, -615, -215),
    (SetId::Mineshafts, 64, 0),
    // `/locate structure` typed as a command from the spawn point (two runs).
    (SetId::Villages, 848, -880),
    (SetId::Villages, 144, -1584),
    (SetId::OceanMonuments, -288, -1408),
    (SetId::RuinedPortals, -304, 176),
    (SetId::RuinedPortals, -304, -608),
    (SetId::TrialChambers, -480, 96),
    (SetId::TrialChambers, -512, -256),
    (SetId::AbandonedCamps, -1120, 400),
    (SetId::Shipwrecks, -736, 448),
    (SetId::Shipwrecks, -1104, -112),
    (SetId::Igloos, 3328, -4880),
];

#[test]
fn every_located_structure_is_a_seed_candidate() {
    for &(set, x, z) in FOUND {
        let o = set.def().locate_offset;
        let (cx, cz) = ((x - o[0]) >> 4, (z - o[2]) >> 4);
        assert!(
            is_placement_chunk(SEED, set, cx, cz),
            "{set:?} at {x},{z} (chunk {cx},{cz})"
        );
    }
}

#[test]
fn located_structures_are_not_candidates_of_a_neighbouring_chunk_by_chance() {
    // Control: shifting each position by one chunk must (almost always) miss.
    let hits = FOUND
        .iter()
        .filter(|&&(set, x, z)| {
            let o = set.def().locate_offset;
            is_placement_chunk(SEED, set, ((x - o[0]) >> 4) + 1, (z - o[2]) >> 4)
        })
        .count();
    assert!(hits <= 1, "{hits}");
}

#[test]
fn locate_ring_is_never_before_the_first_seed_candidate_ring() {
    // `/locate` (findNearestMapStructure) walks rings of placement cells around the origin and
    // stops at the first ring with a start that passed the biome check. The first ring with a
    // seed candidate (no biome check) can therefore not come after it.
    for &(set, x, z) in &FOUND[..17] {
        let Placement::RandomSpread { spacing, .. } = set.def().placement else {
            unreachable!()
        };
        let ring = |chunk: [i32; 2]| {
            let [gx, gz] = cell_of(chunk, spacing);
            let [ox, oz] = cell_of([0, 0], spacing);
            (gx - ox).abs().max((gz - oz).abs())
        };
        let c = nearest_candidate(SEED, set, [0, 64, 0], 100).unwrap();
        let o = set.def().locate_offset;
        assert!(
            ring(c.chunk) <= ring([(x - o[0]) >> 4, (z - o[2]) >> 4]),
            "{set:?}"
        );
    }
}

/// The server's exact stronghold starts (radius-0 lookups from each ring guess), indices 0-11.
const STRONGHOLDS: &[[i32; 2]] = &[
    [-400, -2256],
    [1712, 768],
    [-1936, 1536],
    [5184, 208],
    [2720, 4992],
    [-2720, 4624],
    [-4608, -80],
    [-2272, -4176],
    [2352, -4000],
    [320, -8176],
    [5200, -6736],
    [7792, -2144],
];

#[test]
fn strongholds_are_within_the_ring_error() {
    let rings = vanilla_rings(SEED);
    for (g, exact) in rings.iter().zip(STRONGHOLDS) {
        let (cx, cz) = (exact[0] >> 4, exact[1] >> 4);
        let off = (cx - g.chunk[0]).abs().max((cz - g.chunk[1]).abs());
        assert!(
            off <= RING_ERROR_CHUNKS,
            "stronghold {} off by {off}",
            g.index
        );
    }
    // `/locate structure minecraft:stronghold` from spawn printed [1712, ~, 768].
    assert!(STRONGHOLDS.contains(&[1712, 768]));
}

#[test]
fn nether_and_end_locates_are_seed_candidates_with_the_right_pick() {
    use atlas_explore::structures::first_pick;
    // `/locate` in the Nether from (0.5, 100, 0.5) and in the End from (1600.5, 100, 0.5).
    let fortress = [0 >> 4, 0 >> 4];
    let bastion = [-336 >> 4, 752 >> 4];
    assert!(is_placement_chunk(
        SEED,
        SetId::NetherComplexes,
        fortress[0],
        fortress[1]
    ));
    assert!(is_placement_chunk(
        SEED,
        SetId::NetherComplexes,
        bastion[0],
        bastion[1]
    ));
    assert_eq!(
        first_pick(SEED, SetId::NetherComplexes, fortress),
        "minecraft:fortress"
    );
    assert_eq!(
        first_pick(SEED, SetId::NetherComplexes, bastion),
        "minecraft:bastion_remnant"
    );
    assert!(is_placement_chunk(SEED, SetId::NetherFossils, -192 >> 4, 0));
    assert!(is_placement_chunk(
        SEED,
        SetId::EndCities,
        1056 >> 4,
        -608 >> 4
    ));
}

#[test]
fn slime_chunks_match_the_server_count() {
    // The probe compared all 81×81 chunks of -40..=40 plus four far chunks with the server's
    // `chunk_is_slime_chunk`: 0 mismatches, 651 slime chunks in total.
    use atlas_explore::is_slime_chunk;
    let mut n = 0;
    for x in -40..=40 {
        for z in -40..=40 {
            n += is_slime_chunk(SEED, x, z) as u32;
        }
    }
    for (x, z) in [
        (1_000_000, -1_000_000),
        (-1_874_999, 1_874_999),
        (123_456, 654_321),
        (-77_777, -99_999),
    ] {
        n += is_slime_chunk(SEED, x, z) as u32;
    }
    assert_eq!(n, 651);
}
