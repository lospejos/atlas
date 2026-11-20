//! The explored map: one 512 x 512 block region per file, three numbers a column (vanilla map
//! colour id, the height of the block that gave it, water depth), kept in memory while near the
//! player or on the open map and written back in the background.
use std::collections::{HashMap, HashSet};

use mod_api::{ConfigFiles, ConfigPath};

/// A region's side in blocks.
pub const REGION: i32 = 512;
const CELLS: usize = (REGION * REGION) as usize;
/// A column nobody has seen yet.
pub const UNSEEN: u8 = 0;
/// A column that was seen and has no coloured block (the void, an all-glass column).
pub const VOID: u8 = 63;
/// `MapColor.WATER`.
pub const WATER: u8 = 12;
/// The first file format (0.1, 0.2): colour, depth and height planes.
const MAGIC_V1: &[u8; 4] = b"MMR1";
/// 0.3: the same, then a flag byte (bit 0: a tint plane follows) and the biome cells.
const MAGIC: &[u8; 4] = b"MMR2";
/// A biome cell's side in blocks, as the game keeps biomes.
pub const BIOME_CELL: i32 = 4;
const BIOME_SIDE: usize = (REGION / BIOME_CELL) as usize;
const BIOME_CELLS: usize = BIOME_SIDE * BIOME_SIDE;

pub struct Region {
    pub color: Vec<u8>,
    pub height: Vec<i16>,
    pub depth: Vec<u8>,
    /// An index into the legend's tints for each column the game tints by biome, else 0.
    /// Not there until a column has one.
    pub tint: Option<Vec<u8>>,
    /// An index into the legend's biomes for each 4 x 4 cell, 0 where it wasn't read.
    pub biome: Vec<u8>,
    pub dirty: bool,
    used: u64,
}

impl Region {
    fn empty() -> Self {
        Self {
            color: vec![UNSEEN; CELLS],
            height: vec![0; CELLS],
            depth: vec![0; CELLS],
            tint: None,
            biome: vec![0; BIOME_CELLS],
            dirty: false,
            used: 0,
        }
    }

    fn encode(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(64 * 1024);
        out.extend_from_slice(MAGIC);
        pack(&self.color, &mut out);
        pack(&self.depth, &mut out);
        // Heights: the high byte barely changes; the low byte as a difference from the column
        // to the west is mostly 0 or +-1.
        let high: Vec<u8> = self.height.iter().map(|h| (*h >> 8) as u8).collect();
        pack(&high, &mut out);
        let mut prev = 0u8;
        let low: Vec<u8> = self
            .height
            .iter()
            .map(|h| {
                let b = *h as u8;
                let d = b.wrapping_sub(prev);
                prev = b;
                d
            })
            .collect();
        pack(&low, &mut out);
        out.push(u8::from(self.tint.is_some()));
        if let Some(tint) = &self.tint {
            pack(tint, &mut out);
        }
        pack(&self.biome, &mut out);
        out
    }

    /// Reads either format; a 0.2 file comes back with no tints and no biomes, and is written
    /// in the new format the next time it changes.
    fn decode(bytes: &[u8]) -> Option<Self> {
        let (mut at, v2) = match (bytes.strip_prefix(MAGIC), bytes.strip_prefix(MAGIC_V1)) {
            (Some(at), _) => (at, true),
            (_, Some(at)) => (at, false),
            _ => return None,
        };
        let color = unpack(&mut at, CELLS)?;
        let depth = unpack(&mut at, CELLS)?;
        let high = unpack(&mut at, CELLS)?;
        let low = unpack(&mut at, CELLS)?;
        let (mut tint, mut biome) = (None, vec![0; BIOME_CELLS]);
        if v2 {
            let (flags, rest) = at.split_first()?;
            at = rest;
            if flags & 1 != 0 {
                tint = Some(unpack(&mut at, CELLS)?);
            }
            biome = unpack(&mut at, BIOME_CELLS)?;
        }
        let mut prev = 0u8;
        let height = high
            .iter()
            .zip(&low)
            .map(|(h, d)| {
                prev = prev.wrapping_add(*d);
                i16::from_be_bytes([*h, prev])
            })
            .collect();
        Some(Self {
            color,
            height,
            depth,
            tint,
            biome,
            dirty: false,
            used: 0,
        })
    }
}

/// PackBits-style runs: a control byte under 128 is that many + 1 literal bytes, 128 and over
/// repeats the next byte (control - 126) times. Prefixed with the packed length.
fn pack(plane: &[u8], out: &mut Vec<u8>) {
    let start = out.len();
    out.extend_from_slice(&[0; 4]);
    let mut i = 0;
    while i < plane.len() {
        let b = plane[i];
        let mut run = 1;
        while run < 129 && i + run < plane.len() && plane[i + run] == b {
            run += 1;
        }
        if run >= 3 {
            out.push((run + 126) as u8);
            out.push(b);
            i += run;
            continue;
        }
        let from = i;
        i += 1;
        while i < plane.len() && i - from < 128 {
            if i + 2 < plane.len() && plane[i] == plane[i + 1] && plane[i] == plane[i + 2] {
                break;
            }
            i += 1;
        }
        out.push((i - from - 1) as u8);
        out.extend_from_slice(&plane[from..i]);
    }
    let len = (out.len() - start - 4) as u32;
    out[start..start + 4].copy_from_slice(&len.to_le_bytes());
}

fn unpack(at: &mut &[u8], cells: usize) -> Option<Vec<u8>> {
    let len = u32::from_le_bytes(at.get(..4)?.try_into().ok()?) as usize;
    let mut src = at.get(4..4 + len)?;
    *at = &at[4 + len..];
    let mut out = Vec::with_capacity(cells);
    while let Some((&control, rest)) = src.split_first() {
        if control < 128 {
            let n = control as usize + 1;
            out.extend_from_slice(rest.get(..n)?);
            src = &rest[n..];
        } else {
            let b = *rest.first()?;
            out.resize(out.len() + control as usize - 126, b);
            src = &rest[1..];
        }
    }
    (out.len() == cells).then_some(out)
}

#[inline]
pub fn split(x: i32, z: i32) -> ((i32, i32), usize) {
    let key = (x.div_euclid(REGION), z.div_euclid(REGION));
    let i = (z.rem_euclid(REGION) * REGION + x.rem_euclid(REGION)) as usize;
    (key, i)
}

/// The mod's data files. The game hands out one for the life of the process, and a wasm mod
/// runs on one thread.
#[derive(Clone, Copy)]
struct Files(&'static dyn ConfigFiles);
// SAFETY: wasm mods are single-threaded (threads and shared memory are off for them); the
// bounds are only there because `Hooks` asks for them.
unsafe impl Send for Files {}
unsafe impl Sync for Files {}

pub struct Store {
    /// At most this many regions stay in memory (about 1.3 MiB each).
    keep: usize,
    files: Option<Files>,
    /// `<world>/<dimension>`, the folder the regions are in; none with no world open.
    dir: Option<String>,
    regions: HashMap<(i32, i32), Region>,
    /// Regions that have a file.
    on_disk: HashSet<(i32, i32)>,
    /// Regions asked for and not read yet.
    wanted: Vec<(i32, i32)>,
    clock: u64,
    /// Bumped whenever a column or a loaded region changes: what was drawn from it is stale.
    pub revision: u64,
}

impl Default for Store {
    fn default() -> Self {
        Self::keeping(64)
    }
}

impl Store {
    pub fn keeping(keep: usize) -> Self {
        Self {
            keep,
            files: None,
            dir: None,
            regions: HashMap::new(),
            on_disk: HashSet::new(),
            wanted: Vec::new(),
            clock: 0,
            revision: 0,
        }
    }

    /// Open with no files behind it: for tests.
    #[cfg(test)]
    pub fn open_memory(&mut self) {
        self.close();
        self.dir = Some("mem".into());
    }

    /// Reads now every region with a file that the blocks from `min` to `max` (x, z) are in,
    /// while `more` allows. False when it had to stop before all were in memory.
    pub fn load_area(&mut self, min: [i32; 2], max: [i32; 2], mut more: impl FnMut() -> bool) -> bool {
        for rz in min[1].div_euclid(REGION)..=max[1].div_euclid(REGION) {
            for rx in min[0].div_euclid(REGION)..=max[0].div_euclid(REGION) {
                let key = (rx, rz);
                if self.on_disk.contains(&key) && !self.regions.contains_key(&key) {
                    if !more() {
                        return false;
                    }
                    self.load(key);
                }
            }
        }
        true
    }

    /// How many regions are queued to be read.
    pub fn waiting(&self) -> usize {
        self.wanted.len()
    }

    /// Every region there is, in memory or in a file.
    pub fn known(&self) -> Vec<(i32, i32)> {
        let mut keys: HashSet<(i32, i32)> = self.on_disk.clone();
        keys.extend(self.regions.keys().copied());
        let mut keys: Vec<_> = keys.into_iter().collect();
        keys.sort();
        keys
    }

    /// The region, read from its file now when it isn't in memory.
    pub fn load(&mut self, key: (i32, i32)) -> Option<&Region> {
        if !self.regions.contains_key(&key) {
            if !self.on_disk.contains(&key) {
                return None;
            }
            let region = self.read(key)?;
            self.insert(key, region);
        }
        self.touch(key);
        self.regions.get(&key)
    }

    /// Opens `dir`'s regions, forgetting the ones in memory (save them first).
    pub fn open(&mut self, files: &'static dyn ConfigFiles, dir: String) {
        self.close();
        if let Ok(folder) = ConfigPath::new(&dir) {
            let mut after: Option<String> = None;
            while let Ok(page) = files.list(&folder, after.as_deref(), 512) {
                let Some(last) = page.last() else { break };
                after = Some(last.name.clone());
                for entry in &page {
                    if let Some(key) = parse_name(&entry.name) {
                        self.on_disk.insert(key);
                    }
                }
            }
        }
        self.files = Some(Files(files));
        self.dir = Some(dir);
        self.revision += 1;
    }

    /// Writes what changed and forgets everything.
    pub fn close(&mut self) {
        // What the caller has not written by now gets a few regions' worth of time, no more.
        self.save(6);
        self.regions.clear();
        self.on_disk.clear();
        self.wanted.clear();
        self.dir = None;
        self.revision += 1;
    }

    pub fn is_open(&self) -> bool {
        self.dir.is_some()
    }

    /// The region in memory, if it is. One with a file that isn't read yet is queued for
    /// [`pump`](Self::pump) and answers None until then.
    pub fn region(&self, key: (i32, i32)) -> Option<&Region> {
        self.regions.get(&key)
    }

    /// Queues `key` for reading when it has a file and isn't in memory.
    pub fn want(&mut self, key: (i32, i32)) {
        if self.on_disk.contains(&key) && !self.regions.contains_key(&key) && !self.wanted.contains(&key) {
            self.wanted.push(key);
        }
    }

    /// Marks the regions in memory around `keys` as in use, so they aren't dropped.
    pub fn touch(&mut self, key: (i32, i32)) {
        self.clock += 1;
        if let Some(region) = self.regions.get_mut(&key) {
            region.used = self.clock;
        }
    }

    /// Reads up to `budget` queued regions. True when one came in.
    pub fn pump(&mut self, budget: usize) -> bool {
        let mut loaded = false;
        for _ in 0..budget {
            let Some(key) = self.wanted.pop() else { break };
            if self.regions.contains_key(&key) {
                continue;
            }
            if let Some(region) = self.read(key) {
                self.insert(key, region);
                loaded = true;
            } else {
                self.on_disk.remove(&key);
            }
        }
        if loaded {
            self.revision += 1;
        }
        loaded
    }

    fn path(&self, key: (i32, i32)) -> Option<ConfigPath> {
        ConfigPath::new(&format!("{}/r.{}.{}.bin", self.dir.as_ref()?, key.0, key.1)).ok()
    }

    fn read(&self, key: (i32, i32)) -> Option<Region> {
        let bytes = self.files?.0.read(&self.path(key)?).ok()?;
        Region::decode(&bytes)
    }

    fn insert(&mut self, key: (i32, i32), mut region: Region) {
        self.clock += 1;
        region.used = self.clock;
        self.regions.insert(key, region);
        if self.regions.len() > self.keep {
            self.evict();
        }
    }

    /// Drops the regions used longest ago, writing the changed ones first.
    fn evict(&mut self) {
        let mut by_age: Vec<((i32, i32), u64)> = self.regions.iter().map(|(k, r)| (*k, r.used)).collect();
        by_age.sort_by_key(|e| e.1);
        for (key, _) in by_age.into_iter().take(self.regions.len().saturating_sub(self.keep.saturating_sub(8))) {
            self.save_region(key);
            self.regions.remove(&key);
        }
    }

    /// The region for writing a scanned column: read from its file or made empty.
    fn region_mut(&mut self, key: (i32, i32)) -> &mut Region {
        if !self.regions.contains_key(&key) {
            let region = if self.on_disk.contains(&key) { self.read(key) } else { None };
            self.insert(key, region.unwrap_or_else(Region::empty));
        }
        self.clock += 1;
        let region = self.regions.get_mut(&key).expect("just inserted");
        region.used = self.clock;
        region
    }

    /// Writes one scanned chunk's 256 columns (`z * 16 + x`). True when anything changed.
    /// `tint` is left as it was when None.
    pub fn put_chunk(
        &mut self,
        chunk: (i32, i32),
        color: &[u8; 256],
        height: &[i16; 256],
        depth: &[u8; 256],
        tint: Option<&[u8; 256]>,
    ) -> bool {
        if !self.is_open() {
            return false;
        }
        let (key, base) = split(chunk.0 * 16, chunk.1 * 16);
        let region = self.region_mut(key);
        let mut changed = false;
        for z in 0..16 {
            let row = base + z * REGION as usize;
            let src = z * 16;
            if region.color[row..row + 16] != color[src..src + 16]
                || region.height[row..row + 16] != height[src..src + 16]
                || region.depth[row..row + 16] != depth[src..src + 16]
            {
                region.color[row..row + 16].copy_from_slice(&color[src..src + 16]);
                region.height[row..row + 16].copy_from_slice(&height[src..src + 16]);
                region.depth[row..row + 16].copy_from_slice(&depth[src..src + 16]);
                changed = true;
            }
            let Some(tint) = tint else { continue };
            let any = tint[src..src + 16].iter().any(|t| *t != 0);
            if region.tint.is_none() && any {
                region.tint = Some(vec![0; CELLS]);
            }
            if let Some(plane) = &mut region.tint
                && plane[row..row + 16] != tint[src..src + 16]
            {
                plane[row..row + 16].copy_from_slice(&tint[src..src + 16]);
                changed = true;
            }
        }
        if changed {
            region.dirty = true;
            self.revision += 1;
        }
        changed
    }

    /// Sets the biome of the 4 x 4 cell `x`, `z` is in.
    pub fn put_biome(&mut self, x: i32, z: i32, biome: u8) {
        if !self.is_open() {
            return;
        }
        let (key, i) = biome_split(x, z);
        let region = self.region_mut(key);
        if region.biome[i] != biome {
            region.biome[i] = biome;
            region.dirty = true;
        }
    }

    /// The legend index of the biome at `x`, `z`: 0 when it wasn't read or isn't in memory.
    pub fn biome(&self, x: i32, z: i32) -> u8 {
        let (key, i) = biome_split(x, z);
        self.regions.get(&key).map_or(0, |r| r.biome[i])
    }

    /// The legend index of the tint at `x`, `z`, 0 for none.
    pub fn tint(&self, x: i32, z: i32) -> u8 {
        let (key, i) = split(x, z);
        self.regions.get(&key).and_then(|r| r.tint.as_ref()).map_or(0, |t| t[i])
    }

    /// One column as stored: colour, height, depth. None while its region isn't in memory.
    pub fn column(&self, x: i32, z: i32) -> Option<(u8, i16, u8)> {
        let (key, i) = split(x, z);
        let region = self.regions.get(&key)?;
        Some((region.color[i], region.height[i], region.depth[i]))
    }

    fn save_region(&mut self, key: (i32, i32)) -> bool {
        let Some(region) = self.regions.get(&key) else { return false };
        if !region.dirty {
            return false;
        }
        let (Some(files), Some(path)) = (self.files, self.path(key)) else { return false };
        let ok = files.0.write(&path, &region.encode()).is_ok();
        if ok {
            self.on_disk.insert(key);
            if let Some(region) = self.regions.get_mut(&key) {
                region.dirty = false;
            }
        }
        ok
    }

    /// Writes up to `budget` changed regions. Returns how many it wrote.
    pub fn save(&mut self, budget: usize) -> usize {
        let dirty: Vec<(i32, i32)> = self
            .regions
            .iter()
            .filter(|(_, r)| r.dirty)
            .map(|(k, _)| *k)
            .take(budget)
            .collect();
        dirty.into_iter().filter(|key| self.save_region(*key)).count()
    }
}

fn biome_split(x: i32, z: i32) -> ((i32, i32), usize) {
    let key = (x.div_euclid(REGION), z.div_euclid(REGION));
    let cell = |v: i32| (v.rem_euclid(REGION) / BIOME_CELL) as usize;
    (key, cell(z) * BIOME_SIDE + cell(x))
}

/// What the tint and biome numbers in the regions stand for, one table per world, in
/// `<world>/legend.txt`: a line `b <biome id>` or `t <RRGGBB>` per entry, in index order from 1.
#[derive(Default)]
pub struct Legend {
    biomes: Vec<String>,
    tints: Vec<[u8; 3]>,
    pub dirty: bool,
    /// Bumped when a tint is added.
    pub revision: u64,
}

impl Legend {
    pub fn decode(text: &str) -> Self {
        let mut legend = Self::default();
        for line in text.lines() {
            match line.split_once('\t') {
                Some(("b", id)) if legend.biomes.len() < 255 => legend.biomes.push(id.to_string()),
                Some(("t", hex)) if legend.tints.len() < 255 => {
                    let rgb = u32::from_str_radix(hex, 16).unwrap_or(0);
                    legend.tints.push([(rgb >> 16) as u8, (rgb >> 8) as u8, rgb as u8]);
                }
                _ => {}
            }
        }
        legend
    }

    pub fn encode(&self) -> String {
        let mut out = String::from("# atlas legend v1: what the biome (b) and tint (t) numbers in the regions mean, from 1\n");
        for id in &self.biomes {
            out.push_str(&format!("b\t{id}\n"));
        }
        for [r, g, b] in &self.tints {
            out.push_str(&format!("t\t{r:02X}{g:02X}{b:02X}\n"));
        }
        out
    }

    /// The index of biome `id`, added when new; 0 once 255 are taken.
    pub fn biome_index(&mut self, id: &str) -> u8 {
        if let Some(i) = self.biomes.iter().position(|b| b == id) {
            return i as u8 + 1;
        }
        if self.biomes.len() >= 255 {
            return 0;
        }
        self.biomes.push(id.to_string());
        self.dirty = true;
        self.biomes.len() as u8
    }

    pub fn biome_id(&self, index: u8) -> Option<&str> {
        self.biomes.get((index as usize).checked_sub(1)?).map(String::as_str)
    }

    /// The index of tint `rgb` (0xRRGGBB), rounded to steps of 4 a channel so blends between
    /// biomes share entries; added when new, the nearest one once 255 are taken.
    pub fn tint_index(&mut self, rgb: u32) -> u8 {
        let q = |shift: u32| (((rgb >> shift) & 255) as u8) & 0xFC | 2;
        let want = [q(16), q(8), q(0)];
        if let Some(i) = self.tints.iter().position(|t| *t == want) {
            return i as u8 + 1;
        }
        if self.tints.len() >= 255 {
            let far = |t: &[u8; 3]| (0..3).map(|c| (i32::from(t[c]) - i32::from(want[c])).pow(2)).sum::<i32>();
            return self.tints.iter().enumerate().min_by_key(|(_, t)| far(t)).map_or(0, |(i, _)| i as u8 + 1);
        }
        self.tints.push(want);
        self.dirty = true;
        self.revision += 1;
        self.tints.len() as u8
    }

    /// The tints by index, entry 0 unused.
    pub fn tints(&self) -> impl Iterator<Item = [u8; 3]> + '_ {
        std::iter::once([0; 3]).chain(self.tints.iter().copied())
    }
}

fn parse_name(name: &str) -> Option<(i32, i32)> {
    let mut parts = name.strip_prefix("r.")?.strip_suffix(".bin")?.split('.');
    let key = (parts.next()?.parse().ok()?, parts.next()?.parse().ok()?);
    parts.next().is_none().then_some(key)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_region_survives_its_file() {
        let mut region = Region::empty();
        for i in 0..CELLS {
            region.color[i] = ((i / 700) % 62) as u8;
            region.height[i] = ((i % 512) as i16 - 64) + ((i / 9000) as i16);
            region.depth[i] = if i % 5000 < 300 { (i % 11) as u8 } else { 0 };
        }
        region.height[77] = -64;
        region.height[78] = 319;
        // The 0.2 format: the planes up to the heights.
        let mut old = region.encode();
        old.truncate(old.len() - 1 - 4 - packed_len(&region.biome));
        old[..4].copy_from_slice(MAGIC_V1);
        let back = Region::decode(&old).expect("reads a 0.2 file");
        assert_eq!(back.color, region.color);
        assert_eq!(back.height, region.height);
        assert!(back.tint.is_none() && back.biome.iter().all(|b| *b == 0));

        let mut tint = vec![0u8; CELLS];
        tint[5000..9000].fill(3);
        region.tint = Some(tint);
        region.biome[77] = 9;
        let bytes = region.encode();
        assert!(bytes.len() < CELLS * 2, "{}", bytes.len());
        let back = Region::decode(&bytes).expect("decodes");
        assert_eq!(back.color, region.color);
        assert_eq!(back.height, region.height);
        assert_eq!(back.depth, region.depth);
        assert_eq!(back.tint, region.tint);
        assert_eq!(back.biome, region.biome);
        assert!(Region::decode(&bytes[..bytes.len() - 3]).is_none());
        assert!(Region::decode(b"nope").is_none());
    }

    fn packed_len(plane: &[u8]) -> usize {
        let mut out = Vec::new();
        pack(plane, &mut out);
        out.len() - 4
    }

    #[test]
    fn the_legend_keeps_its_numbers() {
        let mut legend = Legend::default();
        assert_eq!(legend.biome_index("minecraft:plains"), 1);
        assert_eq!(legend.biome_index("minecraft:swamp"), 2);
        assert_eq!(legend.biome_index("minecraft:plains"), 1);
        let grass = legend.tint_index(0x91BD59);
        assert_eq!(legend.tint_index(0x90BC58), grass, "close colours share an entry");
        assert_ne!(legend.tint_index(0x6A7039), grass);
        let back = Legend::decode(&legend.encode());
        assert_eq!(back.biome_id(2), Some("minecraft:swamp"));
        assert_eq!(back.tints().collect::<Vec<_>>(), legend.tints().collect::<Vec<_>>());
        for i in 0..400u32 {
            assert_ne!(legend.tint_index(i * 0x030507), 0);
        }
        assert_eq!(legend.tints().count(), 256);
    }

    #[test]
    fn an_empty_region_is_tiny() {
        assert!(Region::empty().encode().len() < 20_000);
    }

    #[test]
    fn names_and_cells() {
        assert_eq!(parse_name("r.-3.12.bin"), Some((-3, 12)));
        assert_eq!(parse_name("r.1.bin"), None);
        assert_eq!(parse_name("waypoints.txt"), None);
        assert_eq!(split(-1, -1), ((-1, -1), CELLS - 1));
        assert_eq!(split(512, 0), ((1, 0), 0));
    }
}

