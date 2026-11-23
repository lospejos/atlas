//! The explore overlays, drawn where the maps' overlay layer is: the chunk and region grid,
//! slime chunks and structures, all worked out from the world's seed by `atlas-explore`.
//!
//! In single player and as a LAN host the mod's half in the integrated server knows the seed
//! and checks every structure the seed makes possible against the real world (what the loaded
//! chunks hold), one a tick at most: those markers are exact. Strongholds are not checked (the
//! game's search places all of them at once, past a hook's budget): they stay "about here".
//! On someone else's server nothing tells the seed; typed into the filter panel it gives exact
//! slime chunks and "possible" structures, drawn hollow because their biome is not checked.
use std::collections::HashMap;
use std::sync::Mutex;

use atlas_explore::grid::{self, GridLine, LineKind};
use atlas_explore::{Budget, Dimension, Marker, SetId, Status, Verifier, seed_markers, slime, stronghold};
use atlas_explore::verify::Target;
use mod_api::{ClientHost, ConfigPath, DrawContext, ServerHost, ServerTickEvent, ServerTickPhase};

use crate::gfx::{self, PANEL_BG, argb, inside};
use crate::mapscreen::Hit;
use crate::mapview::View;
use crate::State;

/// What the two halves of the mod share: the same wasm instance serves the client's hooks and
/// the integrated server's.
struct Shared {
    /// Built by the server half from the world's seed; none on someone else's server.
    verifier: Option<Verifier>,
    /// From the client half: check around this chunk of this dimension.
    want: Option<(Dimension, [i32; 2])>,
    sets: Vec<SetId>,
    /// Server ticks since the verifier was made.
    warm: u32,
    /// The server tick a check ran in, to read how long that tick took on the next one.
    checked: Option<u64>,
    /// A check made a tick run long: no more for this world.
    off: bool,
    /// The world the client half is in now (a number new for each world it enters, 0 for
    /// none), and the one the verifier was built in: a server tick that runs once more after
    /// the client left builds nothing, and a verifier of another world is never used.
    world: u64,
    built_for: u64,
}
static SHARED: Mutex<Shared> = Mutex::new(Shared {
    verifier: None,
    want: None,
    sets: Vec::new(),
    warm: 0,
    checked: None,
    off: false,
    world: 0,
    built_for: 0,
});
/// Counts the worlds the client half entered.
static WORLDS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
/// Checks a second and a tick: each is up to a few hundred cheap reads.
const CHECK_BUDGET: Budget = Budget { per_second: 10, per_tick: 1 };
/// A tick with a check that ran this much (ns) over the average stops the checks.
const CHECK_SLOW_NS: f64 = 20.0e6;
/// Server ticks before the first lookup: the world is still coming up.
const WARM_UP: u32 = 200;
/// No lookups while the server's ticks take longer than this on average, in milliseconds.
const BUSY_MS: f64 = 12.0;

/// Test builds: what the server half is about to do, for the log should the call never return.
pub static TRACE: Mutex<String> = Mutex::new(String::new());

fn trace(text: String) {
    if crate::TEST && let Ok(mut t) = TRACE.try_lock() {
        *t = text;
    }
}

/// The sets that are off until asked for: one candidate a chunk or so, most of them not real.
const OPT_IN: [SetId; 3] = [SetId::Mineshafts, SetId::BuriedTreasures, SetId::NetherFossils];
/// A colour for each set, in `SetId::ALL`'s order.
const COLORS: [u32; 21] = [
    0xF2C14E, 0x9B6A4A, 0xE8D38A, 0x5FA052, 0x6B7D3A, 0xBFE6F2, 0x3FB6B2, 0x7A4A2A, 0x5B8FD9, 0x8A6B4A, 0xB45CF0,
    0x2E6F78, 0xC98A5A, 0xE07A3A, 0xA0A0A0, 0xFFD700, 0x8C7A5A, 0x9FE070, 0xC03030, 0xE0E0D0, 0xD9A6F2,
];
/// Regions of markers worked out a frame at most.
const PER_FRAME: usize = 1;
const PANEL_W: f32 = 236.0;
const ROW: f32 = 11.0;

pub struct Explore {
    pub grid: bool,
    pub slime: bool,
    pub structures: bool,
    /// The structure sets that are shown: none when all are switched off (unlike
    /// `atlas_explore`'s lists, where an empty one is every set).
    pub sets: Vec<SetId>,
    /// The filter panel is open.
    pub panel: bool,
    /// The seed being typed into the panel.
    pub entry: Option<String>,
    /// The seed typed for this world, when the server does not tell it.
    typed: Option<i64>,
    /// The seed in use, and whether the server half gave it (then the markers are checked).
    seed: Option<(i64, bool)>,
    /// The markers of each 32 x 32 chunk region, with the stamp they were made at.
    cache: HashMap<(Dimension, i32, i32), (u64, Vec<Marker>)>,
    slimes: HashMap<(i32, i32), [u32; 32]>,
    /// Changes with the seed, the shown sets and each answer of the server.
    stamp: u64,
    revision: u64,
    asked: Option<(Dimension, [i32; 2], u64)>,
    /// Counts the changes of the shown sets.
    generation: u64,
    /// Test builds: behave as on someone else's server, where no server half answers.
    pub remote: bool,
    loaded: bool,
    dirty: bool,
    /// Structure lookups still waiting in the server half.
    pub pending: usize,
    /// The structure markers the last draw of the world map put on the screen: where, the
    /// structure's place and its name (a click there makes a temporary waypoint).
    pub picks: Vec<([f32; 2], [i32; 3], String)>,
}

impl Default for Explore {
    fn default() -> Self {
        Self {
            grid: false,
            slime: false,
            structures: false,
            sets: SetId::ALL.into_iter().filter(|s| !OPT_IN.contains(s)).collect(),
            panel: false,
            entry: None,
            typed: None,
            seed: None,
            cache: HashMap::new(),
            slimes: HashMap::new(),
            stamp: 1,
            revision: 0,
            asked: None,
            generation: 0,
            remote: false,
            loaded: false,
            dirty: false,
            pending: 0,
            picks: Vec::new(),
        }
    }
}

/// A typed seed as the game reads it: a number, else the text's Java `hashCode`.
pub fn parse_seed(text: &str) -> Option<i64> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    if let Ok(seed) = text.parse::<i64>() {
        return Some(seed);
    }
    let hash = text.encode_utf16().fold(0i32, |h, c| h.wrapping_mul(31).wrapping_add(i32::from(c)));
    Some(i64::from(hash))
}

impl Explore {
    /// The settings file. `sets=none` when every set is switched off; a file without a `sets`
    /// line (written before there was one) shows the default sets.
    fn encode(&self) -> String {
        let sets: Vec<&str> = self.sets.iter().map(|s| s.def().id).collect();
        let sets = if sets.is_empty() { "none".to_string() } else { sets.join(",") };
        format!(
            "# atlas explore overlays\ngrid={}\nslime={}\nstructures={}\nsets={}\n",
            u8::from(self.grid),
            u8::from(self.slime),
            u8::from(self.structures),
            sets
        )
    }

    fn decode(&mut self, text: &str) {
        for line in text.lines() {
            match line.split_once('=') {
                Some(("grid", v)) => self.grid = v == "1",
                Some(("slime", v)) => self.slime = v == "1",
                Some(("structures", v)) => self.structures = v == "1",
                // `none`, and the empty list 0.4.0 wrote when all were switched off: none.
                Some(("sets", v)) => self.sets = v.split(',').filter_map(SetId::from_id).collect(),
                _ => {}
            }
        }
    }

    /// A world was entered: from now the server half builds for this one only.
    pub fn enter(&mut self) {
        self.leave();
        let world = WORLDS.fetch_add(1, std::sync::atomic::Ordering::Relaxed) + 1;
        if let Ok(mut shared) = SHARED.try_lock() {
            shared.world = world;
        }
    }

    /// Forgets the world: the next one has another seed.
    pub fn leave(&mut self) {
        if let Ok(mut shared) = SHARED.try_lock() {
            shared.verifier = None;
            shared.want = None;
            shared.warm = 0;
            shared.checked = None;
            shared.off = false;
            shared.world = 0;
            shared.built_for = 0;
        }
        self.seed = None;
        self.typed = None;
        self.entry = None;
        self.cache.clear();
        self.slimes.clear();
        self.asked = None;
        self.stamp += 1;
        self.pending = 0;
    }

    fn changed(&mut self) {
        self.stamp += 1;
        self.generation += 1;
        self.dirty = true;
    }

    /// How many cached markers are confirmed, possible and approximate: for the test log.
    pub fn counts(&self) -> [usize; 3] {
        let mut n = [0; 3];
        for m in self.cache.values().filter(|c| c.0 == self.stamp).flat_map(|c| &c.1) {
            n[match m.status {
                Status::Confirmed => 0,
                Status::Possible => 1,
                Status::Approximate => 2,
            }] += 1;
        }
        n
    }

    /// The marker nearest block `x`, `z` among those worked out: for the test build.
    pub fn nearest(&self, x: f64, z: f64) -> Option<[i32; 3]> {
        let far = |m: &&Marker| ((f64::from(m.pos[0]) - x).powi(2) + (f64::from(m.pos[2]) - z).powi(2)) as i64;
        self.cache.values().flat_map(|c| &c.1).min_by_key(far).map(|m| m.pos)
    }

    /// Whether slime chunks and structures have no seed to be worked out from.
    pub fn seed_missing(&self) -> bool {
        self.seed.is_none()
    }

    /// Starts typing the seed, unless the server half gave it.
    pub fn begin_entry(&mut self) {
        if !self.seed.is_some_and(|s| s.1) {
            self.entry = Some(String::new());
        }
    }

    /// Flips set `index` of `SetId::ALL`; any other index only marks the settings changed.
    pub fn flip_set(&mut self, index: usize) {
        let Some(set) = SetId::ALL.get(index).copied() else {
            self.changed();
            return;
        };
        match self.sets.iter().position(|s| *s == set) {
            Some(at) => {
                self.sets.remove(at);
            }
            None => self.sets.push(set),
        }
        self.changed();
    }

    /// The markers of region `key`, made now when they are missing or out of date and `budget`
    /// allows; the old ones meanwhile.
    fn region(&mut self, key: (Dimension, i32, i32), budget: &mut usize) -> &[Marker] {
        let Some((seed, _)) = self.seed else { return &[] };
        let fresh = self.cache.get(&key).is_some_and(|(stamp, _)| *stamp == self.stamp);
        if !fresh && *budget > 0 {
            let (min, max) = ([key.1 * 32, key.2 * 32], [key.1 * 32 + 31, key.2 * 32 + 31]);
            let made = match SHARED.try_lock() {
                Ok(shared) => {
                    let checked = shared.verifier.as_ref().filter(|_| !self.remote);
                    Some(shown_markers(seed, checked, key.0, min, max, &self.sets))
                }
                Err(_) => None,
            };
            if let Some(markers) = made {
                *budget -= 1;
                if self.cache.len() > 400 {
                    self.cache.clear();
                }
                self.cache.insert(key, (self.stamp, markers));
            }
        }
        self.cache.get(&key).map_or(&[], |(_, markers)| markers.as_slice())
    }
}

/// The markers of sets `sets` in a box of chunks: checked by `verifier` when there is one, else
/// what the seed makes possible. No set, no marker (`atlas_explore` reads an empty list as
/// every set).
fn shown_markers(
    seed: i64,
    verifier: Option<&Verifier>,
    dim: Dimension,
    min: [i32; 2],
    max: [i32; 2],
    sets: &[SetId],
) -> Vec<Marker> {
    if sets.is_empty() {
        return Vec::new();
    }
    match verifier {
        Some(v) => v.markers(dim, min, max, sets),
        None => seed_markers(seed, dim, min, max, sets),
    }
}

/// How a marker reads in the status line.
fn words(m: &Marker, checked: bool) -> String {
    let name = match &m.structure {
        Some(id) => crate::radar::pretty(id),
        None => m.set.def().label.to_string(),
    };
    let how = match m.status {
        Status::Confirmed => "",
        Status::Possible if checked => " (possible: not checked yet)",
        Status::Possible => " (possible: the seed allows it here, the biome is not checked)",
        Status::Approximate => " (about here: within 128 blocks)",
    };
    format!("{name}{how}  X {}  Z {}", m.pos[0], m.pos[2])
}

/// A marker's name beside the cursor: "Village (possible)", "Trial Chambers".
fn short(m: &Marker) -> String {
    let name = match &m.structure {
        Some(id) => crate::radar::pretty(id),
        None => m.set.def().label.to_string(),
    };
    match m.status {
        Status::Confirmed => name,
        Status::Possible => format!("{name} (possible)"),
        Status::Approximate => format!("{name} (about here)"),
    }
}

/// A structure marker: solid when it is certain, hollow when it is only possible.
fn mark(draw: &mut dyn DrawContext, at: [f32; 2], rgb: u32, status: Status, half: f32) {
    if status == Status::Confirmed {
        gfx::marker(draw, at, rgb, half);
        draw.fill([at[0] - 0.5, at[1] - 0.5, 1.0, 1.0], 0xFF00_0000);
        return;
    }
    let (x, y, side) = (at[0] - half, at[1] - half, half * 2.0);
    draw.fill([x - 1.0, y - 1.0, side + 2.0, side + 2.0], 0x6000_0000);
    let color = 0xD000_0000 | rgb;
    draw.fill([x, y, side, 1.0], color);
    draw.fill([x, y + side - 1.0, side, 1.0], color);
    draw.fill([x, y + 1.0, 1.0, side - 2.0], color);
    draw.fill([x + side - 1.0, y + 1.0, 1.0, side - 2.0], color);
}

impl State {
    /// The client tick's part: the settings file, the seed, and what the server half is asked.
    pub(crate) fn explore_tick(&mut self, host: &dyn ClientHost, pos: [f64; 3]) {
        let world = self.world.clone();
        let ex = &mut self.explore;
        if !ex.loaded {
            ex.loaded = true;
            if let Some(text) = ConfigPath::new("explore.txt").ok().and_then(|p| host.data_files().read(&p).ok()) {
                ex.decode(&String::from_utf8_lossy(&text));
            }
        }
        if ex.typed.is_none()
            && ex.seed.is_none()
            && let Some(world) = &world
            && let Some(text) = ConfigPath::new(&format!("{world}/seed.txt")).ok().and_then(|p| host.data_files().read(&p).ok())
        {
            ex.typed = parse_seed(&String::from_utf8_lossy(&text));
        }
        if ex.dirty {
            ex.dirty = false;
            if let Ok(path) = ConfigPath::new("explore.txt") {
                let _ = host.data_files().write(&path, ex.encode().as_bytes());
            }
            if let (Some(seed), Some(world)) = (ex.typed, &world)
                && let Ok(path) = ConfigPath::new(&format!("{world}/seed.txt"))
            {
                let _ = host.data_files().write(&path, seed.to_string().as_bytes());
            }
        }
        let Ok(mut shared) = SHARED.try_lock() else { return };
        let ours = shared.world != 0 && shared.built_for == shared.world;
        let seed = match &shared.verifier {
            Some(v) if !ex.remote && ours => {
                if v.revision() != ex.revision {
                    ex.revision = v.revision();
                    ex.stamp += 1;
                }
                ex.pending = v.pending();
                Some((v.seed(), true))
            }
            _ => ex.typed.map(|s| (s, false)),
        };
        if seed != ex.seed {
            ex.seed = seed;
            ex.slimes.clear();
            ex.stamp += 1;
        }
        // Ask the server half to check around the player, again when they come into another
        // region or the shown sets change.
        let Some(dim) = Dimension::from_key(&self.dim) else { return };
        let chunk = [(pos[0].floor() as i32) >> 4, (pos[2].floor() as i32) >> 4];
        let here = (dim, [chunk[0] >> 5, chunk[1] >> 5], ex.generation);
        if ex.structures && ex.asked != Some(here) {
            ex.asked = Some(here);
            shared.want = Some((dim, chunk));
            shared.sets = ex.sets.clone();
        }
    }

    /// The bar's part: the three switches and the filter panel's button.
    pub(crate) fn explore_bar(&self) -> [(&'static str, bool, String); 4] {
        let ex = &self.explore;
        let need = if ex.seed.is_some() { "" } else { " (needs the seed: Filter)" };
        let structures = match ex.seed {
            Some((_, true)) if ex.pending > 0 => format!("Structures: checking {} with the world", ex.pending),
            Some((_, true)) => "Structures, each checked against the world (strongholds: about here)".to_string(),
            Some((_, false)) => "Structures the seed makes possible: hollow, their biome is not checked".to_string(),
            None => format!("Structures{need}"),
        };
        [
            ("Grid", ex.grid, "Chunk borders, region borders stronger".to_string()),
            ("Slime", ex.slime, format!("Slime chunks (Overworld){need}")),
            ("Structures", ex.structures, structures),
            ("Filter", ex.panel, "Which structures are shown, and the seed".to_string()),
        ]
    }

    /// Draws the grid, the slime chunks and the structures over the world map's terrain.
    pub(crate) fn draw_explore(
        &mut self,
        draw: &mut dyn DrawContext,
        view: View,
        mouse: [f32; 2],
        tip: &mut Option<String>,
        label: &mut Option<String>,
    ) {
        let [w, h] = self.gui;
        self.explore.picks.clear();
        let (min, max) = (self.block_at(view, [0.0, 0.0]), self.block_at(view, [w, h]));
        let (min, max) = ([min[0].floor() as i32, min[1].floor() as i32], [max[0].ceil() as i32, max[1].ceil() as i32]);
        let dim = Dimension::from_key(&self.dim);
        if self.explore.slime
            && dim == Some(Dimension::Overworld)
            && view.zoom >= 0.5
            && let Some((seed, _)) = self.explore.seed
        {
            let side = 16.0 * view.zoom;
            for cz in (min[1] >> 4)..=(max[1] >> 4) {
                for cx in (min[0] >> 4)..=(max[0] >> 4) {
                    let rows = self
                        .explore
                        .slimes
                        .entry((cx >> 5, cz >> 5))
                        .or_insert_with(|| slime::slime_region_bitmap(seed, cx >> 5, cz >> 5));
                    if rows[(cz & 31) as usize] >> (cx & 31) & 1 != 0 {
                        let at = self.screen_at(view, f64::from(cx * 16), f64::from(cz * 16));
                        draw.fill([at[0], at[1], side, side], 0x5A40_E040);
                        draw.fill([at[0], at[1], side, 1.0], 0x9040_E040);
                        draw.fill([at[0], at[1], 1.0, side], 0x9040_E040);
                    }
                }
            }
            if self.explore.slimes.len() > 600 {
                self.explore.slimes.clear();
            }
        }
        if self.explore.grid {
            let mut lines = Vec::new();
            grid::grid_lines(min, max, view.zoom, |line| lines.push(line));
            for line in lines {
                let (x, z, kind) = match line {
                    GridLine::X { x, kind } => (Some(x), None, kind),
                    GridLine::Z { z, kind } => (None, Some(z), kind),
                };
                let color = if kind == LineKind::Region { 0xB0FF_E080 } else { 0x4000_0000 };
                if let Some(x) = x {
                    let at = self.screen_at(view, f64::from(x), 0.0);
                    draw.fill([at[0].round(), 0.0, 1.0, h], color);
                }
                if let Some(z) = z {
                    let at = self.screen_at(view, 0.0, f64::from(z));
                    draw.fill([0.0, at[1].round(), w, 1.0], color);
                }
            }
        }
        let (Some(dim), true) = (dim, self.explore.structures) else { return };
        let checked = self.explore.seed.is_some_and(|s| s.1);
        // The regions on the screen, nearest the middle first; a wide view shows the middle ones.
        let (r0, r1) = ([min[0] >> 9, min[1] >> 9], [max[0] >> 9, max[1] >> 9]);
        let mid = [(r0[0] + r1[0]) / 2, (r0[1] + r1[1]) / 2];
        let mut keys: Vec<(i32, i32)> =
            (r0[1]..=r1[1]).flat_map(|z| (r0[0]..=r1[0]).map(move |x| (x, z))).take(400).collect();
        keys.sort_by_key(|k| (k.0 - mid[0]).abs().max((k.1 - mid[1]).abs()));
        keys.truncate(80);
        let mut budget = PER_FRAME;
        let mut drawn: Vec<([f32; 2], Marker)> = Vec::new();
        for key in keys {
            for m in self.explore.region((dim, key.0, key.1), &mut budget) {
                let at = [
                    ((f64::from(m.pos[0]) + 0.5 - view.cx) as f32) * view.zoom + w / 2.0,
                    ((f64::from(m.pos[2]) + 0.5 - view.cz) as f32) * view.zoom + h / 2.0,
                ];
                if at[0] > -8.0 && at[1] > -8.0 && at[0] < w + 8.0 && at[1] < h + 8.0 {
                    drawn.push((at, m.clone()));
                }
            }
        }
        // None under a panel, the bar or a status line.
        drawn.retain(|(at, _)| !self.covered(*at));
        for (at, m) in &drawn {
            let rgb = COLORS[SetId::ALL.iter().position(|s| *s == m.set).unwrap_or(0)];
            if m.status == Status::Approximate {
                // A ring as wide as the guess may be off.
                let r = (stronghold::RING_ERROR_CHUNKS * 16) as f32 * view.zoom;
                if r > 6.0 {
                    for i in 0..32 {
                        let (s, c) = (i as f32 / 32.0 * std::f32::consts::TAU).sin_cos();
                        draw.fill([at[0] + c * r - 0.5, at[1] + s * r - 0.5, 1.5, 1.5], 0xC000_0000 | rgb);
                    }
                }
            }
            mark(draw, *at, rgb, m.status, 3.0);
            // Named as its set is ("Ocean Monument", "Village"): what a waypoint should say.
            self.explore.picks.push((*at, m.pos, m.set.def().label.to_string()));
            if (mouse[0] - at[0]).abs() <= 5.0 && (mouse[1] - at[1]).abs() <= 5.0 {
                *tip = Some(words(m, checked));
                *label = Some(short(m));
            }
        }
    }

    /// The structures on the minimap, through `place`.
    pub(crate) fn draw_explore_mini(
        &mut self,
        draw: &mut dyn DrawContext,
        bounds: [f64; 4],
        place: impl Fn(f64, f64) -> Option<[f32; 2]>,
    ) {
        let (Some(dim), true) = (Dimension::from_key(&self.dim), self.explore.structures) else { return };
        let r = |v: f64| (v.floor() as i32) >> 9;
        let mut budget = PER_FRAME;
        for rz in r(bounds[1])..=r(bounds[3]) {
            for rx in r(bounds[0])..=r(bounds[2]) {
                for m in self.explore.region((dim, rx, rz), &mut budget) {
                    if let Some(at) = place(f64::from(m.pos[0]) + 0.5, f64::from(m.pos[2]) + 0.5) {
                        let rgb = COLORS[SetId::ALL.iter().position(|s| *s == m.set).unwrap_or(0)];
                        mark(draw, at, rgb, m.status, 2.0);
                    }
                }
            }
        }
    }

    /// The filter panel under the bar: the seed, and a switch for each structure set.
    pub(crate) fn draw_explore_panel(
        &mut self,
        draw: &mut dyn DrawContext,
        top: f32,
        mouse: [f32; 2],
        hits: &mut Vec<(Hit, [f32; 4])>,
        tip: &mut Option<String>,
    ) {
        if !self.explore.panel {
            return;
        }
        let rows = SetId::ALL.len().div_ceil(2);
        let panel = [4.0, top, PANEL_W, 18.0 + rows as f32 * ROW + 4.0];
        draw.fill(panel, 0xE810_1014);
        hits.push((Hit::Panel, panel));
        // The seed.
        let field = [panel[0] + 4.0, top + 3.0, PANEL_W - 8.0, 12.0];
        let ex = &self.explore;
        let (text, color) = match (&ex.entry, ex.seed) {
            (Some(typing), _) => (format!("Seed: {typing}{}", if (self.now / 300).is_multiple_of(2) { "_" } else { "" }), 0xFFFF_FFFF),
            (None, Some((seed, true))) => (format!("Seed: {seed} (this world's)"), 0xFF9F_E070),
            (None, Some((seed, false))) => (format!("Seed: {seed} (typed: click to change)"), 0xFFFF_E080),
            (None, None) => ("Seed: click, type it (/seed), Enter".to_string(), 0xFFA0_A0A0),
        };
        draw.fill(field, if ex.entry.is_some() { 0xFF00_0000 } else { 0x4000_0000 });
        draw.text(&text, [field[0] + 3.0, field[1] + 2.0], color, false);
        hits.push((Hit::Seed, field));
        if inside(field, mouse) {
            *tip = Some(match ex.seed {
                Some((_, true)) => "The server half read the seed: structures are checked against the world".into(),
                _ => "On a server the seed is not told: with it, slime chunks are exact, structures possible".into(),
            });
        }
        let dim = Dimension::from_key(&self.dim);
        for (i, set) in SetId::ALL.into_iter().enumerate() {
            let def = set.def();
            let (col, row) = ((i / rows) as f32, (i % rows) as f32);
            let r = [panel[0] + 4.0 + col * (PANEL_W / 2.0 - 2.0), top + 18.0 + row * ROW, PANEL_W / 2.0 - 6.0, ROW];
            let on = ex.sets.contains(&set);
            let here = dim.is_some_and(|d| def.dimensions.contains(&d));
            let hot = inside(r, mouse);
            if hot {
                draw.fill(r, 0x30FF_FFFF);
                *tip = Some(format!("{}: {}", def.label, if on { "shown" } else { "hidden" }));
            }
            draw.fill([r[0] + 1.0, r[1] + 2.0, 7.0, 7.0], 0xFF00_0000);
            if on {
                draw.fill([r[0] + 2.0, r[1] + 3.0, 5.0, 5.0], argb(COLORS[i]));
            }
            let color = match (on, here) {
                (true, true) => 0xFFFF_FFFF,
                (true, false) => 0xFF90_9090,
                _ => 0xFF60_6060,
            };
            draw.text(&gfx::fit(draw, def.label, r[2] - 12.0), [r[0] + 11.0, r[1] + 2.0], color, false);
            hits.push((Hit::Set(i), r));
        }
        let _ = PANEL_BG;
    }

    /// A key while the seed is being typed. True when it was taken.
    pub(crate) fn explore_key(&mut self, short: &str) -> bool {
        let Some(entry) = &mut self.explore.entry else { return false };
        match short {
            "enter" | "keypad.enter" => {
                let typed = parse_seed(entry);
                self.explore.entry = None;
                if typed.is_some() {
                    self.explore.typed = typed;
                    self.explore.changed();
                }
            }
            "escape" => self.explore.entry = None,
            "backspace" => {
                entry.pop();
            }
            "minus" if crate::TEST => entry.push('-'),
            _ if crate::TEST && short.len() == 1 => entry.push_str(short),
            _ => {}
        }
        true
    }

    pub(crate) fn explore_char(&mut self, c: char) {
        if let Some(entry) = &mut self.explore.entry
            && !c.is_control()
            && entry.chars().count() < 40
        {
            entry.push(c);
        }
    }
}

/// The server half, in the integrated server only: builds the verifier from the seed and checks
/// one seed candidate a tick at most against the world.
///
/// A check never searches (`find_nearest_structure` runs the generator, and for strongholds
/// places all 128 of them whatever the radius: in a real world one call ran past the hook's
/// 100 ms and stopped the mod). It reads `structures_at` at the candidate's place in its chunk,
/// a read that loads and builds nothing (None for a chunk not loaded: that candidate waits).
/// Strongholds are not checked: they stay "about here" from the ring maths.
pub fn server_tick(host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
    if !matches!(event.phase, ServerTickPhase::ServerTickEnd { .. }) {
        return;
    }
    let tick = event.context.tick.get();
    let Ok(mut shared) = SHARED.try_lock() else { return };
    trace(format!("tick {tick}: server_tick"));
    // No client in a world (it left, and this is the integrated server's last tick): nothing
    // is built, so no seed reaches the next world.
    if shared.world == 0 {
        shared.verifier = None;
        return;
    }
    if shared.built_for != shared.world {
        shared.verifier = None;
    }
    if shared.verifier.is_none() {
        let Some(seed) = host.level(Dimension::Overworld.key()).and_then(|l| l.seed(host)) else { return };
        shared.verifier = Some(Verifier::new(seed, CHECK_BUDGET));
        shared.built_for = shared.world;
    }
    let want = shared.want.take();
    // The sets the client shows (none when all are switched off); strongholds are never checked.
    let sets: Vec<SetId> = shared.sets.iter().copied().filter(|s| *s != SetId::Strongholds).collect();
    let Some(v) = shared.verifier.as_mut() else { return };
    if let Some((dim, center)) = want {
        v.retain_near(dim, center, 96);
        // None, or strongholds alone: nothing to check (an empty list would ask for every set).
        if !sets.is_empty() {
            v.request_area(dim, center, 64, &sets);
        }
    }
    if shared.off {
        return;
    }
    let (_, average) = host.tick_stats();
    let times = host.tick_times();
    // How long the tick of the last check took: well over the average, and checks stop here.
    if let Some(at) = shared.checked.take()
        && let Some(&ns) = times.get((at % 100) as usize)
        && ns as f64 - average * 1.0e6 > CHECK_SLOW_NS
    {
        shared.off = true;
        trace(format!("tick {tick}: a check took {} ms: checks stop", ns / 1_000_000));
        return;
    }
    // While the world is coming up, or the server is busy, no check.
    if shared.warm < WARM_UP {
        shared.warm += 1;
        return;
    }
    let recent = (1..=3).filter_map(|back| times.get((tick.wrapping_sub(back) % 100) as usize)).max().copied();
    if average > BUSY_MS || recent.is_some_and(|ns| ns > (BUSY_MS * 2.0e6) as u64) {
        return;
    }
    let Some(v) = shared.verifier.as_mut() else { return };
    let Some(lookup) = v.take(tick) else { return };
    let Target::Candidate { set, .. } = lookup.target else { return };
    let Some(level) = host.level(lookup.dimension.key()) else { return };
    trace(format!("tick {tick}: check {set:?} at {:?}", lookup.origin));
    match check(&*host, level, set, lookup.origin) {
        Some(Some(id)) => v.record(&lookup, Some((&id, lookup.origin))),
        Some(None) => v.record(&lookup, None),
        None => v.defer(lookup),
    }
    shared.checked = Some(tick);
    trace(format!("tick {tick}: done"));
}

/// Heights a check reads at, every 4 blocks of the level from `min` up to `top`, nearest the
/// sea first: where most structures' boxes are.
fn check_heights(min: i32, top: i32) -> Vec<i32> {
    let mut ys: Vec<i32> = (min..top).step_by(4).collect();
    ys.sort_by_key(|y| (y - 63).abs());
    ys
}

/// Whether one of `set`'s structures starts at candidate `at`: Some(its id) when one is there,
/// Some(None) when its chunk is loaded and none is, None when the chunk isn't loaded.
fn check(host: &dyn ServerHost, level: mod_api::LevelRef, set: SetId, at: [i32; 3]) -> Option<Option<String>> {
    let (min, height) = level.height(host)?;
    let ours = |id: &str| set.def().structures.iter().any(|(s, _)| *s == id);
    // The candidate's own column (a start's corner, with the set's locate offset), and the middle
    // of its chunk.
    let middle = [(at[0] & !15) + 8, (at[2] & !15) + 8];
    for (x, z) in [(at[0], at[2]), (middle[0], middle[1])] {
        for y in check_heights(min, min + height) {
            let found = level.structures_at(host, [x, y, z])?;
            if let Some(s) = found.iter().find(|s| ours(&s.structure)) {
                return Some(Some(s.structure.clone()));
            }
        }
    }
    Some(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_left_world_lets_no_seed_through() {
        // enter and leave skip a busy SHARED (try_lock): other tests may hold it, so retry until it took.
        fn settle(ex: &mut Explore, enter: bool, done: impl Fn(&Shared) -> bool) {
            for _ in 0..1000 {
                if enter { ex.enter() } else { ex.leave() }
                if done(&SHARED.lock().unwrap()) { return }
                std::thread::yield_now();
            }
            panic!("SHARED stayed busy");
        }
        let mut ex = Explore::default();
        settle(&mut ex, true, |s| s.world != 0);
        let world = SHARED.lock().unwrap().world;
        settle(&mut ex, true, |s| s.world != world);
        settle(&mut ex, false, |s| (s.world, s.built_for) == (0, 0));
        assert!(SHARED.lock().unwrap().verifier.is_none());
    }

    #[test]
    fn seeds_read_as_the_game_reads_them() {
        assert_eq!(parse_seed(" 42 "), Some(42));
        assert_eq!(parse_seed("-7"), Some(-7));
        // Java's "hello".hashCode().
        assert_eq!(parse_seed("hello"), Some(99_162_322));
        assert_eq!(parse_seed(""), None);
    }

    #[test]
    fn the_overlay_settings_come_back() {
        let mut ex = Explore { grid: true, structures: true, ..Explore::default() };
        ex.flip_set(SetId::ALL.iter().position(|s| *s == SetId::Mineshafts).unwrap());
        ex.flip_set(0);
        let mut back = Explore::default();
        back.decode(&ex.encode());
        assert!(back.grid && !back.slime && back.structures);
        assert_eq!(back.sets, ex.sets);
        assert!(back.sets.contains(&SetId::Mineshafts) && !back.sets.contains(&SetId::Villages));
    }

    #[test]
    fn all_sets_switched_off_show_none_and_stay_off() {
        let mut ex = Explore { structures: true, ..Explore::default() };
        for i in 0..SetId::ALL.len() {
            if ex.sets.contains(&SetId::ALL[i]) {
                ex.flip_set(i);
            }
        }
        assert!(ex.sets.is_empty());
        // Neither the seed's "possible" markers nor the checked ones (a verifier for the seed).
        let checked = Verifier::new(42, CHECK_BUDGET);
        let (min, max) = ([0, 0], [31, 31]);
        for verifier in [None, Some(&checked)] {
            assert!(!shown_markers(42, verifier, Dimension::Overworld, min, max, &[SetId::Villages]).is_empty());
            assert!(shown_markers(42, verifier, Dimension::Overworld, min, max, &ex.sets).is_empty());
        }
        // Saved as none, read back as none.
        let text = ex.encode();
        assert!(text.contains("sets=none\n"), "{text}");
        let mut back = Explore::default();
        back.decode(&text);
        assert!(back.sets.is_empty());
        // 0.4.0 wrote an empty list for all switched off: none too.
        let mut old = Explore::default();
        old.decode("grid=0\nslime=0\nstructures=1\nsets=\n");
        assert!(old.sets.is_empty() && old.structures);
        // A file without the line: the default sets.
        let mut older = Explore::default();
        older.decode("grid=1\nslime=0\nstructures=1\n");
        assert_eq!(older.sets, Explore::default().sets);
        assert!(!older.sets.is_empty());
    }

    #[test]
    fn a_typed_seed_gives_markers_for_a_region() {
        let mut ex = Explore { seed: Some((42, false)), ..Explore::default() };
        let mut budget = 1;
        let first = ex.region((Dimension::Overworld, 0, 0), &mut budget).len();
        assert!(first > 0 && budget == 0);
        // No budget left: another region waits, the made one stays.
        assert!(ex.region((Dimension::Overworld, 1, 0), &mut budget).is_empty());
        assert_eq!(ex.region((Dimension::Overworld, 0, 0), &mut budget).len(), first);
    }
}

