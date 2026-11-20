//! Structure placement without biomes: where each vanilla structure set *may* start.
//!
//! Constants are vanilla 26.3's `data/minecraft/worldgen/structure_set/*.json` (read from the
//! 26.3 client jar). The algorithms are Horizon's `StructureRegistry::potential_chunk`,
//! `structure_chunk` and `reduce` (crates/horizon-world/src/worldgen/structure/start.rs), which
//! are vanilla's `RandomSpreadStructurePlacement.getPotentialStructureChunk`,
//! `StructurePlacement.isStructureChunk` (frequency reduction + exclusion zone) and
//! `ChunkGenerator.tryGenerateStructure`'s weighted pick.
//!
//! What is *not* checked here: the structure's biome (and, for some, terrain height / water).
//! So a [`Candidate`] is "a structure of this set starts here if the biome allows it". See
//! [`Certainty`] for which sets that matters for.

use alloc::vec::Vec;

use crate::java_random::JavaRandom;

/// The three vanilla dimensions (custom dimensions: use the server path).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Dimension {
    Overworld,
    Nether,
    End,
}

impl Dimension {
    /// The level key the game uses (`minecraft:overworld`, …).
    pub const fn key(self) -> &'static str {
        match self {
            Dimension::Overworld => "minecraft:overworld",
            Dimension::Nether => "minecraft:the_nether",
            Dimension::End => "minecraft:the_end",
        }
    }

    /// Inverse of [`Dimension::key`].
    pub fn from_key(key: &str) -> Option<Self> {
        match key {
            "minecraft:overworld" => Some(Dimension::Overworld),
            "minecraft:the_nether" => Some(Dimension::Nether),
            "minecraft:the_end" => Some(Dimension::End),
            _ => None,
        }
    }
}

/// How far a seed-only answer can be trusted.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Certainty {
    /// The set places a structure at every candidate in practice (every biome of the dimension
    /// has a variant, no terrain check that fails). Position exact.
    Exact,
    /// Position exact, but the structure only generates there if the biome (and for some, the
    /// terrain) fits. Show as "possible".
    NeedsBiome,
    /// The position itself is only approximate (strongholds: within ~8 chunks).
    Approximate,
}

/// `spread_type` of a random spread.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Spread {
    Linear,
    Triangular,
}

/// `frequency_reduction_method`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reduction {
    Default,
    LegacyType1,
    LegacyType2,
    LegacyType3,
}

/// How a set picks its start chunks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Placement {
    /// `minecraft:random_spread`.
    RandomSpread {
        spacing: i32,
        separation: i32,
        spread: Spread,
    },
    /// `minecraft:concentric_rings` (strongholds): see [`crate::stronghold`].
    ConcentricRings {
        distance: i32,
        spread: i32,
        count: i32,
    },
}

/// The vanilla structure sets, in a fixed order (index into [`SETS`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
#[repr(u8)]
pub enum SetId {
    Villages,
    PillagerOutposts,
    DesertPyramids,
    JungleTemples,
    SwampHuts,
    Igloos,
    OceanMonuments,
    WoodlandMansions,
    OceanRuins,
    Shipwrecks,
    RuinedPortals,
    AncientCities,
    TrailRuins,
    TrialChambers,
    AbandonedCamps,
    BuriedTreasures,
    Mineshafts,
    Strongholds,
    NetherComplexes,
    NetherFossils,
    EndCities,
}

impl SetId {
    /// Every set, in [`SETS`] order.
    pub const ALL: [SetId; 21] = [
        SetId::Villages,
        SetId::PillagerOutposts,
        SetId::DesertPyramids,
        SetId::JungleTemples,
        SetId::SwampHuts,
        SetId::Igloos,
        SetId::OceanMonuments,
        SetId::WoodlandMansions,
        SetId::OceanRuins,
        SetId::Shipwrecks,
        SetId::RuinedPortals,
        SetId::AncientCities,
        SetId::TrailRuins,
        SetId::TrialChambers,
        SetId::AbandonedCamps,
        SetId::BuriedTreasures,
        SetId::Mineshafts,
        SetId::Strongholds,
        SetId::NetherComplexes,
        SetId::NetherFossils,
        SetId::EndCities,
    ];

    /// The set's definition.
    pub fn def(self) -> &'static StructureSet {
        &SETS[self as usize]
    }

    /// Look a set up by its registry id (`minecraft:villages`).
    pub fn from_id(id: &str) -> Option<SetId> {
        SetId::ALL.into_iter().find(|s| s.def().id == id)
    }
}

/// One structure set's placement, as in its JSON, plus what the map needs to draw and verify it.
#[derive(Debug)]
pub struct StructureSet {
    /// Registry id of the set.
    pub id: &'static str,
    /// Short English label for a legend.
    pub label: &'static str,
    /// Dimensions whose generator can place it (vanilla biome sources).
    pub dimensions: &'static [Dimension],
    pub placement: Placement,
    pub salt: i32,
    /// `frequency` (1.0 = no reduction).
    pub frequency: f32,
    pub reduction: Reduction,
    /// `exclusion_zone`: no start within `chunk_count` chunks of a start chunk of `other_set`.
    pub exclusion: Option<(SetId, i32)>,
    /// `locate_offset`, added to the start chunk's corner for `/locate`'s position.
    pub locate_offset: [i32; 3],
    /// `(structure id, weight)` entries.
    pub structures: &'static [(&'static str, i32)],
    /// What to pass to `level_find_nearest_structure` to check one candidate, in order (an id or
    /// a `#tag`); the first that answers wins.
    pub lookup: &'static [&'static str],
    /// How far a seed-only candidate can be trusted (without the server check).
    pub certainty: Certainty,
}

const OW: &[Dimension] = &[Dimension::Overworld];

const fn rs(spacing: i32, separation: i32) -> Placement {
    Placement::RandomSpread {
        spacing,
        separation,
        spread: Spread::Linear,
    }
}
const fn tri(spacing: i32, separation: i32) -> Placement {
    Placement::RandomSpread {
        spacing,
        separation,
        spread: Spread::Triangular,
    }
}

#[allow(clippy::too_many_arguments)]
const fn base(
    id: &'static str,
    label: &'static str,
    dimensions: &'static [Dimension],
    placement: Placement,
    salt: i32,
    structures: &'static [(&'static str, i32)],
    lookup: &'static [&'static str],
    certainty: Certainty,
) -> StructureSet {
    StructureSet {
        id,
        label,
        dimensions,
        placement,
        salt,
        frequency: 1.0,
        reduction: Reduction::Default,
        exclusion: None,
        locate_offset: [0, 0, 0],
        structures,
        lookup,
        certainty,
    }
}

macro_rules! set {
    ($id:literal, $label:literal, $dims:expr, $placement:expr, $salt:expr, [$($s:literal = $w:literal),+ $(,)?], [$($l:literal),+], $cert:ident $(, $field:ident : $value:expr)* $(,)?) => {
        StructureSet {
            $($field: $value,)*
            ..base($id, $label, $dims, $placement, $salt, &[$(($s, $w)),+], &[$($l),+], Certainty::$cert)
        }
    };
}

/// Vanilla 26.3's structure sets (`data/minecraft/worldgen/structure_set`), indexed by [`SetId`].
pub static SETS: [StructureSet; 21] = [
    set!(
        "minecraft:villages",
        "Village",
        OW,
        rs(34, 8),
        10_387_312,
        [
            "minecraft:village_plains" = 1,
            "minecraft:village_desert" = 1,
            "minecraft:village_savanna" = 1,
            "minecraft:village_snowy" = 1,
            "minecraft:village_taiga" = 1
        ],
        ["#minecraft:village"],
        NeedsBiome
    ),
    set!("minecraft:pillager_outposts", "Pillager Outpost", OW, rs(32, 8), 165_745_296,
        ["minecraft:pillager_outpost" = 1], ["minecraft:pillager_outpost"], NeedsBiome,
        frequency: 0.2, reduction: Reduction::LegacyType1, exclusion: Some((SetId::Villages, 10))),
    set!(
        "minecraft:desert_pyramids",
        "Desert Pyramid",
        OW,
        rs(32, 8),
        14_357_617,
        ["minecraft:desert_pyramid" = 1],
        ["minecraft:desert_pyramid"],
        NeedsBiome
    ),
    set!(
        "minecraft:jungle_temples",
        "Jungle Temple",
        OW,
        rs(32, 8),
        14_357_619,
        ["minecraft:jungle_pyramid" = 1],
        ["minecraft:jungle_pyramid"],
        NeedsBiome
    ),
    set!(
        "minecraft:swamp_huts",
        "Witch Hut",
        OW,
        rs(32, 8),
        14_357_620,
        ["minecraft:swamp_hut" = 1],
        ["minecraft:swamp_hut"],
        NeedsBiome
    ),
    set!(
        "minecraft:igloos",
        "Igloo",
        OW,
        rs(32, 8),
        14_357_618,
        ["minecraft:igloo" = 1],
        ["minecraft:igloo"],
        NeedsBiome
    ),
    set!(
        "minecraft:ocean_monuments",
        "Ocean Monument",
        OW,
        tri(32, 5),
        10_387_313,
        ["minecraft:monument" = 1],
        ["minecraft:monument"],
        NeedsBiome
    ),
    set!(
        "minecraft:woodland_mansions",
        "Woodland Mansion",
        OW,
        tri(80, 20),
        10_387_319,
        ["minecraft:mansion" = 1],
        ["minecraft:mansion"],
        NeedsBiome
    ),
    set!(
        "minecraft:ocean_ruins",
        "Ocean Ruin",
        OW,
        rs(20, 8),
        14_357_621,
        [
            "minecraft:ocean_ruin_cold" = 1,
            "minecraft:ocean_ruin_warm" = 1
        ],
        ["#minecraft:ocean_ruin"],
        NeedsBiome
    ),
    set!(
        "minecraft:shipwrecks",
        "Shipwreck",
        OW,
        rs(24, 4),
        165_745_295,
        ["minecraft:shipwreck" = 1, "minecraft:shipwreck_beached" = 1],
        ["#minecraft:shipwreck"],
        NeedsBiome
    ),
    set!(
        "minecraft:ruined_portals",
        "Ruined Portal",
        &[Dimension::Overworld, Dimension::Nether],
        rs(40, 15),
        34_222_645,
        [
            "minecraft:ruined_portal" = 1,
            "minecraft:ruined_portal_desert" = 1,
            "minecraft:ruined_portal_jungle" = 1,
            "minecraft:ruined_portal_swamp" = 1,
            "minecraft:ruined_portal_mountain" = 1,
            "minecraft:ruined_portal_ocean" = 1,
            "minecraft:ruined_portal_nether" = 1
        ],
        ["#minecraft:ruined_portal"],
        Exact
    ),
    set!(
        "minecraft:ancient_cities",
        "Ancient City",
        OW,
        rs(24, 8),
        20_083_232,
        ["minecraft:ancient_city" = 1],
        ["minecraft:ancient_city"],
        NeedsBiome
    ),
    set!(
        "minecraft:trail_ruins",
        "Trail Ruins",
        OW,
        rs(34, 8),
        83_469_867,
        ["minecraft:trail_ruins" = 1],
        ["minecraft:trail_ruins"],
        NeedsBiome
    ),
    set!(
        "minecraft:trial_chambers",
        "Trial Chambers",
        OW,
        rs(34, 12),
        94_251_327,
        ["minecraft:trial_chambers" = 1],
        ["minecraft:trial_chambers"],
        NeedsBiome
    ),
    set!(
        "minecraft:abandoned_camp",
        "Abandoned Camp",
        OW,
        rs(37, 8),
        91_231_127,
        [
            "minecraft:abandoned_camp_bamboo_jungle" = 1,
            "minecraft:abandoned_camp_birch_forest" = 1,
            "minecraft:abandoned_camp_cherry_grove" = 1,
            "minecraft:abandoned_camp_dappled_forest" = 1,
            "minecraft:abandoned_camp_flower_forest" = 1,
            "minecraft:abandoned_camp_forest" = 1,
            "minecraft:abandoned_camp_meadow" = 1,
            "minecraft:abandoned_camp_old_growth_pine_taiga" = 1,
            "minecraft:abandoned_camp_old_growth_birch_forest" = 1,
            "minecraft:abandoned_camp_old_growth_spruce_taiga" = 1,
            "minecraft:abandoned_camp_pale_garden" = 1,
            "minecraft:abandoned_camp_savanna" = 1,
            "minecraft:abandoned_camp_snowy_taiga" = 1,
            "minecraft:abandoned_camp_sparse_jungle" = 1,
            "minecraft:abandoned_camp_swamp" = 1,
            "minecraft:abandoned_camp_taiga" = 1,
            "minecraft:abandoned_camp_windswept_forest" = 1,
            "minecraft:abandoned_camp_wooded_badlands" = 1
        ],
        ["#minecraft:abandoned_camp"],
        NeedsBiome
    ),
    set!("minecraft:buried_treasures", "Buried Treasure", OW, rs(1, 0), 0,
        ["minecraft:buried_treasure" = 1], ["minecraft:buried_treasure"], NeedsBiome,
        frequency: 0.01, reduction: Reduction::LegacyType2, locate_offset: [9, 0, 9]),
    set!("minecraft:mineshafts", "Mineshaft", OW, rs(1, 0), 0,
        ["minecraft:mineshaft" = 1, "minecraft:mineshaft_mesa" = 1], ["#minecraft:mineshaft"], NeedsBiome,
        frequency: 0.004, reduction: Reduction::LegacyType3),
    set!(
        "minecraft:strongholds",
        "Stronghold",
        OW,
        Placement::ConcentricRings {
            distance: 32,
            spread: 3,
            count: 128
        },
        0,
        ["minecraft:stronghold" = 1],
        ["minecraft:stronghold"],
        Approximate
    ),
    set!(
        "minecraft:nether_complexes",
        "Fortress / Bastion",
        &[Dimension::Nether],
        rs(27, 4),
        30_084_232,
        ["minecraft:fortress" = 2, "minecraft:bastion_remnant" = 3],
        ["minecraft:fortress", "minecraft:bastion_remnant"],
        Exact
    ),
    set!(
        "minecraft:nether_fossils",
        "Nether Fossil",
        &[Dimension::Nether],
        rs(2, 1),
        14_357_921,
        ["minecraft:nether_fossil" = 1],
        ["minecraft:nether_fossil"],
        NeedsBiome
    ),
    set!(
        "minecraft:end_cities",
        "End City",
        &[Dimension::End],
        tri(20, 11),
        10_387_313,
        ["minecraft:end_city" = 1],
        ["minecraft:end_city"],
        NeedsBiome
    ),
];

/// A place where a structure set would start a structure, before biome checks.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Candidate {
    pub set: SetId,
    /// The start chunk.
    pub chunk: [i32; 2],
    /// The block position `/locate` reports for it (chunk corner + `locate_offset`, y from the set).
    pub pos: [i32; 3],
    /// The structure vanilla tries first here (the set's weighted pick). For a one-structure set
    /// that is the structure. For villages / ruined portals / camps it is meaningless without
    /// biomes (vanilla tries the others when it fails), so the map should show the set's label.
    /// For nether complexes it decides: a fortress pick is always a fortress; a bastion pick is a
    /// bastion unless the start is in basalt deltas, then a fortress.
    pub first_pick: &'static str,
    pub certainty: Certainty,
}

/// The random-spread cell (`spacing`-chunk square) a chunk lies in.
pub fn cell_of(chunk: [i32; 2], spacing: i32) -> [i32; 2] {
    [chunk[0].div_euclid(spacing), chunk[1].div_euclid(spacing)]
}

/// `RandomSpreadStructurePlacement.getPotentialStructureChunk` for cell (`cell_x`, `cell_z`):
/// the one chunk of that cell where the set may start. `None` for ring placements.
pub fn potential_chunk(seed: i64, set: SetId, cell_x: i32, cell_z: i32) -> Option<[i32; 2]> {
    let def = set.def();
    let Placement::RandomSpread {
        spacing,
        separation,
        spread,
    } = def.placement
    else {
        return None;
    };
    let mut random = JavaRandom::new(0);
    random.set_large_feature_with_salt(seed, cell_x, cell_z, def.salt);
    let limit = spacing - separation;
    let mut draw = || match spread {
        Spread::Linear => random.next_int_bound(limit),
        Spread::Triangular => (random.next_int_bound(limit) + random.next_int_bound(limit)) / 2,
    };
    let sx = draw();
    let sz = draw();
    Some([cell_x * spacing + sx, cell_z * spacing + sz])
}

/// `StructurePlacement.isStructureChunk` without biomes: the spread picks this chunk, the
/// frequency reduction keeps it and no exclusion zone removes it. False for ring placements
/// (use [`crate::stronghold`]).
pub fn is_placement_chunk(seed: i64, set: SetId, chunk_x: i32, chunk_z: i32) -> bool {
    let def = set.def();
    let Placement::RandomSpread { spacing, .. } = def.placement else {
        return false;
    };
    let [cx, cz] = cell_of([chunk_x, chunk_z], spacing);
    if potential_chunk(seed, set, cx, cz) != Some([chunk_x, chunk_z]) {
        return false;
    }
    if def.frequency < 1.0
        && !keeps(
            def.reduction,
            seed,
            def.salt,
            chunk_x,
            chunk_z,
            def.frequency,
        )
    {
        return false;
    }
    match def.exclusion {
        Some((other, range)) => !has_start_in_range(seed, other, chunk_x, chunk_z, range),
        None => true,
    }
}

/// `ChunkGeneratorStructureState.hasStructureChunkInRange`, without biomes: whether `set` has a
/// placement chunk within `range` chunks (Chebyshev) of (`x`, `z`).
pub fn has_start_in_range(seed: i64, set: SetId, x: i32, z: i32, range: i32) -> bool {
    let mut found = false;
    candidates_in(
        seed,
        set,
        [x - range, z - range],
        [x + range, z + range],
        |_| {
            found = true;
        },
    );
    found
}

/// `StructurePlacement.applyAdditionalChunkRestrictions`' frequency reduction: whether the chunk
/// survives (`frequency_reduction_method`, Horizon's `reduce`).
fn keeps(method: Reduction, seed: i64, salt: i32, x: i32, z: i32, probability: f32) -> bool {
    let mut random = JavaRandom::new(0);
    match method {
        Reduction::Default => {
            // Vanilla passes the salt as x, x as z and z as the salt here.
            random.set_large_feature_with_salt(seed, salt, x, z);
            random.next_float() < probability
        }
        Reduction::LegacyType1 => {
            let (cx, cz) = (x >> 4, z >> 4);
            random.set_seed(i64::from(cx ^ (cz << 4)) ^ seed);
            random.next_int();
            random.next_int_bound((1.0f32 / probability) as i32) == 0
        }
        Reduction::LegacyType2 => {
            random.set_large_feature_with_salt(seed, x, z, 10_387_320);
            random.next_float() < probability
        }
        Reduction::LegacyType3 => {
            random.set_large_feature_seed(seed, x, z);
            random.next_double() < f64::from(probability)
        }
    }
}

/// The order vanilla tries a set's structures at `chunk` (`ChunkGenerator.tryGenerateStructure`:
/// a weighted pick seeded by `setLargeFeatureSeed(seed, x, z)`, each failure removed and the pick
/// repeated). Calls `out` with each structure id, first try first.
pub fn try_order(seed: i64, set: SetId, chunk: [i32; 2], mut out: impl FnMut(&'static str)) {
    let entries = set.def().structures;
    if entries.len() == 1 {
        out(entries[0].0);
        return;
    }
    let mut options: Vec<(&'static str, i32)> = entries.to_vec();
    let mut random = JavaRandom::new(0);
    random.set_large_feature_seed(seed, chunk[0], chunk[1]);
    while !options.is_empty() {
        let total: i64 = options.iter().map(|o| i64::from(o.1)).sum();
        let mut choice = i64::from(random.next_int_bound(total as i32));
        let index = options
            .iter()
            .position(|o| {
                choice -= i64::from(o.1);
                choice < 0
            })
            .unwrap_or(0);
        out(options[index].0);
        options.remove(index);
    }
}

/// The first structure of [`try_order`].
pub fn first_pick(seed: i64, set: SetId, chunk: [i32; 2]) -> &'static str {
    let mut first = None;
    try_order(seed, set, chunk, |s| {
        if first.is_none() {
            first = Some(s);
        }
    });
    first.unwrap_or(set.def().structures[0].0)
}

fn candidate(seed: i64, set: SetId, chunk: [i32; 2]) -> Candidate {
    let def = set.def();
    let o = def.locate_offset;
    Candidate {
        set,
        chunk,
        pos: [chunk[0] * 16 + o[0], o[1], chunk[1] * 16 + o[2]],
        first_pick: first_pick(seed, set, chunk),
        certainty: def.certainty,
    }
}

/// Every placement chunk of `set` inside the inclusive chunk rectangle, without biome checks.
///
/// Cost is per spread cell (one LCG seed + 2-4 draws), so even a 1000×1000-chunk rectangle is
/// cheap for villages (~900 cells). The per-chunk sets (mineshafts, buried treasure: spacing 1)
/// and nether fossils (spacing 2) cost one check per chunk/cell: keep their rectangle to the view.
pub fn candidates_in(
    seed: i64,
    set: SetId,
    min_chunk: [i32; 2],
    max_chunk: [i32; 2],
    mut out: impl FnMut(Candidate),
) {
    let def = set.def();
    let Placement::RandomSpread { spacing, .. } = def.placement else {
        return;
    };
    let [gx0, gz0] = cell_of(min_chunk, spacing);
    let [gx1, gz1] = cell_of(max_chunk, spacing);
    for gz in gz0..=gz1 {
        for gx in gx0..=gx1 {
            let Some(chunk) = potential_chunk(seed, set, gx, gz) else {
                continue;
            };
            if chunk[0] < min_chunk[0]
                || chunk[0] > max_chunk[0]
                || chunk[1] < min_chunk[1]
                || chunk[1] > max_chunk[1]
            {
                continue;
            }
            if def.frequency < 1.0
                && !keeps(
                    def.reduction,
                    seed,
                    def.salt,
                    chunk[0],
                    chunk[1],
                    def.frequency,
                )
            {
                continue;
            }
            if let Some((other, range)) = def.exclusion
                && has_start_in_range(seed, other, chunk[0], chunk[1], range)
            {
                continue;
            }
            out(candidate(seed, set, chunk));
        }
    }
}

/// All random-spread candidates of every set placed in `dimension` within `radius` chunks
/// (square) of `center`, nearest first. `sets` filters (empty = all of the dimension's).
/// Strongholds are not included (see [`crate::stronghold`]).
pub fn candidates_near(
    seed: i64,
    dimension: Dimension,
    center: [i32; 2],
    radius: i32,
    sets: &[SetId],
) -> Vec<Candidate> {
    let mut out = Vec::new();
    for set in SetId::ALL {
        if !sets.is_empty() && !sets.contains(&set) {
            continue;
        }
        if !set.def().dimensions.contains(&dimension) {
            continue;
        }
        candidates_in(
            seed,
            set,
            [center[0] - radius, center[1] - radius],
            [center[0] + radius, center[1] + radius],
            |c| out.push(c),
        );
    }
    let d2 = |c: &Candidate| {
        let (dx, dz) = (
            i64::from(c.chunk[0] - center[0]),
            i64::from(c.chunk[1] - center[1]),
        );
        dx * dx + dz * dz
    };
    out.sort_by_key(|c| (d2(c), c.set, c.chunk));
    out
}

/// `/locate`'s search order (`ChunkGenerator.findNearestMapStructure`) over seed candidates only:
/// rings of cells around `from` (blocks), stopping at the first ring that has one; the nearest
/// candidate by locate position. Without biomes this is the nearest *possible* start, so it can
/// be nearer than what `/locate` reports, never farther within the searched rings.
pub fn nearest_candidate(
    seed: i64,
    set: SetId,
    from: [i32; 3],
    max_rings: i32,
) -> Option<Candidate> {
    let def = set.def();
    let Placement::RandomSpread { spacing, .. } = def.placement else {
        return None;
    };
    let (cx, cz) = (from[0] >> 4, from[2] >> 4);
    let dist = |p: [i32; 3]| {
        let (dx, dz) = (f64::from(p[0] - from[0]), f64::from(p[2] - from[2]));
        dx * dx + dz * dz
    };
    for r in 0..=max_rings {
        let mut best: Option<(f64, Candidate)> = None;
        for dx in -r..=r {
            for dz in -r..=r {
                if dx.abs() != r && dz.abs() != r {
                    continue;
                }
                let [gx, gz] = cell_of([cx + spacing * dx, cz + spacing * dz], spacing);
                let Some(chunk) = potential_chunk(seed, set, gx, gz) else {
                    continue;
                };
                if !is_placement_chunk(seed, set, chunk[0], chunk[1]) {
                    continue;
                }
                let c = candidate(seed, set, chunk);
                let d = dist(c.pos);
                if best.as_ref().is_none_or(|(b, _)| d < *b) {
                    best = Some((d, c));
                }
            }
        }
        if let Some((_, c)) = best {
            return Some(c);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_table_is_in_setid_order_and_valid() {
        for (i, set) in SetId::ALL.into_iter().enumerate() {
            assert_eq!(set as usize, i);
            let def = set.def();
            assert_eq!(SetId::from_id(def.id), Some(set));
            if let Placement::RandomSpread {
                spacing,
                separation,
                ..
            } = def.placement
            {
                assert!(spacing > separation, "{}", def.id);
            }
            assert!(!def.lookup.is_empty());
        }
    }

    #[test]
    fn potential_chunk_stays_inside_its_cell_and_separation() {
        for set in SetId::ALL {
            let Placement::RandomSpread {
                spacing,
                separation,
                ..
            } = set.def().placement
            else {
                continue;
            };
            for gx in -20..20 {
                for gz in -20..20 {
                    let [x, z] = potential_chunk(-4_172_144_997_902_289_642, set, gx, gz).unwrap();
                    let (ox, oz) = (x - gx * spacing, z - gz * spacing);
                    assert!((0..spacing - separation).contains(&ox));
                    assert!((0..spacing - separation).contains(&oz));
                }
            }
        }
    }

    #[test]
    fn candidates_in_equals_is_placement_chunk_scan() {
        let seed = 42;
        for set in [
            SetId::Villages,
            SetId::PillagerOutposts,
            SetId::WoodlandMansions,
            SetId::Mineshafts,
            SetId::BuriedTreasures,
        ] {
            let mut listed = Vec::new();
            candidates_in(seed, set, [-60, -45], [70, 55], |c| listed.push(c.chunk));
            let mut scanned = Vec::new();
            for z in -45..=55 {
                for x in -60..=70 {
                    if is_placement_chunk(seed, set, x, z) {
                        scanned.push([x, z]);
                    }
                }
            }
            listed.sort();
            scanned.sort();
            assert_eq!(listed, scanned, "{:?}", set);
        }
    }

    #[test]
    fn outposts_respect_village_exclusion_and_frequency() {
        let seed = 42;
        let mut kept = 0;
        let mut cells = 0;
        for gx in -40..40 {
            for gz in -40..40 {
                cells += 1;
                let chunk = potential_chunk(seed, SetId::PillagerOutposts, gx, gz).unwrap();
                if is_placement_chunk(seed, SetId::PillagerOutposts, chunk[0], chunk[1]) {
                    kept += 1;
                    assert!(!has_start_in_range(
                        seed,
                        SetId::Villages,
                        chunk[0],
                        chunk[1],
                        10
                    ));
                }
            }
        }
        // 1 in 5 survive the frequency, then some fall to villages: well under 20 %.
        assert!(kept * 100 / cells < 20 && kept > 0, "{kept}/{cells}");
    }

    #[test]
    fn mineshaft_and_treasure_rates() {
        let (mut m, mut t) = (0, 0);
        for x in -100..100 {
            for z in -100..100 {
                m += is_placement_chunk(7, SetId::Mineshafts, x, z) as i32;
                t += is_placement_chunk(7, SetId::BuriedTreasures, x, z) as i32;
            }
        }
        // 40_000 chunks: 0.4 % ≈ 160, 1 % ≈ 400.
        assert!((110..220).contains(&m), "{m}");
        assert!((320..480).contains(&t), "{t}");
    }

    #[test]
    fn nether_pick_is_two_to_three() {
        let mut fortress = 0;
        for gx in -50..50 {
            for gz in -50..50 {
                let c = potential_chunk(1, SetId::NetherComplexes, gx, gz).unwrap();
                if first_pick(1, SetId::NetherComplexes, c) == "minecraft:fortress" {
                    fortress += 1;
                }
            }
        }
        assert!((3600..4400).contains(&fortress), "{fortress}");
    }

    #[test]
    fn try_order_lists_every_structure_once() {
        let mut seen = Vec::new();
        try_order(99, SetId::Villages, [3, -8], |s| seen.push(s));
        seen.sort();
        let mut all: Vec<_> = SetId::Villages
            .def()
            .structures
            .iter()
            .map(|s| s.0)
            .collect();
        all.sort();
        assert_eq!(seen, all);
    }

    #[test]
    fn buried_treasure_locate_offset() {
        let c = candidate(1, SetId::BuriedTreasures, [-3, 5]);
        assert_eq!(c.pos, [-48 + 9, 0, 80 + 9]);
    }
}
