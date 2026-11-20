//! Reads the loaded chunks around the player into a store, a few a tick: new chunks nearest
//! first, then the ones already read again in turn. The surface is read from the top block of
//! each column down; a cave layer from a fixed height down to the first floor under open space.
use std::collections::HashMap;

use mod_api::{BlockState, ClientHost, Heightmap, HostBlockStates, LevelRef};

use crate::render::{GRASS, PLANT};
use crate::store::{BIOME_CELL, Legend, Store, VOID, WATER};

/// Chunks around the player that the surface is read in.
pub const RADIUS: i32 = 12;
/// Chunks around the player that a cave layer is read in.
pub const CAVE_RADIUS: i32 = 5;
/// How far below the surface a column is searched for a block with a map colour.
const DIG: i32 = 16;
/// Water deeper than this is as dark as a map shows it.
const MAX_DEPTH: u8 = 10;
/// A chunk that wasn't loaded is asked again after this many ticks.
const RETRY: u64 = 20;
/// A read chunk near the player is fully read again after this many ticks.
const FULL_AGAIN: u64 = 100;
/// A cave layer is this many blocks of height: the player's `y / 16` picks it.
pub const BAND: i32 = 16;
/// A cave column is read from this far above its band's bottom...
const CAVE_TOP: i32 = 24;
/// ...down through this much rock to the first opening...
const CAVE_ROCK: i32 = 40;
/// ...and through this much open space to its floor.
const CAVE_DROP: i32 = 32;
/// `MapColor.FIRE`: lava.
const LAVA: u8 = 4;
/// A tick stops starting chunks once it made this many host calls (about 0.2 microseconds each).
const CALL_BUDGET: u32 = 3500;

/// What a scanner reads.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Layer {
    Surface,
    /// The cave layer of this band.
    Cave(i32),
}

/// The band a player at height `y` is in, staying in `current` until `y` is 3 blocks out of it,
/// so standing on a border doesn't flip between two maps.
pub fn band(y: f64, current: Option<i32>) -> i32 {
    let y = y.floor() as i32;
    match current {
        Some(b) if y >= b * BAND - 3 && y < (b + 1) * BAND + 3 => b,
        _ => y.div_euclid(BAND),
    }
}

struct Seen {
    /// The surface heights of the last read.
    surface: [i16; 256],
    full_at: u64,
}

/// One column as it is stored.
#[derive(Clone, Copy)]
struct Column {
    color: u8,
    height: i16,
    depth: u8,
    tint: u8,
}

pub struct Scanner {
    pub layer: Layer,
    seen: HashMap<(i32, i32), Seen>,
    /// Chunks that weren't loaded, and when they were asked.
    absent: HashMap<(i32, i32), u64>,
    /// Map colour by block state.
    colors: HashMap<BlockState, u8>,
    /// Block states the game doesn't tint.
    untinted: HashMap<BlockState, ()>,
    /// The tints read in the chunk being scanned, by block state and 4 x 4 cell: the game
    /// blends a tint over the biomes around, which makes the read dear and its answer smooth,
    /// so one read stands for the cell.
    cell_tints: HashMap<(BlockState, u8), u8>,
    cursor: usize,
    /// A `run` stops starting chunks once it made this many host calls.
    pub budget: u32,
    pub tick: u64,
    /// Host calls made by the last `run`, and by all of them.
    pub calls: u32,
    pub calls_total: u64,
    /// The most calls one `run` made.
    pub calls_peak: u32,
    pub chunks_read: u64,
}

impl Scanner {
    pub fn new(layer: Layer) -> Self {
        Self {
            layer,
            seen: HashMap::new(),
            absent: HashMap::new(),
            colors: HashMap::new(),
            untinted: HashMap::new(),
            cell_tints: HashMap::new(),
            cursor: 0,
            budget: CALL_BUDGET,
            tick: 0,
            calls: 0,
            calls_total: 0,
            calls_peak: 0,
            chunks_read: 0,
        }
    }

    pub fn clear(&mut self) {
        self.seen.clear();
        self.absent.clear();
        self.cursor = 0;
    }

    fn color(&mut self, host: &dyn ClientHost, state: BlockState) -> u8 {
        if let Some(color) = self.colors.get(&state) {
            return *color;
        }
        self.calls += 1;
        let color = host.block_states().map_color(state).unwrap_or(0).min(61);
        self.colors.insert(state, color);
        color
    }

    /// The column whose top coloured block is `state` at `pos`: its water depth and its tint.
    fn found(
        &mut self,
        host: &dyn ClientHost,
        level: LevelRef,
        legend: &mut Legend,
        state: BlockState,
        color: u8,
        pos: [i32; 3],
    ) -> Column {
        let [x, y, z] = pos;
        let mut depth = 0;
        if color == WATER {
            depth = 1;
            while depth < MAX_DEPTH {
                self.calls += 1;
                let below = level.block_state(host, [x, y - i32::from(depth), z]);
                if below.map(|s| self.color(host, s)) != Some(WATER) {
                    break;
                }
                depth += 1;
            }
        }
        let mut tint = 0;
        if matches!(color, GRASS | PLANT | WATER) && !self.untinted.contains_key(&state) {
            let cell = ((z & 15) >> 2 << 2 | (x & 15) >> 2) as u8;
            if let Some(known) = self.cell_tints.get(&(state, cell)) {
                tint = *known;
            } else {
                self.calls += 1;
                match host.block_tint_color(state, 0, pos) {
                    Some(rgb) => {
                        tint = legend.tint_index(rgb & 0xFF_FFFF);
                        self.cell_tints.insert((state, cell), tint);
                    }
                    None => {
                        self.untinted.insert(state, ());
                    }
                }
            }
        }
        Column { color, height: y as i16, depth, tint }
    }

    /// One surface column from its height `top` (one above the highest block) down.
    fn surface_column(
        &mut self,
        host: &dyn ClientHost,
        level: LevelRef,
        legend: &mut Legend,
        x: i32,
        z: i32,
        top: i32,
    ) -> Column {
        let mut y = top - 1;
        for _ in 0..DIG {
            self.calls += 1;
            let Some(state) = level.block_state(host, [x, y, z]) else { break };
            let color = self.color(host, state);
            if color != 0 {
                return self.found(host, level, legend, state, color, [x, y, z]);
            }
            y -= 1;
        }
        Column { color: VOID, height: y as i16, depth: 0, tint: 0 }
    }

    /// One cave column from `top` down: through rock (every second block: an opening is two
    /// high or more) to the first open block, air or a fluid, then down the opening to its
    /// floor. A fluid is its own floor. Rock all the way is a seen column with no colour.
    fn cave_column(
        &mut self,
        host: &dyn ClientHost,
        level: LevelRef,
        legend: &mut Legend,
        x: i32,
        z: i32,
        top: i32,
    ) -> Column {
        let rock = Column { color: VOID, height: top as i16, depth: 0, tint: 0 };
        let read = |me: &mut Self, y: i32| -> Option<(BlockState, u8)> {
            me.calls += 1;
            let state = level.block_state(host, [x, y, z])?;
            Some((state, me.color(host, state)))
        };
        let mut y = top;
        // Through the rock.
        let open = loop {
            if y < top - CAVE_ROCK {
                return rock;
            }
            let Some((state, color)) = read(self, y) else { return rock };
            if color == 0 || color == WATER || color == LAVA {
                // The block skipped above may be the opening's first.
                if y < top
                    && let Some((above, c)) = read(self, y + 1)
                    && (c == WATER || c == LAVA)
                {
                    break (above, c, y + 1);
                }
                break (state, color, y);
            }
            y -= 2;
        };
        let (mut state, mut color, mut y) = open;
        // Down the opening.
        let floor = y - CAVE_DROP;
        while color == 0 {
            y -= 1;
            if y < floor {
                return rock;
            }
            let Some(next) = read(self, y) else { return rock };
            (state, color) = next;
        }
        self.found(host, level, legend, state, color, [x, y, z])
    }

    /// Reads chunk `chunk`: every column, or with `light` only those whose surface moved since
    /// the last read. None when the client doesn't hold the chunk.
    fn chunk(
        &mut self,
        host: &dyn ClientHost,
        level: LevelRef,
        store: &mut Store,
        legend: &mut Legend,
        chunk: (i32, i32),
        light: bool,
    ) -> Option<()> {
        let (bx, bz) = (chunk.0 * 16, chunk.1 * 16);
        let columns: Vec<(i32, i32)> = (0..256).map(|i| (bx + i % 16, bz + i / 16)).collect();
        self.calls += 1;
        let heights = host.client_surface_heights(&columns, Heightmap::WorldSurface).ok()?;
        let mut surface = [0i16; 256];
        for (s, h) in surface.iter_mut().zip(&heights) {
            *s = (*h)? as i16;
        }
        let mut color = [0u8; 256];
        let mut height = [0i16; 256];
        let mut depth = [0u8; 256];
        let mut tint = [0u8; 256];
        let old = if light { self.seen.get(&chunk).map(|s| s.surface) } else { None };
        if old == Some(surface) {
            return Some(());
        }
        self.cell_tints.clear();
        for i in 0..256 {
            let (x, z) = columns[i];
            let column = match self.layer {
                Layer::Surface => {
                    if let Some(old) = old
                        && old[i] == surface[i]
                        && let Some(kept) = store.column(x, z)
                    {
                        (color[i], height[i], depth[i]) = kept;
                        tint[i] = store.tint(x, z);
                        continue;
                    }
                    self.surface_column(host, level, legend, x, z, i32::from(surface[i]))
                }
                Layer::Cave(band) => {
                    let top = band * BAND + CAVE_TOP;
                    if top >= i32::from(surface[i]) {
                        // The layer starts in the sky here: it shows the ground.
                        self.surface_column(host, level, legend, x, z, i32::from(surface[i]))
                    } else {
                        self.cave_column(host, level, legend, x, z, top)
                    }
                }
            };
            (color[i], height[i], depth[i], tint[i]) = (column.color, column.height, column.depth, column.tint);
        }
        store.put_chunk(chunk, &color, &height, &depth, Some(&tint));
        if !light {
            // The biome of each 4 x 4 cell, where its first column's block is.
            for cell in 0..16 {
                let (cx, cz) = (cell % 4 * BIOME_CELL, cell / 4 * BIOME_CELL);
                let i = (cz * 16 + cx) as usize;
                self.calls += 1;
                if let Some(id) = level.client_biome(host, [bx + cx, i32::from(height[i]), bz + cz]) {
                    store.put_biome(bx + cx, bz + cz, legend.biome_index(&id));
                }
            }
        }
        let full_at = match (light, self.seen.get(&chunk)) {
            (true, Some(seen)) => seen.full_at,
            _ => self.tick,
        };
        self.seen.insert(chunk, Seen { surface, full_at });
        self.chunks_read += 1;
        Some(())
    }

    /// One tick's work around the player's chunk `center`: up to `fresh` chunks never read, and
    /// `again` already read ones checked for changes.
    #[allow(clippy::too_many_arguments)]
    pub fn run(
        &mut self,
        host: &dyn ClientHost,
        level: LevelRef,
        store: &mut Store,
        legend: &mut Legend,
        center: (i32, i32),
        fresh: usize,
        again: usize,
    ) {
        self.tick += 1;
        self.calls = 0;
        let tick = self.tick;
        let cave = matches!(self.layer, Layer::Cave(_));
        let radius = if cave { CAVE_RADIUS } else { RADIUS };
        self.budget = if cave { CALL_BUDGET * 2 } else { CALL_BUDGET };
        if tick.is_multiple_of(200) {
            let far = |k: &(i32, i32)| (k.0 - center.0).abs().max((k.1 - center.1).abs()) > radius + 4;
            self.seen.retain(|k, _| !far(k));
            self.absent.retain(|k, _| !far(k));
        }
        // New chunks, nearest ring first.
        let mut done = 0;
        let mut probes = 0;
        'rings: for ring in 0..=radius {
            // A cave layer just entered: the chunks the minimap shows are read two a tick.
            if cave && ring > 3 {
                self.budget = CALL_BUDGET;
            }
            for (dx, dz) in ring_offsets(ring) {
                let key = (center.0 + dx, center.1 + dz);
                if self.seen.contains_key(&key) {
                    continue;
                }
                if self.absent.get(&key).is_some_and(|at| tick - at < RETRY) {
                    continue;
                }
                // One column tells whether the chunk is there, before asking for all of it.
                self.calls += 1;
                probes += 1;
                let held = host
                    .client_surface_heights(&[(key.0 * 16, key.1 * 16)], Heightmap::WorldSurface)
                    .is_ok_and(|h| h.first().copied().flatten().is_some());
                if held && self.chunk(host, level, store, legend, key, false).is_some() {
                    self.absent.remove(&key);
                    done += 1;
                } else {
                    self.absent.insert(key, tick);
                }
                if done >= fresh || probes >= 64 || self.calls >= self.budget {
                    break 'rings;
                }
            }
        }
        // Chunks read before: the player's own and its neighbours in turn, the rest round the
        // whole square.
        let near = ring_offsets(0).into_iter().chain(ring_offsets(1));
        let side = (2 * radius + 1) as usize;
        let mut turn = Vec::with_capacity(again + 1);
        if cave {
            // A cave column has no height to tell that it changed: the nine chunks round the
            // player are read again in full, one every ten ticks.
            if tick.is_multiple_of(10) && self.calls < CALL_BUDGET {
                turn.push(near.clone().nth((tick / 10 % 9) as usize).unwrap_or((0, 0)));
            }
        } else {
            turn.push(near.clone().nth((tick % 9) as usize).unwrap_or((0, 0)));
            for _ in 0..again {
                self.cursor = (self.cursor + 1) % (side * side);
                turn.push(((self.cursor % side) as i32 - radius, (self.cursor / side) as i32 - radius));
            }
        }
        for (dx, dz) in turn {
            let key = (center.0 + dx, center.1 + dz);
            let Some(seen) = self.seen.get(&key) else { continue };
            let close = dx.abs() <= 1 && dz.abs() <= 1;
            let light = !cave && !(close && tick - seen.full_at >= FULL_AGAIN);
            if self.chunk(host, level, store, legend, key, light).is_none() {
                // Unloaded since: read it afresh when it comes back.
                self.seen.remove(&key);
            }
        }
        self.calls_total += u64::from(self.calls);
        self.calls_peak = self.calls_peak.max(self.calls);
    }
}

/// The offsets at Chebyshev distance `ring`.
fn ring_offsets(ring: i32) -> Vec<(i32, i32)> {
    if ring == 0 {
        return vec![(0, 0)];
    }
    let mut out = Vec::with_capacity((ring * 8) as usize);
    for d in -ring..=ring {
        out.push((d, -ring));
        out.push((d, ring));
    }
    for d in -ring + 1..ring {
        out.push((-ring, d));
        out.push((ring, d));
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rings_cover_the_square_once() {
        let mut all: Vec<(i32, i32)> = (0..=3).flat_map(ring_offsets).collect();
        assert_eq!(all.len(), 49);
        all.sort();
        all.dedup();
        assert_eq!(all.len(), 49);
    }

    #[test]
    fn bands_hold_on_their_borders() {
        assert_eq!(band(40.0, None), 2);
        assert_eq!(band(-1.0, None), -1);
        // Three blocks out either way still counts as the band the player was in.
        assert_eq!(band(30.0, Some(2)), 2);
        assert_eq!(band(50.0, Some(2)), 2);
        assert_eq!(band(28.0, Some(2)), 1);
        assert_eq!(band(51.0, Some(2)), 3);
    }
}
