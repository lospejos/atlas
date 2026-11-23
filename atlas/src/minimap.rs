//! The minimap in a corner of the HUD, the waypoint labels in the world and the direction
//! arrow at the top of the screen.
//!
//! Square and north-up, the minimap is the image slot drawn as one sprite, scrolled by
//! fractions of a pixel as the player walks. Round or turning with the view, there is nothing
//! in the HUD's drawing that could cut or turn a sprite, so the picture is made here instead:
//! the north-up picture is kept in memory and each frame's view of it (turned, cut to a
//! circle) is written into the slot, two texels a GUI pixel.
use mod_api::{ClientHost, Corner, DrawContext, RenderGuiPostEvent};

use crate::gfx::{self, argb, centered, marker};
use crate::mapview::{Image, Look, MARGIN};
use crate::overlays::{OverlayMarker, OverlayQuery};
use crate::render::{self, Frame};
use crate::waypoints::distance_text;
use crate::{FRAME_COLOR, ID, MINIMAP_ZOOMS, Player, SPRITE, State, UNSEEN_COLOR};

/// With "names only on hover", a waypoint's label shows within this many GUI pixels of the
/// crosshair, sideways.
const AIM: f32 = 18.0;
/// The image is painted again after this many milliseconds even when nothing changed.
const REPAINT: u64 = 3000;

/// Seconds the minimap takes to grow to its enlarged size, or back.
const GROW_TIME: f32 = 0.14;
/// How many times larger the minimap is while the enlarge key is held (as far as the screen
/// has room). It shows more around you at the same zoom: a texel stays the size it was.
const GROW: f32 = 2.2;
/// The zoom steps' names, in the order of `MINIMAP_ZOOMS`.
const ZOOM_LABELS: [&str; 5] = ["4x", "2x", "1x", "1/2x", "1/4x"];

/// The minimap's keys and hiding, and the compass bar's place.
#[derive(Default)]
pub struct Hud {
    /// How far the minimap has grown, 0 to 1, and whether the enlarge key is held.
    pub grow: f32,
    pub held: bool,
    /// The zoom step the keys picked last, and when (frame clock), for the indicator.
    pub zoomed: Option<(usize, f64)>,
    /// The minimap was left out of the last draw (F3 in its corner, a screen open, F1).
    pub hidden: bool,
    /// GUI pixels the compass bar took at the top of the screen in the last draw: the boss
    /// bars move down by as much.
    pub compass_h: f32,
    /// Where the minimap was drawn last, frame and lines under it, GUI pixels.
    pub mini_rect: Option<[f32; 4]>,
}

/// Part `src` of the minimap's image (`w` x `h` texels) drawn into `dst`, cut to what the image
/// holds: past it the slot shows somebody else's picture (only for the frame or two a larger
/// image takes to come).
fn sprite_within(draw: &mut dyn DrawContext, w: usize, h: usize, src: [f32; 4], dst: [f32; 4]) {
    let (w, h) = (w as f32, h as f32);
    if src[0] >= 0.0 && src[1] >= 0.0 && src[0] + src[2] <= w && src[1] + src[3] <= h {
        draw.sprite(SPRITE, src, dst, None);
        return;
    }
    let (kx, ky) = (dst[2] / src[2], dst[3] / src[3]);
    let (x0, y0) = (src[0].max(0.0), src[1].max(0.0));
    let (x1, y1) = ((src[0] + src[2]).min(w), (src[1] + src[3]).min(h));
    if x1 <= x0 || y1 <= y0 {
        return;
    }
    let to = [dst[0] + (x0 - src[0]) * kx, dst[1] + (y0 - src[1]) * ky, (x1 - x0) * kx, (y1 - y0) * ky];
    draw.sprite(SPRITE, [x0, y0, x1 - x0, y1 - y0], to, None);
}

/// The north-up picture a round or turning minimap is cut from.
#[derive(Default)]
pub struct Soft {
    src: Vec<u8>,
    /// The picture's frame, what it was painted from and when.
    painted: Option<(Frame, u64, Look, u64)>,
    out: Vec<u8>,
    /// What the slot shows: the player's place and turn, the picture's time, the side.
    shown: Option<(f64, f64, f32, u64, usize)>,
    pub uploads: u64,
}

impl State {
    /// How the minimap is turned: the angle, clockwise from up, that north-up map directions
    /// are turned back by. 0 when north is up.
    fn turn(&self, player: Player) -> f32 {
        if self.config.rotate { (player.yaw + 180.0).to_radians() } else { 0.0 }
    }

    /// GUI pixels of map the image must hold: the side, or its diagonal when the map turns.
    /// While the minimap is enlarged, or on its way there or back, the enlarged side: one
    /// image for the whole way, painted once.
    pub(crate) fn minimap_span(&self) -> f32 {
        let side = if self.hud.held || self.hud.grow > 0.0 { self.big_side() } else { self.config.size };
        if self.config.rotate { side * 1.42 } else { side }
    }

    pub(crate) fn minimap_corner(&self) -> Corner {
        match self.config.corner {
            1 => Corner::TopLeft,
            2 => Corner::BottomRight,
            3 => Corner::BottomLeft,
            _ => Corner::TopRight,
        }
    }

    /// The lines under the minimap, in GUI pixels.
    fn minimap_text_h(&self) -> f32 {
        let c = &self.config;
        let n = [c.coords, c.other_coords, c.biome, c.light, c.clock].into_iter().filter(|on| *on).count();
        if n == 0 { 0.0 } else { n as f32 * 10.0 + 1.0 }
    }

    /// The lines under the minimap: coordinates, the other dimension's, biome, light, time.
    fn info_lines(&self, host: &dyn ClientHost, player: Player) -> Vec<(String, u32)> {
        let cfg = &self.config;
        let mut lines = Vec::new();
        if cfg.coords {
            let p = player.pos;
            lines.push((format!("{} {} {}", p[0].floor(), p[1].floor(), p[2].floor()), 0xFFFF_FFFF));
        }
        if cfg.other_coords && let Some((name, ox, oz)) = self.other_side(player.pos[0], player.pos[2]) {
            lines.push((format!("{name} {ox} {oz}"), 0xFFB0_B0C8));
        }
        let level = host.level(&self.dim).filter(|_| cfg.biome || cfg.light || cfg.clock);
        let Some(level) = level else { return lines };
        let b = crate::block_of(player.pos);
        if cfg.biome && let Some(id) = level.client_biome(host, b) {
            lines.push((crate::radar::pretty(&id), 0xFFB0_E0B0));
        }
        if cfg.light {
            let block = level.brightness(host, mod_api::LightLayer::Block, b);
            let sky = level.brightness(host, mod_api::LightLayer::Sky, b);
            if let (Some(block), Some(sky)) = (block, sky) {
                lines.push((format!("Light {block}  Sky {sky}"), 0xFFF0_E0A0));
            }
        }
        if cfg.clock && let Some(t) = level.clock_time(host) {
            // Tick 0 is 6:00; a day is 24000 ticks.
            let t = t.rem_euclid(24_000);
            lines.push((format!("{:02}:{:02}", (t / 1000 + 6) % 24, t % 1000 * 60 / 1000), 0xFFC0_D0FF));
        }
        lines
    }

    /// The minimap's side fully enlarged: `GROW` times the setting, as far as the screen has
    /// room (its height less the lines under it, and under half its width), even.
    fn big_side(&self) -> f32 {
        let base = self.config.size;
        let [ox, oy] = self.config.offset;
        let room = (self.gui[1] - oy - self.minimap_text_h() - 16.0).min((self.gui[0] - ox) * 0.45);
        (((base * GROW).min(room) / 2.0).floor() * 2.0).max(base)
    }

    /// The minimap's side this frame, in whole GUI pixels (an even number, so its centre is
    /// on a pixel's corner as at its own size).
    pub(crate) fn mini_side(&self) -> f32 {
        let base = self.config.size;
        let g = self.hud.grow.clamp(0.0, 1.0);
        if g <= 0.0 {
            return base;
        }
        let eased = g * g * (3.0 - 2.0 * g);
        let side = base + (self.big_side() - base) * eased;
        ((side / 2.0).round() * 2.0).max(base)
    }

    /// How much the enlarged minimap covers `area` (GUI pixels): 0 when it doesn't reach it,
    /// up to 1 as the minimap grows over it. Worked out from where it was drawn last and the
    /// side it grows to, its corner staying where it is.
    pub(crate) fn enlarged_over(&self, area: [f32; 4]) -> f32 {
        let Some(r) = self.hud.mini_rect.filter(|_| self.hud.grow > 0.0 && !self.hud.hidden) else { return 0.0 };
        let big = self.big_side() + 2.0;
        let text_h = r[3] - (r[2]);
        let corner = self.minimap_corner();
        let right = matches!(corner, Corner::TopRight | Corner::BottomRight);
        let top = matches!(corner, Corner::TopRight | Corner::TopLeft);
        let x = if right { r[0] + r[2] - big } else { r[0] };
        let y = if top { r[1] } else { r[1] + r[3] - big - text_h };
        let meets = x < area[0] + area[2] && area[0] < x + big && y < area[1] + area[3] && area[1] < y + big + text_h;
        if !meets {
            return 0.0;
        }
        let g = self.hud.grow.clamp(0.0, 1.0);
        g * g * (3.0 - 2.0 * g)
    }

    /// Grows the minimap while the enlarge key is held and shrinks it back after.
    pub(crate) fn animate_minimap(&mut self, held: bool, dt: f32) {
        self.hud.held = held && self.config.minimap;
        let step = dt / GROW_TIME;
        self.hud.grow =
            if self.hud.held { (self.hud.grow + step).min(1.0) } else { (self.hud.grow - step).max(0.0) };
    }

    /// Moves the minimap's zoom a step (-1 closer, 1 wider) and keeps it as the setting, so
    /// Options shows it and it stays.
    pub(crate) fn step_zoom(&mut self, host: &dyn ClientHost, by: i32) {
        let now = MINIMAP_ZOOMS.iter().position(|z| *z == self.config.scale).unwrap_or(2) as i32;
        let next = (now + by).clamp(0, MINIMAP_ZOOMS.len() as i32 - 1) as usize;
        self.hud.zoomed = Some((next, self.clock));
        if next as i32 == now {
            return;
        }
        host.set_setting(ID, "minimap_zoom", &next.to_string());
        self.read_config(host);
        self.config.scale = MINIMAP_ZOOMS[next];
    }

    /// "2x" on the minimap's lower edge for a moment after the zoom keys.
    fn draw_zoom_hint(&self, draw: &mut dyn DrawContext, cx: f32, bottom: f32) {
        let Some((step, at)) = self.hud.zoomed else { return };
        let age = (self.clock - at) as f32;
        let a = if age < 0.9 { 1.0 } else { 1.0 - (age - 0.9) / 0.35 };
        if !(0.03..=1.0).contains(&a) {
            return;
        }
        let text = ZOOM_LABELS[step];
        let tw = draw.text_width(text).ceil();
        let x = (cx - tw / 2.0).round();
        let alpha = |most: f32| ((most * a).round() as u32) << 24;
        draw.fill([x - 3.0, bottom - 13.0, tw + 6.0, 11.0], alpha(160.0));
        if a * 255.0 >= 8.0 {
            draw.text(text, [x, bottom - 11.0], alpha(255.0) | 0xFF_FFFF, false);
        }
    }

    /// Whether the picture has to be made here: only a turning map does. A round one that
    /// keeps north up is the sprite drawn in rows, each as wide as the circle is there.
    fn soft(&self) -> bool {
        self.config.rotate
    }

    /// Keeps the minimap's image current.
    pub(crate) fn refresh_minimap(&mut self, host: &mut dyn ClientHost, player: Player, look: Look) {
        let frame = self.minimap_frame(player, self.minimap_span());
        let revision = self.painted();
        let soft = self.soft();
        // The old picture still covers the map while the player is within the margin.
        let (old, old_revision, old_at) = if soft {
            match self.soft.painted {
                Some((f, r, l, at)) if l == look => (Some(f), r, at),
                _ => (None, 0, 0),
            }
        } else {
            match self.image {
                Some(i) if !i.full && !i.soft && i.look == look => (Some(i.frame), i.revision, i.at),
                _ => (None, 0, 0),
            }
        };
        let urgent = old.is_none_or(|f| {
            f.step != frame.step
                || f.w != frame.w
                || (f.x0 - frame.x0).abs() > (MARGIN - 2) * frame.step
                || (f.z0 - frame.z0).abs() > (MARGIN - 2) * frame.step
        });
        let since = if old.is_some() { self.now.saturating_sub(old_at) } else { u64::MAX };
        // New terrain a few times a second. And now and then regardless: the slot is shared,
        // so another mod's picture, or a resource reload, may have replaced this one.
        let paint = urgent || (old_revision != revision && since >= 250) || (!soft && since >= REPAINT);
        if paint {
            // What is stored for the frame is read first, so a layer switched to comes in whole;
            // the picture of the one left stays up for the frame or two that may take.
            let start = host.millis();
            let (min, max) = frame.blocks();
            let store = if look.cave.is_some() { &mut self.cave } else { &mut self.surface };
            if !store.load_area(min, max, || host.millis().saturating_sub(start) < 30) {
                if soft {
                    self.refresh_soft(host, player, look);
                }
                return;
            }
            let mut pixels = std::mem::take(if soft { &mut self.soft.src } else { &mut self.pixels });
            let tints = std::mem::take(&mut self.tints);
            render::compose(store, &self.palette, if look.tints { &tints } else { &[] }, frame, &mut pixels);
            self.tints = tints;
            if soft {
                self.soft.src = pixels;
                self.soft.painted = Some((frame, revision, look, self.now));
            } else {
                self.pixels = pixels;
                if host.mod_image(frame.w as u32, frame.h as u32, &self.pixels) {
                    self.image = Some(Image { full: false, soft: false, frame, revision, look, at: self.now });
                    self.stats.uploads += 1;
                }
            }
        }
        if soft {
            self.refresh_soft(host, player, look);
        }
    }

    /// Writes this frame's view of the north-up picture into the slot, when it moved.
    fn refresh_soft(&mut self, host: &mut dyn ClientHost, player: Player, look: Look) {
        let Some((frame, revision, _, painted_at)) = self.soft.painted else { return };
        let side = self.mini_side();
        // Three texels to two GUI pixels: an upload costs by the texel, and this one is made
        // every frame the view turns.
        let n = ((side * 1.5) as usize).clamp(32, 384);
        let angle = self.turn(player);
        let key = (player.pos[0], player.pos[2], angle, painted_at, n);
        let in_slot = self.image.is_some_and(|i| i.soft && self.now.saturating_sub(i.at) < REPAINT);
        if in_slot && self.soft.shown == Some(key) {
            return;
        }
        let round = self.config.round;
        let out = &mut self.soft.out;
        out.clear();
        out.resize(n * n * 4, 0);
        let src = &self.soft.src;
        let (sin, cos) = angle.sin_cos();
        // Source texels per output texel, and the player in source texels.
        let k = side / n as f32 * self.config.scale / frame.step as f32;
        let cx = (player.pos[0] - f64::from(frame.x0)) as f32 / frame.step as f32;
        let cz = (player.pos[2] - f64::from(frame.z0)) as f32 / frame.step as f32;
        let half = n as f32 / 2.0;
        let unseen = [(UNSEEN_COLOR >> 16) as u8, (UNSEEN_COLOR >> 8) as u8, UNSEEN_COLOR as u8, 255];
        let rim = [(FRAME_COLOR >> 16) as u8, (FRAME_COLOR >> 8) as u8, FRAME_COLOR as u8, 255];
        for j in 0..n {
            let v = j as f32 + 0.5 - half;
            // Along a row the source point moves by a fixed step.
            let (mut sx, mut sz) = (cx + ((0.5 - half) * cos - v * sin) * k, cz + ((0.5 - half) * sin + v * cos) * k);
            let (dx, dz) = (cos * k, sin * k);
            let row = &mut out[j * n * 4..(j + 1) * n * 4];
            for (i, pixel) in row.as_chunks_mut::<4>().0.iter_mut().enumerate() {
                let (px, pz) = (sx, sz);
                sx += dx;
                sz += dz;
                if round {
                    let u = i as f32 + 0.5 - half;
                    let edge = half - (u * u + v * v).sqrt();
                    if edge <= 0.0 {
                        continue;
                    }
                    if edge < 2.5 {
                        // The rim, its outer texel fading out.
                        pixel.copy_from_slice(&rim);
                        pixel[3] = (edge.min(1.0) * 255.0) as u8;
                        continue;
                    }
                }
                let (tx, tz) = (px.floor() as i32, pz.floor() as i32);
                let texel = if tx >= 0 && tz >= 0 && (tx as usize) < frame.w && (tz as usize) < frame.h {
                    let at = (tz as usize * frame.w + tx as usize) * 4;
                    &src[at..at + 4]
                } else {
                    &[0u8; 4][..]
                };
                pixel.copy_from_slice(if texel[3] == 0 { &unseen } else { texel });
            }
        }
        if host.mod_image(n as u32, n as u32, &self.soft.out) {
            let frame = Frame { x0: 0, z0: 0, step: 1, w: n, h: n };
            self.image = Some(Image { full: false, soft: true, frame, revision, look, at: self.now });
            self.soft.shown = Some(key);
            self.soft.uploads += 1;
        }
    }

    pub(crate) fn draw_minimap(&mut self, host: &dyn ClientHost, event: &mut RenderGuiPostEvent<'_>) {
        let Some(player) = self.player else { return };
        let cfg = self.config;
        let side = self.mini_side();
        let lines = self.info_lines(host, player);
        let text_h = if lines.is_empty() { 0.0 } else { lines.len() as f32 * 10.0 + 1.0 };
        let (w, h) = (side + 2.0, side + 2.0 + text_h);
        let (bx, by) = event.corners.place(self.minimap_corner(), w, h);
        // Moved in from its corner by the offset, but never off the screen.
        let (right, bottom) = (matches!(cfg.corner, 0 | 2), matches!(cfg.corner, 2 | 3));
        let bx = (bx + if right { -cfg.offset[0] } else { cfg.offset[0] }).clamp(0.0, (self.gui[0] - w).max(0.0));
        let by = (by + if bottom { -cfg.offset[1] } else { cfg.offset[1] }).clamp(0.0, (self.gui[1] - h).max(0.0));
        self.hud.mini_rect = Some([bx, by, side + 2.0, side + 2.0 + text_h]);
        let draw = &mut *event.draw;
        let (x, y) = (bx + 1.0, by + 1.0);
        let center = [x + side / 2.0, y + side / 2.0];
        let soft = self.soft();
        if !cfg.round {
            draw.fill([bx, by, side + 2.0, side + 2.0], FRAME_COLOR);
            draw.fill([x, y, side, side], UNSEEN_COLOR);
        }
        match self.image.filter(|i| !i.full && i.soft == soft) {
            Some(image) if soft => {
                let n = image.frame.w as f32;
                draw.sprite(SPRITE, [0.0, 0.0, n, n], [x, y, side, side], None);
            }
            Some(image) => {
                let f = image.frame;
                let step = f.step as f32;
                // The map's top-left corner in the image's texels.
                let across = side * cfg.scale / step;
                let sx = ((player.pos[0] - f64::from(f.x0)) as f32) / step - across / 2.0;
                let sz = ((player.pos[2] - f64::from(f.z0)) as f32) / step - across / 2.0;
                if cfg.round {
                    // A row a GUI pixel high at a time, as wide as the circle is there, on a
                    // rim one pixel wider.
                    let (r, per) = (side / 2.0, across / side);
                    for row in 0..side as i32 {
                        let v = row as f32 + 0.5 - r;
                        let half = (r * r - v * v).max(0.0).sqrt().round();
                        if half < 1.0 {
                            continue;
                        }
                        let (left, top) = (center[0] - half, y + row as f32);
                        draw.fill([left - 1.0, top, half * 2.0 + 2.0, 1.0], FRAME_COLOR);
                        draw.fill([left, top, half * 2.0, 1.0], UNSEEN_COLOR);
                        let src = [sx + (r - half) * per, sz + row as f32 * per, half * 2.0 * per, per];
                        sprite_within(draw, f.w, f.h, src, [left, top, half * 2.0, 1.0]);
                    }
                    draw.fill([center[0] - 4.0, y - 1.0, 8.0, 1.0], FRAME_COLOR);
                    draw.fill([center[0] - 4.0, y + side, 8.0, 1.0], FRAME_COLOR);
                } else {
                    sprite_within(draw, f.w, f.h, [sx, sz, across, across], [x, y, side, side]);
                }
            }
            None => {}
        }
        // Where a block is on the minimap, from its centre, in GUI pixels.
        let (sin, cos) = self.turn(player).sin_cos();
        let offset = move |bx: f64, bz: f64| {
            let dx = ((bx - player.pos[0]) as f32) / cfg.scale;
            let dz = ((bz - player.pos[2]) as f32) / cfg.scale;
            [dx * cos + dz * sin, -dx * sin + dz * cos]
        };
        let reach = side / 2.0 - 2.0;
        let within = move |o: [f32; 2]| {
            if cfg.round { o[0] * o[0] + o[1] * o[1] <= reach * reach } else { o[0].abs().max(o[1].abs()) <= reach }
        };
        let place = move |bx: f64, bz: f64| {
            let o = offset(bx, bz);
            within(o).then_some([center[0] + o[0], center[1] + o[1]])
        };

        // Overlays, then the entities, then the waypoints, then the player.
        let span = f64::from(self.minimap_span() * cfg.scale / 2.0);
        let query = OverlayQuery {
            dimension: &self.dim,
            bounds: [player.pos[0] - span, player.pos[2] - span, player.pos[0] + span, player.pos[2] + span],
            tick: self.scan_surface.tick,
        };
        let mut marks: Vec<OverlayMarker> = Vec::new();
        for slot in self.overlays.iter_mut().filter(|s| s.on) {
            slot.overlay.markers(&query, &mut marks);
        }
        // Nothing of a marker reaches past the map's own edge.
        let clipped = !cfg.round && draw.push_clip([x, y, side, side]).is_ok();
        for mark in marks.iter().filter(|m| m.area.is_none()) {
            if let Some(at) = place(mark.at[0], mark.at[1]) {
                marker(draw, at, mark.rgb, 1.5);
            }
        }
        let around = [player.pos[0] - span, player.pos[2] - span, player.pos[0] + span, player.pos[2] + span];
        self.draw_explore_mini(draw, around, place);
        self.draw_blips(draw, f64::from(self.partial), None, 1.0, place);
        // Waypoints: on the map, or on its edge towards them, wholly inside it (an item or a
        // face is wider than a diamond).
        // The tracked one and the nearest, as many as fit a frame: each is several draw calls.
        let until = host.millis() + crate::DRAW_WORK_MS;
        for (n, (_, w)) in self.here_nearest([player.pos[0], player.pos[2]]).into_iter().enumerate() {
            if n >= crate::MINIMAP_WAYPOINTS || (n % 16 == 15 && host.millis() >= until) {
                break;
            }
            let o = offset(f64::from(w.pos[0]) + 0.5, f64::from(w.pos[2]) + 0.5);
            let far = if cfg.round { (o[0] * o[0] + o[1] * o[1]).sqrt() } else { o[0].abs().max(o[1].abs()) };
            let edge = (side / 2.0 - 1.0 - crate::icons::reach(w, 1.5)).min(reach);
            let k = (edge / far.max(0.001)).min(1.0);
            gfx::waypoint_icon(draw, w, [center[0] + o[0] * k, center[1] + o[1] * k], if k < 1.0 { 1.5 } else { 2.0 }, false);
        }
        if clipped {
            let _ = draw.pop_clip();
        }
        let facing = if cfg.rotate { 0.0 } else { player.yaw + 180.0 };
        gfx::player_marker(draw, center, facing, 1.0);
        // North, on the rim.
        let north = [center[0] - sin * (side / 2.0 - 5.0), center[1] - cos * (side / 2.0 - 5.0)];
        draw.text("N", [north[0] - 2.5, north[1] - 4.0], 0xFFFF_FFFF, true);
        self.draw_zoom_hint(draw, center[0], y + side);
        let mut line_y = y + side + 3.0;
        for (text, color) in &lines {
            centered(draw, text, center[0], line_y, *color);
            line_y += 10.0;
        }
    }

    /// Where world point `at` is on this frame's screen, in GUI pixels: through the camera's own
    /// matrices, the ones the world is drawn with, view bobbing and all. (The game's `project`
    /// leaves the bobbing out, so what it places drifts against the world while walking.)
    pub(crate) fn on_screen(&self, host: &dyn ClientHost, at: [f64; 3]) -> Option<([f32; 2], f64)> {
        self.on_screen_by(&host.camera()?, at)
    }

    /// [`on_screen`](Self::on_screen) through a camera read once for many points.
    pub(crate) fn on_screen_by(&self, cam: &mod_api::Camera, at: [f64; 3]) -> Option<([f32; 2], f64)> {
        let rel = [0, 1, 2].map(|i| (at[i] - cam.position[i]) as f32);
        // Column-major, the point taken from the camera first.
        let apply = |m: &[f32; 16], v: [f32; 4]| -> [f32; 4] {
            std::array::from_fn(|r| (0..4).map(|c| m[c * 4 + r] * v[c]).sum())
        };
        let clip = apply(&cam.projection, apply(&cam.view, [rel[0], rel[1], rel[2], 1.0]));
        if clip[3] <= 1e-4 {
            return None;
        }
        let (nx, ny) = (clip[0] / clip[3], clip[1] / clip[3]);
        if !(nx.abs() <= 1.0 && ny.abs() <= 1.0) {
            return None;
        }
        let far = rel.iter().map(|v| f64::from(*v).powi(2)).sum::<f64>().sqrt();
        Some(([(nx + 1.0) / 2.0 * self.gui[0], (1.0 - ny) / 2.0 * self.gui[1]], far))
    }

    /// Name and distance over each waypoint, where it is on the screen: the game's own font at
    /// a fixed small size (one or two screen pixels a font pixel, never the full GUI scale), on
    /// a faint plate as a name tag has, the waypoint's icon left of the name. Each label is
    /// placed from this frame's camera alone:
    /// nothing is carried over from the last frame and labels don't push each other about.
    pub(crate) fn draw_labels(&self, host: &dyn ClientHost, draw: &mut dyn DrawContext) {
        use mod_api::client::hud_layout::PoseOp;
        use mod_api::{GuiDraw, Text};
        let [w, _] = draw.size();
        let scale = host.gui_scale().unwrap_or(2).max(1) as f32;
        let font = crate::label_font(self.gui[1], scale);
        let k = font / scale;
        let mut list = Vec::new();
        let mut later = Vec::new();
        let Some(cam) = host.camera() else { return };
        // The tracked one and the nearest first; past a screenful of labels the rest would only
        // pile up, and each one costs the frame time.
        let from = [cam.position[0], cam.position[2]];
        let until = host.millis() + crate::DRAW_WORK_MS;
        for (n, (_, wp)) in self.here_nearest(from).into_iter().enumerate() {
            if later.len() >= crate::LABELS_MAX || (n % 16 == 15 && host.millis() >= until) {
                break;
            }
            let at = [f64::from(wp.pos[0]) + 0.5, f64::from(wp.pos[1]) + 1.6, f64::from(wp.pos[2]) + 0.5];
            let Some(([sx, sy], far)) = self.on_screen_by(&cam, at) else { continue };
            if self.config.hover_names && (sx - w / 2.0).abs() > AIM {
                continue;
            }
            let text = format!("{} {}", wp.name, distance_text(far));
            let tw = draw.text_width(&text).ceil() as i32;
            // On whole screen pixels, so the glyphs stay sharp; a screen pixel is the finest
            // step there is.
            let snap = |v: f32| (v * scale).round() / scale;
            let origin = [snap(sx), snap(sy - 13.0 * k)];
            list.push(GuiDraw::Push(vec![PoseOp::Translate(origin[0], origin[1]), PoseOp::Scale(k, k)]));
            // The waypoint's icon, then a gap, then the name: "◆ Base 125m", centred.
            let icon = crate::icons::LabelIcon::of(wp, font);
            let width = icon.width + 2 + tw;
            let left = -(width / 2);
            list.push(GuiDraw::Fill { x0: left - 2, y0: -2, x1: left + width + 2, y1: 9, color: 0x5000_0000 });
            icon.paint(&mut list, left, 3, wp.tracked);
            list.push(GuiDraw::Text {
                text: Text::literal(text),
                x: left + icon.width + 2,
                y: 0,
                color: 0xFFFF_FFFF,
                shadow: false,
            });
            list.push(GuiDraw::Pop);
            later.push((icon, origin, left, wp.tracked));
        }
        if !list.is_empty() {
            let _ = draw.gui_paint(&list);
        }
        for (icon, origin, left, tracked) in &later {
            icon.draw_after(draw, *origin, k, *left, 3, *tracked);
        }
    }

    /// The direction arrow: at the top of the screen, pointing to the tracked waypoint as the
    /// view is turned, with its name and distance.
    pub(crate) fn draw_direction(&self, draw: &mut dyn DrawContext) {
        let Some(player) = self.player else { return };
        let Some(wp) = self.waypoints.iter().find(|w| w.tracked && w.dim == self.dim) else { return };
        let (dx, dz) = (f64::from(wp.pos[0]) + 0.5 - player.pos[0], f64::from(wp.pos[2]) + 0.5 - player.pos[2]);
        let dist = (dx * dx + dz * dz).sqrt();
        // As on a map turned with the view: straight ahead is up.
        let (sin, cos) = (player.yaw + 180.0).to_radians().sin_cos();
        let (u, v) = (dx as f32 * cos + dz as f32 * sin, -(dx as f32) * sin + dz as f32 * cos);
        let angle = u.atan2(-v).to_degrees();
        let [w, _] = draw.size();
        let text = format!("{} {}", wp.name, distance_text(dist));
        let tw = draw.text_width(&text);
        let cx = (w / 2.0).round();
        draw.fill([cx - tw / 2.0 - 4.0, 4.0, tw + 8.0, 31.0], 0x6000_0000);
        if dist >= 2.0 {
            gfx::arrow(draw, [cx, 14.0], angle, 7.0, argb(wp.color));
        } else {
            gfx::waypoint_icon(draw, wp, [cx, 14.0], 3.0, false);
        }
        centered(draw, &text, cx, 24.0, 0xFFFF_FFFF);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_turning_map_needs_its_diagonal() {
        let mut st = State::default();
        assert_eq!(st.minimap_span(), 96.0);
        st.config.rotate = true;
        assert!(st.minimap_span() >= 96.0 * std::f32::consts::SQRT_2);
        // Facing south (yaw 0) the map is turned half round; facing north not at all.
        let turn = |yaw: f32| st.turn(Player { pos: [0.0; 3], yaw }).to_degrees().rem_euclid(360.0);
        assert!((turn(0.0) - 180.0).abs() < 1e-3);
        assert!(turn(180.0).abs() < 1e-3 || (turn(180.0) - 360.0).abs() < 1e-3);
    }
}
