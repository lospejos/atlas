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
use crate::{FRAME_COLOR, Player, SPRITE, State, UNSEEN_COLOR};

/// With "names only on hover", a waypoint's label shows within this many GUI pixels of the
/// crosshair, sideways.
const AIM: f32 = 18.0;
/// The image is painted again after this many milliseconds even when nothing changed.
const REPAINT: u64 = 3000;

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
    pub(crate) fn minimap_span(&self) -> f32 {
        if self.config.rotate { self.config.size * 1.42 } else { self.config.size }
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
        let side = self.config.size;
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
            for (i, pixel) in row.chunks_exact_mut(4).enumerate() {
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
        let side = cfg.size;
        let other = if cfg.other_coords { self.other_side(player.pos[0], player.pos[2]) } else { None };
        let text_h = if cfg.coords { 11.0 } else { 0.0 } + if other.is_some() { 10.0 } else { 0.0 };
        let corner = match cfg.corner {
            1 => Corner::TopLeft,
            2 => Corner::BottomRight,
            3 => Corner::BottomLeft,
            _ => Corner::TopRight,
        };
        let (bx, by) = event.corners.place(corner, side + 2.0, side + 2.0 + text_h);
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
                        draw.sprite(SPRITE, src, [left, top, half * 2.0, 1.0], None);
                    }
                    draw.fill([center[0] - 4.0, y - 1.0, 8.0, 1.0], FRAME_COLOR);
                    draw.fill([center[0] - 4.0, y + side, 8.0, 1.0], FRAME_COLOR);
                } else {
                    draw.sprite(SPRITE, [sx, sz, across, across], [x, y, side, side], None);
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
        for mark in marks.iter().filter(|m| m.area.is_none()) {
            if let Some(at) = place(mark.at[0], mark.at[1]) {
                marker(draw, at, mark.rgb, 1.5);
            }
        }
        let around = [player.pos[0] - span, player.pos[2] - span, player.pos[0] + span, player.pos[2] + span];
        self.draw_explore_mini(draw, around, place);
        self.draw_blips(draw, f64::from(self.partial), None, 1.0, place);
        // Waypoints: on the map, or on its edge towards them.
        for (_, w) in self.here() {
            let o = offset(f64::from(w.pos[0]) + 0.5, f64::from(w.pos[2]) + 0.5);
            let k = if cfg.round {
                (reach / (o[0] * o[0] + o[1] * o[1]).sqrt().max(0.001)).min(1.0)
            } else {
                (reach / o[0].abs().max(o[1].abs()).max(0.001)).min(1.0)
            };
            marker(draw, [center[0] + o[0] * k, center[1] + o[1] * k], w.color, if k < 1.0 { 1.5 } else { 2.0 });
        }
        let facing = if cfg.rotate { 0.0 } else { player.yaw + 180.0 };
        gfx::player_marker(draw, center, facing, 1.0);
        // North, on the rim.
        let north = [center[0] - sin * (side / 2.0 - 5.0), center[1] - cos * (side / 2.0 - 5.0)];
        draw.text("N", [north[0] - 2.5, north[1] - 4.0], 0xFFFF_FFFF, true);
        let mut line_y = y + side + 3.0;
        if cfg.coords {
            let p = player.pos;
            let text = format!("{} {} {}", p[0].floor(), p[1].floor(), p[2].floor());
            centered(draw, &text, center[0], line_y, 0xFFFF_FFFF);
            line_y += 10.0;
        }
        if let Some((name, ox, oz)) = other {
            centered(draw, &format!("{name} {ox} {oz}"), center[0], line_y, 0xFFB0_B0C8);
        }
        let _ = host;
    }

    /// Name and distance over each waypoint, where it is on the screen.
    pub(crate) fn draw_labels(&self, host: &dyn ClientHost, draw: &mut dyn DrawContext) {
        let Some(player) = self.player else { return };
        let [w, h] = draw.size();
        // Nearest first, so a nearer label keeps its place and a farther one moves under it.
        let mut shown = Vec::new();
        for (_, wp) in self.here() {
            let at = [f64::from(wp.pos[0]) + 0.5, f64::from(wp.pos[1]) + 1.6, f64::from(wp.pos[2]) + 0.5];
            let Some([nx, ny, _]) = host.project(at) else { continue };
            if nx.abs() > 1.0 || ny.abs() > 1.0 {
                continue;
            }
            let d = (0..3).map(|i| (at[i] - player.pos[i]).powi(2)).sum::<f64>().sqrt();
            let (sx, sy) = ((nx + 1.0) / 2.0 * w, (1.0 - ny) / 2.0 * h);
            if self.config.hover_names && (sx - w / 2.0).abs() > AIM {
                continue;
            }
            shown.push((d, wp, sx, sy));
        }
        shown.sort_by(|a, b| a.0.total_cmp(&b.0));
        // Small and the same size on the screen at any distance and any GUI scale: a font pixel
        // is two screen pixels, so a label is ten screen pixels high.
        let px = 2.0 / host.gui_scale().unwrap_or(2).max(1) as f32;
        let mut placed: Vec<[f32; 4]> = Vec::new();
        for (d, wp, sx, sy) in shown {
            let text = format!("{} {}", wp.name, distance_text(d));
            let tw = gfx::tiny_width(&text, px);
            let high = 9.0 * px;
            let mut rect = [(sx - tw / 2.0 - 2.0 * px).round(), (sy - high - 2.0).round(), tw + 4.0 * px, high];
            while placed.iter().any(|p| {
                rect[0] < p[0] + p[2] && p[0] < rect[0] + rect[2] && rect[1] < p[1] + p[3] && p[1] < rect[1] + rect[3]
            }) {
                rect[1] += high + px;
            }
            placed.push(rect);
            draw.fill([rect[0], rect[1], rect[2], high - px], 0x9000_0000);
            draw.fill([rect[0], rect[1] + high - px, rect[2], px], argb(wp.color));
            gfx::tiny_text(draw, &text, [rect[0] + 2.0 * px, rect[1] + 1.5 * px], px, 0xFFFF_FFFF);
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
            marker(draw, [cx, 14.0], wp.color, 3.0);
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
