//! Exact structures in singleplayer / on a LAN host: seed candidates checked one by one with the
//! server's own structure search.
//!
//! The trick: `level_find_nearest_structure(level, s, StructureSearch::new(origin, 0))` with
//! radius **0** searches only the placement cell holding `origin`'s chunk (Horizon's
//! `StructureGenerator::locate`: ring 0 is the origin cell alone; strongholds' rings are always
//! all checked). With `origin` = a seed candidate's locate position, the answer is "this exact
//! candidate is a real structure (and which variant)" or `None`. That is the game's full biome and
//! terrain check, for one chunk's starts, without loading or generating chunks.
//!
//! [`Verifier`] holds the plan: which candidates to check (nearest first), a per-second budget,
//! and the verdicts. It has no mod-API dependency: the mod's server half calls [`Verifier::take`],
//! runs the [`Lookup`] with the real host, and hands the answer to [`Verifier::record`].

use alloc::collections::{BTreeMap, BTreeSet, VecDeque};
use alloc::string::{String, ToString};
use alloc::vec::Vec;

use crate::stronghold::{RingGuess, vanilla_rings};
use crate::structures::{Candidate, Certainty, Dimension, SetId, candidates_in};

/// What a lookup is checking.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Target {
    /// A seed candidate of a random-spread set.
    Candidate { set: SetId, chunk: [i32; 2] },
    /// The stronghold with this ring index (0..128).
    Stronghold { index: u8 },
}

/// One server call to make. Run, in order, for each `structure` in `structures`:
/// `level.find_nearest_structure(host, structure, StructureSearch::new(origin, radius))` on the
/// server level of `dimension`, and stop at the first `Some`. Pass that (or `None`) to
/// [`Verifier::record`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Lookup {
    pub dimension: Dimension,
    pub target: Target,
    pub structures: &'static [&'static str],
    pub origin: [i32; 3],
    pub radius: i32,
}

/// What the server said about a target.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Verdict {
    /// The structure is there (`structure` is the exact id, e.g. `minecraft:village_taiga`).
    Present { structure: String, pos: [i32; 3] },
    /// Nothing starts at this candidate (biome / terrain said no).
    Absent,
}

/// How a marker should be drawn.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Status {
    /// Checked by the server: it is there.
    Confirmed,
    /// Seed says it may be there; not (yet) checked.
    Possible,
    /// Position is a guess (unchecked stronghold): within `stronghold::RING_ERROR_CHUNKS` chunks.
    Approximate,
}

/// One map marker.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Marker {
    pub set: SetId,
    /// Start chunk.
    pub chunk: [i32; 2],
    /// Block position (`/locate`'s).
    pub pos: [i32; 3],
    /// Exact structure id when confirmed, else the deterministic pick when the set has one
    /// structure / is the nether complex, else `None` (show the set's label).
    pub structure: Option<String>,
    pub status: Status,
}

/// Lookups per second and burst.
#[derive(Clone, Copy, Debug)]
pub struct Budget {
    /// Lookups allowed per second (20 server ticks).
    pub per_second: u32,
    /// Most lookups in one tick.
    pub per_tick: u32,
}

impl Default for Budget {
    /// 20 per second, at most 2 in one tick. Measured in Horizon (seed 42): 6 radius-0 lookups
    /// every tick (120/s) kept the integrated server at ~4-6 ms per tick; see INTEGRATION.md.
    fn default() -> Self {
        Budget {
            per_second: 20,
            per_tick: 2,
        }
    }
}

type Key = (Dimension, Target);

/// The plan and the cache. Keep one per world (seed); drop it when the world closes.
#[derive(Debug)]
pub struct Verifier {
    seed: i64,
    budget: Budget,
    verdicts: BTreeMap<Key, Verdict>,
    queue: VecDeque<Lookup>,
    queued: BTreeSet<Key>,
    rings: Vec<RingGuess>,
    /// Budget accounting: tokens in 1/20ths of a lookup, last tick seen, lookups this tick.
    tokens: u64,
    tick: u64,
    used_this_tick: u32,
    revision: u64,
}

impl Verifier {
    pub fn new(seed: i64, budget: Budget) -> Self {
        Verifier {
            seed,
            budget,
            verdicts: BTreeMap::new(),
            queue: VecDeque::new(),
            queued: BTreeSet::new(),
            rings: vanilla_rings(seed),
            tokens: u64::from(budget.per_second) * 20,
            tick: 0,
            used_this_tick: 0,
            revision: 0,
        }
    }

    pub fn seed(&self) -> i64 {
        self.seed
    }

    /// Queue every unchecked candidate of `sets` (empty = all sets of the dimension) within
    /// `radius` chunks of `center`, nearest first, ahead of older requests. Call it when the view
    /// moves to a new region (e.g. on crossing a 32-chunk region border); calling it again for
    /// the same area is cheap and queues nothing new. Strongholds are added with
    /// [`Verifier::request_strongholds`].
    pub fn request_area(
        &mut self,
        dimension: Dimension,
        center: [i32; 2],
        radius: i32,
        sets: &[SetId],
    ) {
        let mut found: Vec<Candidate> = Vec::new();
        for set in SetId::ALL {
            if (!sets.is_empty() && !sets.contains(&set))
                || !set.def().dimensions.contains(&dimension)
            {
                continue;
            }
            candidates_in(
                self.seed,
                set,
                [center[0] - radius, center[1] - radius],
                [center[0] + radius, center[1] + radius],
                |c| found.push(c),
            );
        }
        let d2 = |c: &[i32; 2]| {
            let (dx, dz) = (i64::from(c[0] - center[0]), i64::from(c[1] - center[1]));
            dx * dx + dz * dz
        };
        // Farthest first, each pushed to the front: the nearest ends up first.
        found.sort_by_key(|c| core::cmp::Reverse((d2(&c.chunk), c.set)));
        for c in found {
            let target = Target::Candidate {
                set: c.set,
                chunk: c.chunk,
            };
            let key = (dimension, target);
            if self.verdicts.contains_key(&key) {
                continue;
            }
            let lookup = Lookup {
                dimension,
                target,
                structures: c.set.def().lookup,
                origin: c.pos,
                radius: 0,
            };
            if self.queued.contains(&key) {
                // Move it to the front.
                self.queue.retain(|l| (l.dimension, l.target) != key);
            }
            self.queued.insert(key);
            self.queue.push_front(lookup);
        }
    }

    /// Queue the strongholds whose ring guess is within `radius` chunks of `center` (Overworld).
    /// The first stronghold lookup of a world makes the server place all 128 (it does that at
    /// world start anyway); later ones are cheap.
    pub fn request_strongholds(&mut self, center: [i32; 2], radius: i32) {
        for g in self.rings.clone() {
            if (g.chunk[0] - center[0]).abs() > radius || (g.chunk[1] - center[1]).abs() > radius {
                continue;
            }
            let target = Target::Stronghold { index: g.index };
            let key = (Dimension::Overworld, target);
            if self.verdicts.contains_key(&key) || !self.queued.insert(key) {
                continue;
            }
            self.queue.push_back(Lookup {
                dimension: Dimension::Overworld,
                target,
                structures: SetId::Strongholds.def().lookup,
                origin: g.pos(),
                radius: 0,
            });
        }
    }

    /// Drop queued lookups that are farther than `radius` chunks from `center` or in another
    /// dimension (the player went elsewhere). Verdicts are kept.
    pub fn retain_near(&mut self, dimension: Dimension, center: [i32; 2], radius: i32) {
        let queued = &mut self.queued;
        self.queue.retain(|l| {
            let keep = l.dimension == dimension
                && ((l.origin[0] >> 4) - center[0]).abs() <= radius
                && ((l.origin[2] >> 4) - center[1]).abs() <= radius;
            if !keep {
                queued.remove(&(l.dimension, l.target));
            }
            keep
        });
    }

    /// Lookups still waiting.
    pub fn pending(&self) -> usize {
        self.queue.len()
    }

    /// The next lookup to run now, if the budget allows. Call from the server tick (once per
    /// tick-end, in a loop until `None`) with the server's tick counter.
    pub fn take(&mut self, tick: u64) -> Option<Lookup> {
        if tick != self.tick {
            let elapsed = tick.saturating_sub(self.tick).min(20);
            let cap = u64::from(self.budget.per_second) * 20;
            self.tokens = (self.tokens + elapsed * u64::from(self.budget.per_second)).min(cap);
            self.tick = tick;
            self.used_this_tick = 0;
        }
        if self.used_this_tick >= self.budget.per_tick || self.tokens < 20 || self.queue.is_empty()
        {
            return None;
        }
        let lookup = self.queue.pop_front()?;
        self.queued.remove(&(lookup.dimension, lookup.target));
        self.tokens -= 20;
        self.used_this_tick += 1;
        Some(lookup)
    }

    /// Put `lookup` back at the end of the queue unanswered (its chunk is not loaded yet): it
    /// comes round again after the others, unless `retain_near` drops it first.
    pub fn defer(&mut self, lookup: Lookup) {
        let key = (lookup.dimension, lookup.target);
        if self.verdicts.contains_key(&key) || !self.queued.insert(key) {
            return;
        }
        self.queue.push_back(lookup);
    }

    /// Store the server's answer for `lookup`: the first `Some((structure, position))` of its
    /// structures, or `None`.
    pub fn record(&mut self, lookup: &Lookup, answer: Option<(&str, [i32; 3])>) {
        let verdict = match (lookup.target, answer) {
            // A candidate is confirmed only by a start at that very position.
            (Target::Candidate { .. }, Some((s, pos))) if pos == lookup.origin => {
                Verdict::Present {
                    structure: s.to_string(),
                    pos,
                }
            }
            (Target::Candidate { .. }, _) => Verdict::Absent,
            // The nearest stronghold from a ring guess is that stronghold (rings are ≥ 3
            // guesses apart by hundreds of chunks).
            (Target::Stronghold { .. }, Some((s, pos))) => Verdict::Present {
                structure: s.to_string(),
                pos,
            },
            (Target::Stronghold { .. }, None) => Verdict::Absent,
        };
        self.verdicts
            .insert((lookup.dimension, lookup.target), verdict);
        self.revision += 1;
    }

    /// Bumped by every [`Verifier::record`]: rebuild cached markers when it changes.
    pub fn revision(&self) -> u64 {
        self.revision
    }

    /// The verdict for a target, if checked.
    pub fn verdict(&self, dimension: Dimension, target: Target) -> Option<&Verdict> {
        self.verdicts.get(&(dimension, target))
    }

    /// Markers for a view rectangle (chunks, inclusive): seed candidates of `sets` (empty = all)
    /// with what the server said; candidates the server rejected are left out. Strongholds:
    /// confirmed ones at their exact place, unchecked ones as `Approximate` ring guesses.
    pub fn markers(
        &self,
        dimension: Dimension,
        min_chunk: [i32; 2],
        max_chunk: [i32; 2],
        sets: &[SetId],
    ) -> Vec<Marker> {
        markers_with(self.seed, Some(self), dimension, min_chunk, max_chunk, sets)
    }
}

/// [`Verifier::markers`] without a server (remote servers, user-entered seed): every candidate is
/// `Possible` (or `Approximate` for strongholds), except sets whose [`Certainty`] is `Exact`,
/// which are shown `Confirmed`.
pub fn seed_markers(
    seed: i64,
    dimension: Dimension,
    min_chunk: [i32; 2],
    max_chunk: [i32; 2],
    sets: &[SetId],
) -> Vec<Marker> {
    markers_with(seed, None, dimension, min_chunk, max_chunk, sets)
}

fn markers_with(
    seed: i64,
    verifier: Option<&Verifier>,
    dimension: Dimension,
    min_chunk: [i32; 2],
    max_chunk: [i32; 2],
    sets: &[SetId],
) -> Vec<Marker> {
    let wanted = |s: SetId| {
        (sets.is_empty() || sets.contains(&s)) && s.def().dimensions.contains(&dimension)
    };
    let inside = |c: [i32; 2]| {
        c[0] >= min_chunk[0] && c[0] <= max_chunk[0] && c[1] >= min_chunk[1] && c[1] <= max_chunk[1]
    };
    let mut out = Vec::new();
    for set in SetId::ALL {
        if !wanted(set) || set == SetId::Strongholds {
            continue;
        }
        candidates_in(seed, set, min_chunk, max_chunk, |c| {
            let target = Target::Candidate {
                set,
                chunk: c.chunk,
            };
            let deterministic = set.def().structures.len() == 1 || set == SetId::NetherComplexes;
            let guess = deterministic.then(|| c.first_pick.to_string());
            let marker = match verifier.and_then(|v| v.verdict(dimension, target)) {
                Some(Verdict::Present { structure, pos }) => Some(Marker {
                    set,
                    chunk: c.chunk,
                    pos: *pos,
                    structure: Some(structure.clone()),
                    status: Status::Confirmed,
                }),
                Some(Verdict::Absent) => None,
                None => Some(Marker {
                    set,
                    chunk: c.chunk,
                    pos: c.pos,
                    structure: guess,
                    status: if c.certainty == Certainty::Exact {
                        Status::Confirmed
                    } else {
                        Status::Possible
                    },
                }),
            };
            out.extend(marker);
        });
    }
    if wanted(SetId::Strongholds) {
        for g in vanilla_rings(seed) {
            let target = Target::Stronghold { index: g.index };
            match verifier.and_then(|v| v.verdict(dimension, target)) {
                Some(Verdict::Present { structure, pos }) => {
                    let chunk = [pos[0] >> 4, pos[2] >> 4];
                    if inside(chunk) {
                        out.push(Marker {
                            set: SetId::Strongholds,
                            chunk,
                            pos: *pos,
                            structure: Some(structure.clone()),
                            status: Status::Confirmed,
                        });
                    }
                }
                Some(Verdict::Absent) => {}
                None => {
                    if inside(g.chunk) {
                        out.push(Marker {
                            set: SetId::Strongholds,
                            chunk: g.chunk,
                            pos: g.pos(),
                            structure: Some("minecraft:stronghold".to_string()),
                            status: Status::Approximate,
                        });
                    }
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::structures::potential_chunk;

    #[test]
    fn queue_is_nearest_first_and_deduplicated() {
        let mut v = Verifier::new(
            42,
            Budget {
                per_second: 1000,
                per_tick: 1000,
            },
        );
        v.request_area(
            Dimension::Overworld,
            [0, 0],
            80,
            &[SetId::Villages, SetId::DesertPyramids],
        );
        let n = v.pending();
        assert!(n > 10);
        v.request_area(
            Dimension::Overworld,
            [0, 0],
            80,
            &[SetId::Villages, SetId::DesertPyramids],
        );
        assert_eq!(v.pending(), n);
        let mut last = -1i64;
        while let Some(l) = v.take(1) {
            let (x, z) = (i64::from(l.origin[0] >> 4), i64::from(l.origin[2] >> 4));
            let d = x * x + z * z;
            assert!(d >= last);
            last = d;
            assert_eq!(l.radius, 0);
        }
        assert_eq!(v.pending(), 0);
    }

    #[test]
    fn budget_limits_per_tick_and_per_second() {
        let mut v = Verifier::new(
            1,
            Budget {
                per_second: 4,
                per_tick: 2,
            },
        );
        v.request_area(Dimension::Overworld, [0, 0], 300, &[SetId::Villages]);
        let mut taken_by_tick = Vec::new();
        for tick in 1..=60u64 {
            let mut n = 0;
            while v.take(tick).is_some() {
                n += 1;
            }
            taken_by_tick.push(n);
        }
        assert!(taken_by_tick.iter().all(|&n| n <= 2));
        let total: i32 = taken_by_tick.iter().sum();
        // 4 at start (full bucket) + 4/s for 3 s.
        assert!((12..=17).contains(&total), "{total}");
    }

    #[test]
    fn verdicts_shape_markers() {
        let seed = 42;
        let mut v = Verifier::new(
            seed,
            Budget {
                per_second: 1000,
                per_tick: 1000,
            },
        );
        v.request_area(Dimension::Overworld, [0, 0], 40, &[SetId::Villages]);
        let a = v.take(1).unwrap();
        let b = v.take(1).unwrap();
        v.record(&a, Some(("minecraft:village_plains", a.origin)));
        v.record(&b, None);
        let markers = v.markers(
            Dimension::Overworld,
            [-40, -40],
            [40, 40],
            &[SetId::Villages],
        );
        let at = |o: [i32; 3]| markers.iter().find(|m| m.pos == o);
        let ma = at(a.origin).unwrap();
        assert_eq!(ma.status, Status::Confirmed);
        assert_eq!(ma.structure.as_deref(), Some("minecraft:village_plains"));
        assert!(at(b.origin).is_none());
        assert!(
            markers
                .iter()
                .filter(|m| m.status == Status::Possible)
                .count()
                >= 1
        );
    }

    #[test]
    fn found_elsewhere_is_not_a_confirmation() {
        let mut v = Verifier::new(5, Budget::default());
        v.request_area(Dimension::Overworld, [0, 0], 34, &[SetId::Villages]);
        let l = v.take(1).unwrap();
        v.record(
            &l,
            Some((
                "minecraft:village_plains",
                [l.origin[0] + 16, 0, l.origin[2]],
            )),
        );
        assert_eq!(v.verdict(l.dimension, l.target), Some(&Verdict::Absent));
    }

    #[test]
    fn strongholds_are_queued_and_resolved() {
        let mut v = Verifier::new(
            42,
            Budget {
                per_second: 1000,
                per_tick: 1000,
            },
        );
        v.request_strongholds([0, 0], 200);
        assert_eq!(v.pending(), 3); // ring 0 only (128 ± 40 chunks)
        let l = v.take(1).unwrap();
        v.record(
            &l,
            Some((
                "minecraft:stronghold",
                [l.origin[0] + 48, 0, l.origin[2] - 32],
            )),
        );
        let m = v.markers(
            Dimension::Overworld,
            [-200, -200],
            [200, 200],
            &[SetId::Strongholds],
        );
        assert_eq!(
            m.iter().filter(|m| m.status == Status::Confirmed).count(),
            1
        );
        assert_eq!(
            m.iter().filter(|m| m.status == Status::Approximate).count(),
            2
        );
    }

    #[test]
    fn a_deferred_lookup_comes_last() {
        let mut v = Verifier::new(7, Budget { per_second: 1000, per_tick: 1000 });
        v.request_area(Dimension::Overworld, [0, 0], 40, &[SetId::Villages]);
        let n = v.pending();
        let first = v.take(1).unwrap();
        v.defer(first);
        assert_eq!(v.pending(), n);
        let mut last = None;
        while let Some(l) = v.take(1) {
            last = Some(l);
        }
        assert_eq!(last, Some(first));
        v.record(&first, None);
        v.defer(first);
        assert_eq!(v.pending(), 0, "an answered one is not queued again");
    }

    #[test]
    fn retain_near_drops_far_lookups() {
        let mut v = Verifier::new(3, Budget::default());
        v.request_area(Dimension::Overworld, [0, 0], 200, &[SetId::Villages]);
        let before = v.pending();
        v.retain_near(Dimension::Overworld, [0, 0], 50);
        assert!(v.pending() < before && v.pending() > 0);
        v.retain_near(Dimension::Nether, [0, 0], 50);
        assert_eq!(v.pending(), 0);
    }

    #[test]
    fn seed_markers_mark_exact_sets_confirmed() {
        let m = seed_markers(42, Dimension::Nether, [-100, -100], [100, 100], &[]);
        assert!(
            m.iter()
                .any(|m| m.set == SetId::NetherComplexes && m.status == Status::Confirmed)
        );
        assert!(
            m.iter()
                .any(|m| m.set == SetId::NetherFossils && m.status == Status::Possible)
        );
        // The candidate positions are the spread's.
        for mk in m.iter().filter(|m| m.set == SetId::NetherComplexes) {
            let cell = crate::structures::cell_of(mk.chunk, 27);
            assert_eq!(
                potential_chunk(42, SetId::NetherComplexes, cell[0], cell[1]),
                Some(mk.chunk)
            );
        }
    }
}
