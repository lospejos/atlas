//! "PNG" on the world map: everything explored on the shown layer of this dimension, written
//! as one picture into the mod's data folder, a few rows a frame.
use mod_api::{ClientHost, ConfigPath, DATA_FILE_MAX};

use crate::State;
use crate::png::Png;
use crate::render::{self, Frame};
use crate::store::REGION;

/// The picture's longest side at most, in pixels: a wider area is drawn at 2, 4... blocks a pixel.
const SIDE_MAX: i32 = 8192;
/// Regions across at most: a row of them is in memory while it is drawn.
const REGIONS_MAX: i32 = 40;
/// Pixels drawn a frame.
const BUDGET: usize = 120_000;

pub struct Export {
    png: Png,
    frame: Frame,
    cave: Option<i32>,
    /// Under the mod's data folder.
    path: String,
    strip: Vec<u8>,
    /// How many of the next strip's regions are read: they come in over several frames.
    loaded: usize,
}

impl State {
    /// "Exporting… 40%", while one runs.
    pub(crate) fn export_progress(&self) -> Option<String> {
        let e = self.export.as_ref()?;
        Some(format!(
            "Exporting {} x {} px: {}%",
            e.frame.w,
            e.frame.h,
            e.png.rows() * 100 / e.frame.h.max(1)
        ))
    }

    /// Starts writing the shown layer's explored area, or stops the export that runs.
    pub(crate) fn start_export(&mut self) {
        if self.export.take().is_some() {
            self.note("Export stopped".into());
            return;
        }
        let cave = if self.shown_cave() { self.band } else { None };
        let keys = self.shown().known();
        let (Some(world), Some(first)) = (self.world.clone(), keys.first().copied()) else {
            self.note("Nothing explored here yet".into());
            return;
        };
        let (mut min, mut max) = (first, first);
        for key in &keys {
            min = (min.0.min(key.0), min.1.min(key.1));
            max = (max.0.max(key.0), max.1.max(key.1));
        }
        let across = max.0 - min.0 + 1;
        let (w, h) = (across * REGION, (max.1 - min.1 + 1) * REGION);
        if across > REGIONS_MAX {
            self.note(format!("Too wide to export: {w} blocks across (at most {})", REGIONS_MAX * REGION));
            return;
        }
        let mut step = 1;
        while w / step > SIDE_MAX || h / step > SIDE_MAX {
            step *= 2;
        }
        let frame =
            Frame { x0: min.0 * REGION, z0: min.1 * REGION, step, w: (w / step) as usize, h: (h / step) as usize };
        let layer = match (cave, self.dim_info.ceiling) {
            (Some(band), _) => format!("cave{band}"),
            (None, true) => "roof".to_string(),
            (None, false) => "surface".to_string(),
        };
        let clean = |s: &str| s.replace(|c: char| !c.is_ascii_alphanumeric() && c != '-', "_");
        let path = format!("export/{}-{}-{layer}.png", clean(&world), clean(&self.dim));
        self.export = Some(Export { png: Png::new(frame.w, frame.h), frame, cave, path, strip: Vec::new(), loaded: 0 });
    }

    /// Draws the next rows of a running export; writes the file when the last is in.
    pub(crate) fn export_step(&mut self, host: &dyn ClientHost) {
        let Some(mut e) = self.export.take() else { return };
        let layer = if self.shown_cave() { self.band } else { None };
        if layer != e.cave || !self.shown().is_open() {
            self.note("Export stopped: the layer changed".into());
            return;
        }
        let f = e.frame;
        let from = e.png.rows();
        let rows = (BUDGET / f.w.max(1)).clamp(1, 64).min(f.h - from);
        let strip = Frame { z0: f.z0 + from as i32 * f.step, h: rows, ..f };
        let until = self.until;
        let store = if e.cave.is_some() { &mut self.cave } else { &mut self.surface };
        // The regions these rows and the row north of them are in: read in the frames' time
        // for work (a region a frame at least), then painted. Each is read once per strip.
        let last_z = strip.z0 + (rows as i32 - 1) * f.step;
        let needed: Vec<(i32, i32)> = ((strip.z0 - f.step).div_euclid(REGION)..=last_z.div_euclid(REGION))
            .flat_map(|rz| {
                (f.x0.div_euclid(REGION)..=(f.x0 + (f.w as i32 - 1) * f.step).div_euclid(REGION))
                    .map(move |rx| (rx, rz))
            })
            .collect();
        let first = e.loaded;
        while e.loaded < needed.len() {
            if e.loaded > first && host.millis() >= until {
                self.export = Some(e);
                return;
            }
            store.load(needed[e.loaded]);
            e.loaded += 1;
        }
        if e.loaded > first && host.millis() >= until {
            // Read this frame: painted in the next.
            self.export = Some(e);
            return;
        }
        for key in &needed {
            store.touch(*key);
        }
        e.loaded = 0;
        let tints = std::mem::take(&mut self.tints);
        e.strip.clear();
        e.strip.resize(f.w * rows * 4, 0);
        let table: &[render::Tints] = if self.config.tints { &tints } else { &[] };
        render::compose_rows(store, &self.palette, table, strip, 0..rows, &mut e.strip);
        self.tints = tints;
        e.png.strip(&e.strip);
        if e.png.len() as u64 > DATA_FILE_MAX - (1 << 20) {
            self.note("Export stopped: the picture would be over 64 MiB".into());
            return;
        }
        if e.png.rows() < f.h {
            self.export = Some(e);
            return;
        }
        let (w, h, path) = (f.w, f.h, e.path.clone());
        let bytes = e.png.finish();
        let written = ConfigPath::new(&path).is_ok_and(|p| host.data_files().write(&p, &bytes).is_ok());
        let text = if written {
            format!(
                "Map saved: horizon-mod-data/atlas/{path} in the game folder ({w} x {h} px, {} KiB)",
                bytes.len() / 1024
            )
        } else {
            format!("Could not write {path}")
        };
        self.said.push(format!("[Atlas] {text}"));
        self.notice = Some((text, self.now + 15_000));
    }
}
