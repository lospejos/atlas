//! Test builds only (`TEST` in lib.rs): measures what the hidden tests cannot see from
//! outside and writes it to `probe.log` in the mod's data folder.
use mod_api::{ClientHost, ConfigPath, LevelRef, LightLayer};

use crate::mapview::View;
use crate::render::{self, Frame};
use crate::{State, Ui};

#[derive(Default)]
pub struct Probe {
    log: String,
    frames: u32,
    moved: u32,
    turned: u32,
    zoomed: u32,
    last: (f64, f64, f32, f32),
    /// A drag under way: frames, drag events, frames without an event, frames with several,
    /// the worst distance in GUI pixels between the grabbed block and the cursor, the longest
    /// frame in milliseconds.
    drag: Option<DragProbe>,
    events_this_frame: u32,
    delta_this_frame: [f32; 2],
    /// The worst distance between the cursor the frame hook read and the one the draw got.
    cursor_gap: f32,
    cells_max: u32,
    trace: String,
    /// A layer switch under way: frames drawn since the layer changed, when it began, and the
    /// longest of those frames in milliseconds.
    switch: Option<(u32, f64, f64)>,
    /// Milliseconds: the longest save; the client tick hook's sum, longest and count; the
    /// frame hook's.
    pub save_ms: u64,
    pub tick_ms: (u64, u64, u64),
    pub frame_ms: (u64, u64, u64),
    api_done: bool,
    timed: u8,
    /// A slide or glide under way: which, its frames, the frames the view moved in, seconds.
    motion: Option<Motion>,
    pub drive: Option<Drive>,
    /// Paint as 0.2 did: whenever the view leaves a 48-texel margin, at once.
    pub v02: bool,
    pub press: [f32; 2],
    events: u32,
    delta_sum: [f32; 2],
}

/// Which motion, its frames, the frames the view moved in, its seconds, where the view was.
type Motion = (&'static str, u32, u32, f64, (f64, f64));

#[derive(Default, Clone, Copy)]
struct DragProbe {
    frames: u32,
    events: u32,
    quiet: u32,
    busy: u32,
    anchor_err: f64,
    /// Where 0.2 would have the view: the centre at the press less every drag event's delta.
    ghost: [f64; 2],
    ghost_err: f64,
    ghost_lag: f64,
    dt_max: f64,
    /// Frames that took over 3 ms, and the frame times added up.
    slow: u32,
    dt_sum: f64,
    uploads: u64,
    texels: u64,
    cells_max: u32,
}

/// A drag the test build makes itself, a cursor position a frame: the control socket's own
/// drag presses, moves and releases inside one frame.
#[derive(Clone, Copy)]
pub struct Drive {
    start: f64,
}

impl Probe {
    pub fn drag_event(&mut self, delta: [f32; 2]) {
        self.events_this_frame += 1;
        self.events += 1;
        self.delta_sum[0] += delta[0];
        self.delta_sum[1] += delta[1];
        self.delta_this_frame[0] += delta[0];
        self.delta_this_frame[1] += delta[1];
    }
}

impl State {
    fn probe_write(&mut self, host: &dyn ClientHost, line: String) {
        self.probe.log.push_str(&line);
        self.probe.log.push('\n');
        if let Ok(path) = ConfigPath::new("probe.log") {
            let _ = host.data_files().write(&path, self.probe.log.as_bytes());
        }
    }

    /// A key of the test build on the world map: K drags the map by itself for 1.6 seconds,
    /// L the same with 0.2's painting.
    pub(crate) fn probe_key(&mut self, host: &dyn ClientHost, short: &str) -> bool {
        // Settings the hidden tests cannot reach through the options screen.
        let setting = match short {
            "r" => Some("minimap_round"),
            "t" => Some("minimap_rotate"),
            "o" => Some("other_coords"),
            "a" => Some("arrow"),
            _ => None,
        };
        if let Some(key) = setting {
            self.flip(host, key);
            let line = format!("setting {key} now {:?}", host.setting(crate::ID, key));
            self.probe_write(host, line);
            return true;
        }
        if matches!(short, "g" | "s" | "x" | "f" | "u") {
            let ex = &mut self.explore;
            match short {
                "g" => ex.grid = !ex.grid,
                "s" => ex.slime = !ex.slime,
                "x" => ex.structures = !ex.structures,
                "f" => ex.panel = !ex.panel,
                _ => ex.remote = !ex.remote,
            }
            ex.flip_set(usize::MAX);
            return true;
        }
        if short == "h" {
            // The nearest structure marker to the middle of the screen.
            if let Some(view) = &mut self.view
                && let Some(pos) = self.explore.nearest(view.cx, view.cz)
            {
                (view.cx, view.cz) = (f64::from(pos[0]) + 0.5, f64::from(pos[2]) + 0.5);
            }
            return true;
        }
        if short == "i" {
            host.set_setting(crate::ID, "radar_style", if self.config.icons { "1" } else { "0" });
            self.read_config(host);
            return true;
        }
        if short == "c" {
            let next = (self.config.corner + 1) % 4;
            host.set_setting(crate::ID, "minimap_corner", &next.to_string());
            self.read_config(host);
            let line = format!("setting minimap_corner now {:?} choice {:?}", host.setting(crate::ID, "minimap_corner"), host.setting_choice("minimap_corner"));
            self.probe_write(host, line);
            return true;
        }
        if !matches!(short, "k" | "l") {
            return false;
        }
        self.probe.v02 = short == "l";
        self.image = None;
        self.probe.drive = Some(Drive { start: self.clock + 0.3 });
        true
    }

    /// Moves the cursor of a self-made drag on by a frame.
    pub(crate) fn probe_drive(&mut self, host: &dyn ClientHost) {
        let Some(drive) = self.probe.drive else { return };
        let t = self.clock - drive.start;
        let [w, h] = self.gui;
        // Two wide sweeps and back, fast in the middle, 1.6 seconds.
        let turn = (t / 1.6 * std::f64::consts::TAU) as f32;
        let at = [w * 0.4 + w * 1.2 * turn.sin(), h * 0.5 + h * 0.8 * (turn * 2.0).sin()];
        if t < 0.0 {
            return;
        }
        if self.drag.is_none() && t < 1.6 {
            self.begin_drag([w * 0.4, h * 0.5]);
        }
        if t >= 1.6 {
            self.probe.drive = None;
            self.cursor = [w * 0.4, h * 0.5];
            self.end_drag();
            self.glide = None;
            let _ = host;
            return;
        }
        self.cursor = at;
    }

    /// At a real release: what the drag's events added up to against where the cursor went.
    pub(crate) fn probe_release(&mut self, host: &dyn ClientHost, mouse: [f32; 2]) {
        if self.drag.is_none() {
            return;
        }
        let (events, sum) = (std::mem::take(&mut self.probe.events), std::mem::take(&mut self.probe.delta_sum));
        let went = [mouse[0] - self.probe.press[0], mouse[1] - self.probe.press[1]];
        let under = self.view.map(|v| self.block_at(v, mouse));
        let line = format!(
            "release: {events} drag events, their deltas add up to {sum:?}, the cursor went {went:?}; \
             block under the cursor {:?}, grabbed {:?}",
            under.map(|b| b.map(|v| (v * 100.0).round() / 100.0)),
            self.drag.map(|d| d.block.map(|v| (v * 100.0).round() / 100.0)),
        );
        self.probe_write(host, line);
    }

    pub(crate) fn probe_note(&mut self, host: &dyn ClientHost, line: String) {
        self.probe_write(host, line);
    }

    pub(crate) fn probe_draw(&mut self, mouse: [f32; 2]) {
        let gap = (mouse[0] - self.cursor[0]).abs().max((mouse[1] - self.cursor[1]).abs());
        self.probe.cursor_gap = self.probe.cursor_gap.max(gap);
    }

    /// Counts, over each 120 frames, how many drew the player somewhere new, turned anew and at
    /// a new map zoom; and follows a drag frame by frame.
    pub(crate) fn probe_frame(&mut self, host: &mut dyn ClientHost, dt: f64) {
        let Some(p) = self.player else { return };
        let now = (p.pos[0], p.pos[2], p.yaw, self.view.map_or(0.0, |v| v.zoom));
        let c = &mut self.probe;
        c.frames += 1;
        c.moved += u32::from(now.0 != c.last.0 || now.1 != c.last.1);
        c.turned += u32::from(now.2 != c.last.2);
        c.zoomed += u32::from(now.3 != c.last.3);
        c.last = now;
        c.cells_max = c.cells_max.max(self.stats.cells);
        if c.frames >= 120 {
            let line = format!(
                "frames {} moved {} turned {} zoomed {} cursor_gap {:.2} soft_uploads {} mosaic_cells_max {} \
                 uploads_while_panning {}",
                c.frames, c.moved, c.turned, c.zoomed, c.cursor_gap, self.soft.uploads, c.cells_max,
                self.stats.while_panning
            );
            (c.frames, c.moved, c.turned, c.zoomed, c.cells_max) = (0, 0, 0, 0, 0);
            self.probe_write(host, line);
        }
        // From the frame the shown layer changes to the frame its image is in the slot.
        let want = self.look();
        let have = self.image.map(|i| i.look);
        if have.is_some() && have != Some(want) {
            let s = self.probe.switch.get_or_insert((0, self.clock, 0.0));
            s.0 += 1;
            s.2 = s.2.max(dt * 1000.0);
        } else if let Some((frames, began, worst)) = self.probe.switch.take() {
            let full = self.image.is_some_and(|i| i.full);
            let line = format!(
                "layer switch to {:?} ({}): image in the slot {} frames after the change ({:.0} ms), longest frame {:.1} ms, \
                 regions still to read {}",
                want.cave,
                if full { "world map" } else { "minimap" },
                frames,
                (self.clock - began) * 1000.0,
                worst.max(dt * 1000.0),
                self.shown().waiting()
            );
            self.probe_write(host, line);
        }
        let events = std::mem::take(&mut self.probe.events_this_frame);
        let delta = std::mem::take(&mut self.probe.delta_this_frame);
        match (self.drag, self.view) {
            (Some(drag), Some(view)) => {
                let zoom = f64::from(view.zoom);
                let under = self.block_at(view, self.cursor);
                let d = self.probe.drag.get_or_insert_with(|| DragProbe {
                    ghost: [view.cx, view.cz],
                    uploads: self.stats.uploads,
                    texels: self.stats.texels,
                    ..DragProbe::default()
                });
                d.frames += 1;
                d.events += events;
                d.quiet += u32::from(events == 0);
                d.busy += u32::from(events > 1);
                d.dt_max = d.dt_max.max(dt * 1000.0);
                d.slow += u32::from(dt > 0.003);
                d.dt_sum += dt;
                d.cells_max = d.cells_max.max(self.stats.cells);
                // 0.2's way: the view moved by each event's delta.
                d.ghost[0] -= f64::from(delta[0]) / zoom;
                d.ghost[1] -= f64::from(delta[1]) / zoom;
                let err = ((under[0] - drag.block[0]).powi(2) + (under[1] - drag.block[1]).powi(2)).sqrt() * zoom;
                let ghost = ((d.ghost[0] - view.cx).powi(2) + (d.ghost[1] - view.cz).powi(2)).sqrt() * zoom;
                d.anchor_err = d.anchor_err.max(err);
                d.ghost_err = d.ghost_err.max(ghost);
                d.ghost_lag = ghost;
            }
            _ => {
                if let Some(d) = self.probe.drag.take() {
                    let line = format!(
                        "drag ({}): frames {} in {:.0} ms, anchor_err_max_px {:.4}, frame_max_ms {:.1}, \
                         frames_over_3ms {}, uploads {} ({} texels), mosaic_cells_max {}, events {} glide {}",
                        if self.probe.v02 { "0.2 painting" } else { "0.3" },
                        d.frames,
                        d.dt_sum * 1000.0,
                        d.anchor_err,
                        d.dt_max,
                        d.slow,
                        self.stats.uploads - d.uploads,
                        self.stats.texels - d.texels,
                        d.cells_max,
                        d.events,
                        self.glide.is_some(),
                    );
                    self.probe_write(host, line);
                    self.probe.v02 = false;
                }
            }
        }
        let moving = if self.slide.is_some() || self.goto.is_some() {
            Some("slide")
        } else if self.glide.is_some() {
            Some("glide")
        } else {
            None
        };
        let at = self.view.map_or((0.0, 0.0), |v| (v.cx, v.cz));
        match (moving, &mut self.probe.motion) {
            (Some(kind), None) => self.probe.motion = Some((kind, 1, 1, dt, at)),
            (Some(_), Some(m)) => {
                m.1 += 1;
                m.2 += u32::from(m.4 != at);
                m.3 += dt;
                m.4 = at;
            }
            (None, Some(m)) => {
                let line = format!("{}: frames {} moved_in {} lasted {:.0} ms", m.0, m.1, m.2, m.3 * 1000.0);
                self.probe.motion = None;
                self.probe_write(host, line);
            }
            (None, None) => {}
        }
        // With the map open, a step a frame (a hook call may take 100 ms at most): what
        // uploading an image of each size costs, and what painting the largest does.
        const SIZES: [(usize, usize); 6] = [(64, 64), (192, 192), (256, 256), (512, 512), (1024, 512), (1024, 1024)];
        let stage = self.probe.timed as usize;
        if stage <= SIZES.len() && matches!(self.ui, Ui::Map) && self.image.is_some_and(|i| i.full)
            && self.drag.is_none()
            && self.probe.drive.is_none()
        {
            self.probe.timed += 1;
            let Some(view) = self.view else { return };
            let base = self.map_frame(View { zoom: 1.0, ..view }, [0.0; 2], 48);
            let line = if let Some((w, h)) = SIZES.get(stage).copied() {
                let frame = Frame { w, h, ..base };
                let mut pixels = Vec::new();
                render::compose(&mut self.surface, &self.palette, &[], frame, &mut pixels);
                let start = host.millis();
                let times = if w * h > 300_000 { 3 } else { 8 };
                for _ in 0..times {
                    host.mod_image(w as u32, h as u32, &pixels);
                }
                format!("upload {w} x {h}: {:.2} ms", (host.millis() - start) as f64 / times as f64)
            } else {
                let frame = Frame { w: 1024, h: 1024, ..base };
                let mut pixels = Vec::new();
                let start = host.millis();
                for _ in 0..4 {
                    render::compose(&mut self.surface, &self.palette, &[], frame, &mut pixels);
                }
                format!("compose 1024 x 1024: {:.2} ms (gui {:?})", (host.millis() - start) as f64 / 4.0, self.gui)
            };
            self.probe_write(host, line);
            self.image = None;
        }
    }

    /// Once: what the client's reads answer. And every 200 ticks: what the scanning cost.
    pub(crate) fn probe_tick(&mut self, host: &dyn ClientHost, level: LevelRef, pos: [f64; 3]) {
        let tick = self.scan_surface.tick;
        if let Ok(trace) = crate::explore::TRACE.try_lock()
            && !trace.ends_with("done")
            && *trace != self.probe.trace
        {
            let line = trace.clone();
            drop(trace);
            self.probe.trace = line.clone();
            self.probe_write(host, format!("server: {line}"));
        }
        if tick.is_multiple_of(200) {
            let (s, c, r) = (&self.scan_surface, &self.scan_cave, &self.radar);
            let line = format!(
                "tick {tick}: surface calls {} peak {} chunks {} | cave {:?} calls {} peak {} chunks {} ticks {} | \
                 radar calls {} over {} ticks, blips {} | underground {} band {:?} | structures \
                 confirmed/possible/approximate {:?} pending {} | ms: tick hook avg {:.3} max {}, frame hook \
                 avg {:.4} max {}, save max {}",
                s.calls_total,
                s.calls_peak,
                s.chunks_read,
                c.layer,
                c.calls_total,
                c.calls_peak,
                c.chunks_read,
                c.tick,
                r.calls_total,
                r.ticks,
                r.blips.len(),
                self.underground,
                self.band,
                self.explore.counts(),
                self.explore.pending,
                self.probe.tick_ms.0 as f64 / self.probe.tick_ms.2.max(1) as f64,
                self.probe.tick_ms.1,
                self.probe.frame_ms.0 as f64 / self.probe.frame_ms.2.max(1) as f64,
                self.probe.frame_ms.1,
                self.probe.save_ms
            );
            self.probe.tick_ms = (0, 0, 0);
            self.probe.frame_ms = (0, 0, 0);
            self.probe.save_ms = 0;
            self.probe_write(host, line);
        }
        if self.probe.api_done || tick < 100 {
            return;
        }
        self.probe.api_done = true;
        let b = crate::block_of(pos);
        let mut lines = vec![format!(
            "api: dim {} type {:?} sky {:?} biome {:?}",
            self.dim,
            level.dimension_type(host).map(|t| (t.has_ceiling, t.has_skylight, t.min_y, t.logical_height)),
            level.brightness(host, LightLayer::Sky, [b[0], b[1] + 1, b[2]]),
            level.client_biome(host, b),
        )];
        let below = [b[0], b[1] - 1, b[2]];
        if let Some(state) = level.block_state(host, below) {
            lines.push(format!("api: tint of the block below {:?}", host.block_tint_color(state, 0, below)));
        }
        let found = level.entities_in(host, [pos[0] - 32.0, pos[1] - 16.0, pos[2] - 32.0], [pos[0] + 32.0, pos[1] + 16.0, pos[2] + 32.0]);
        lines.push(format!("api: entities_in {:?}", found.as_ref().map(Vec::len)));
        for e in found.unwrap_or_default().into_iter().take(8) {
            lines.push(format!(
                "api: entity {} kind {:?} name {:?} class {:?} pos {:?}",
                e.id(),
                e.kind(host).map(|k| k.id(host)),
                e.name(host),
                e.classification(host),
                e.position(host).map(|p| p.map(|v| v.round())),
            ));
        }
        // What a block read costs.
        let start = host.millis();
        let mut some = 0u32;
        for i in 0..100_000 {
            some += u32::from(level.block_state(host, [b[0] + (i & 15), b[1] - (i >> 4 & 31), b[2] + (i >> 9 & 15)]).is_some());
        }
        let took = host.millis() - start;
        lines.push(format!("api: 100000 block_state reads ({some} answered) in {took} ms"));
        for line in lines {
            self.probe_write(host, line);
        }
    }
}
