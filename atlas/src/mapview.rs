//! The world map's view and the one image both maps are drawn from: where the view is each
//! frame (dragging, the glide after a drag, the slide back to the player, the wheel's zoom),
//! and when and how the image is painted again.
//!
//! Uploading the image is what costs: about 18 nanoseconds a texel on the game's side, so a
//! screen-sized picture takes 5 to 19 ms, more than a frame. So the image is never painted
//! while the view is being panned (a drag in motion, its glide, a slide): every such frame
//! draws the image it has at the view's exact place, and what the image doesn't hold yet is
//! filled with a coarse mosaic straight from the store. The moment the view rests, the image
//! is painted for where it is, with a wide margin; the hitch falls where nothing moves.
use std::collections::VecDeque;

use mod_api::ClientHost;

use crate::render::{self, Frame};
use crate::{IMAGE_MAX, Player, State, Ui};

/// Texels the minimap's image reaches past the map's edge, so walking scrolls the sprite
/// instead of repainting the image.
pub const MARGIN: i32 = 16;
/// The full map's zoom range, GUI pixels per block.
pub const ZOOM_MAX: f32 = 8.0;
pub const ZOOM_MIN: f32 = 0.125;
/// One notch of the wheel multiplies the zoom by this.
pub const ZOOM_STEP: f32 = 1.15;
/// Seconds a wheel notch takes to arrive.
const ZOOM_TIME: f64 = 0.11;
/// Texels the full map's image reaches past the screen where the slot has room, so dragging
/// and zooming move the sprite instead of repainting the image.
const MAP_MARGIN: i32 = 160;
/// The margin of an image painted in the middle of a zoom: small, so the upload is quick.
const ZOOM_MARGIN: i32 = 48;
/// The view counts as panning until it has been still this long, in seconds.
const SETTLE: f64 = 0.07;
/// The widest the full map may reach, in blocks, so the regions it needs fit in memory.
const SPAN_MAX: f32 = 4096.0;
/// The image is painted again after this many milliseconds even when nothing changed.
const REPAINT: u64 = 3000;
/// Seconds Space takes to bring the view back to the player.
const SLIDE_TIME: f64 = 0.25;
/// Seconds the glide after a drag lasts: its speed falls off as the square of the time left,
/// so it covers a third of what the drag's speed would in that time and stops without a jolt.
const GLIDE_TIME: f64 = 0.28;
/// A drag let go slower than this, in GUI pixels a second, doesn't glide.
const GLIDE_MIN: f64 = 70.0;
const GLIDE_MAX: f64 = 5000.0;
/// The drag's speed is taken over its last moments, this many seconds.
const TRAIL: f64 = 0.08;

/// The full map's view: the block at the screen's centre and GUI pixels per block.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct View {
    pub cx: f64,
    pub cz: f64,
    pub zoom: f32,
}

/// A wheel zoom on its way: the zoom eases from `from` to `to` while the block that was under
/// the cursor stays under it.
#[derive(Clone, Copy)]
pub struct ZoomAnim {
    pub from: f32,
    pub to: f32,
    pub start: f64,
    pub block: [f64; 2],
    pub mouse: [f32; 2],
}

/// A drag: the block that was under the cursor when the button went down stays under it.
#[derive(Clone, Copy)]
pub struct Drag {
    pub block: [f64; 2],
}

/// The glide after a drag: the speed it began with, in blocks a second, and when.
#[derive(Clone, Copy)]
pub struct Glide {
    pub v: [f64; 2],
    pub start: f64,
}

/// The slide back to the player: where the view was, from the player, when it began.
#[derive(Clone, Copy)]
pub struct Slide {
    pub off: [f64; 2],
    pub start: f64,
}

/// What the image slot holds now.
#[derive(Clone, Copy, PartialEq)]
pub struct Image {
    pub full: bool,
    /// The minimap's turned or round picture: the slot holds the map as the screen shows it.
    pub soft: bool,
    pub frame: Frame,
    /// What it was painted from: the store's and the legend's revisions, and which layer.
    pub revision: u64,
    pub look: Look,
    pub at: u64,
}

/// Which map an image shows, beyond its frame.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Look {
    /// The cave band, none for the surface.
    pub cave: Option<i32>,
    pub tints: bool,
    pub tint_revision: u64,
}

impl State {
    // ---- frames ----

    /// The minimap's image frame around the player: the map's `span` GUI pixels and a margin.
    pub(crate) fn minimap_frame(&self, player: Player, span: f32) -> Frame {
        let step = self.config.scale.max(1.0) as i32;
        let texels = (span * self.config.scale / step as f32).ceil() as i32 + 2 * MARGIN;
        let texels = texels.min(IMAGE_MAX as i32);
        let half = texels * step / 2;
        let snap = |v: f64| ((v.floor() as i32 - half).div_euclid(step)) * step;
        Frame { x0: snap(player.pos[0]), z0: snap(player.pos[2]), step, w: texels as usize, h: texels as usize }
    }

    /// Blocks per texel for the full map at `zoom`: the smallest power of two that keeps a
    /// texel at least a GUI pixel, and the image inside the slot.
    fn map_step(&self, zoom: f32) -> i32 {
        let [w, h] = self.gui;
        let mut step = 1;
        while (step as f32) * zoom < 1.0 {
            step *= 2;
        }
        while (w.max(h) / zoom / step as f32).ceil() as i32 + 2 > IMAGE_MAX as i32 {
            step *= 2;
        }
        step
    }

    /// The texels `view` needs across and down, and the margin the slot leaves on each axis.
    fn map_extent(&self, view: View, most: i32) -> ([i32; 2], [i32; 2], i32) {
        let step = self.map_step(view.zoom);
        let need = |gui: f32| (gui / view.zoom / step as f32).ceil() as i32 + 2;
        let margin = |need: i32| most.min((IMAGE_MAX as i32 - need) / 2).max(0);
        let need = [need(self.gui[0]), need(self.gui[1])];
        (need, [margin(need[0]), margin(need[1])], step)
    }

    /// The full map's image frame for `view`: what the screen shows and a margin round it of
    /// up to `most` texels, as far as the slot allows, moved `lead` blocks towards where the
    /// view is going (no further than keeps the view itself inside).
    pub(crate) fn map_frame(&self, view: View, lead: [f64; 2], most: i32) -> Frame {
        let [w, h] = self.gui;
        let (need, margin, step) = self.map_extent(view, most);
        let shift = |lead: f64, margin: i32| {
            let most = f64::from((margin - 2).max(0) * step);
            lead.clamp(-most, most)
        };
        let left = view.cx + shift(lead[0], margin[0]) - f64::from(w / view.zoom / 2.0);
        let top = view.cz + shift(lead[1], margin[1]) - f64::from(h / view.zoom / 2.0);
        let snap = |v: f64, margin: i32| ((v.floor() as i32).div_euclid(step) - margin) * step;
        Frame {
            x0: snap(left, margin[0]),
            z0: snap(top, margin[1]),
            step,
            w: (need[0] + 2 * margin[0]) as usize,
            h: (need[1] + 2 * margin[1]) as usize,
        }
    }

    /// Whether `frame` holds everything `view` shows.
    pub(crate) fn covers(&self, frame: Frame, view: View) -> bool {
        let [w, h] = self.gui;
        let (hw, hh) = (f64::from(w / view.zoom / 2.0), f64::from(h / view.zoom / 2.0));
        f64::from(frame.x0) <= view.cx - hw
            && f64::from(frame.z0) <= view.cz - hh
            && f64::from(frame.x0 + frame.w as i32 * frame.step) >= view.cx + hw
            && f64::from(frame.z0 + frame.h as i32 * frame.step) >= view.cz + hh
    }

    pub(crate) fn zoom_min(&self) -> f32 {
        (self.gui[0] / SPAN_MAX).max(ZOOM_MIN)
    }

    /// The block under GUI point `at`.
    pub(crate) fn block_at(&self, view: View, at: [f32; 2]) -> [f64; 2] {
        [
            view.cx + f64::from((at[0] - self.gui[0] / 2.0) / view.zoom),
            view.cz + f64::from((at[1] - self.gui[1] / 2.0) / view.zoom),
        ]
    }

    pub(crate) fn screen_at(&self, view: View, x: f64, z: f64) -> [f32; 2] {
        [
            ((x - view.cx) as f32) * view.zoom + self.gui[0] / 2.0,
            ((z - view.cz) as f32) * view.zoom + self.gui[1] / 2.0,
        ]
    }

    // ---- the view, each frame ----

    /// The frame's player: between the last two ticks by `partial`, turned as the view is now.
    pub(crate) fn frame_player(&mut self, host: &dyn ClientHost, partial: f32) {
        let Some((old, new)) = self.ticks else { return };
        let jumped = (0..3).any(|i| (new[i] - old[i]).abs() > crate::TELEPORT);
        let t = if jumped { 1.0 } else { f64::from(partial.clamp(0.0, 1.0)) };
        let pos = [0, 1, 2].map(|i| old[i] + (new[i] - old[i]) * t);
        let yaw = host
            .local_player()
            .and_then(|p| p.entity().rotation(host))
            .map_or(self.player.map_or(0.0, |p| p.yaw), |r| r[0]);
        self.player = Some(Player { pos, yaw });
    }

    /// Starts a drag at the cursor.
    pub(crate) fn begin_drag(&mut self, mouse: [f32; 2]) {
        let Some(view) = self.view else { return };
        self.stop_motion();
        self.drag = Some(Drag { block: self.block_at(view, mouse) });
        self.cursor = mouse;
    }

    /// Puts the dragged block under the cursor at `mouse`.
    pub(crate) fn drag_to(&mut self, mouse: [f32; 2]) {
        let (Some(drag), Some(view)) = (self.drag, &mut self.view) else { return };
        self.cursor = mouse;
        if self.zoom_anim.is_none() {
            let before = (view.cx, view.cz);
            view.cx = drag.block[0] - f64::from((mouse[0] - self.gui[0] / 2.0) / view.zoom);
            view.cz = drag.block[1] - f64::from((mouse[1] - self.gui[1] / 2.0) / view.zoom);
            if before != (view.cx, view.cz) {
                self.panned = self.clock;
            }
        }
    }

    /// Whether the view is being panned: moved by a drag, a glide or a slide just now.
    pub(crate) fn panning(&self) -> bool {
        self.glide.is_some() || self.slide.is_some() || self.goto.is_some() || self.clock - self.panned < SETTLE
    }

    /// Lets the drag go: fast enough, the view glides on.
    pub(crate) fn end_drag(&mut self) {
        if self.drag.take().is_none() {
            return;
        }
        let zoom = self.view.map_or(1.0, |v| f64::from(v.zoom));
        if let (Some(first), Some(last)) = (self.trail.front().copied(), self.trail.back().copied())
            && last.0 - first.0 >= 0.02
        {
            let dt = last.0 - first.0;
            let mut v = [(last.1 - first.1) / dt, (last.2 - first.2) / dt];
            let speed = (v[0] * v[0] + v[1] * v[1]).sqrt() * zoom;
            if speed >= GLIDE_MIN {
                let k = (GLIDE_MAX / speed).min(1.0);
                v = [v[0] * k, v[1] * k];
                self.glide = Some(Glide { v, start: self.clock });
            }
        }
        self.trail.clear();
    }

    /// Stops whatever moves the view by itself: a click, the wheel or a key takes over.
    pub(crate) fn stop_motion(&mut self) {
        self.glide = None;
        self.slide = None;
        self.trail.clear();
    }

    /// Starts the slide back to the player.
    pub(crate) fn begin_slide(&mut self) {
        let (Some(p), Some(view)) = (self.player, self.view) else { return };
        self.stop_motion();
        self.drag = None;
        self.zoom_anim = None;
        self.slide = Some(Slide { off: [view.cx - p.pos[0], view.cz - p.pos[2]], start: self.clock });
    }

    /// Starts a slide to block `x`, `z` (a waypoint picked in the list).
    pub(crate) fn begin_slide_to(&mut self, x: f64, z: f64) {
        let Some(view) = self.view else { return };
        self.stop_motion();
        self.drag = None;
        self.zoom_anim = None;
        self.goto = Some(([view.cx, view.cz], [x, z], self.clock));
    }

    /// Moves the view on by one frame of `dt` seconds: the wheel's zoom, then the drag (the
    /// cursor is read here, once a frame, not added up from the mouse's events), else the
    /// glide, else a slide.
    pub(crate) fn animate_view(&mut self, dt: f64) {
        let before = self.view;
        let [w, h] = self.gui;
        if let (Some(drag), Some(anim)) = (self.drag, &mut self.zoom_anim) {
            // Zooming while dragging: the dragged block stays under the cursor.
            anim.block = drag.block;
            anim.mouse = self.cursor;
        }
        if let (Some(anim), Some(view)) = (self.zoom_anim, &mut self.view) {
            let t = ((self.clock - anim.start) / ZOOM_TIME).clamp(0.0, 1.0) as f32;
            // Ease out, in ratios: each frame multiplies the zoom, as each notch does.
            let eased = 1.0 - (1.0 - t) * (1.0 - t);
            view.zoom = anim.from * (anim.to / anim.from).powf(eased);
            view.cx = anim.block[0] - f64::from((anim.mouse[0] - w / 2.0) / view.zoom);
            view.cz = anim.block[1] - f64::from((anim.mouse[1] - h / 2.0) / view.zoom);
            if t >= 1.0 {
                self.zoom_anim = None;
            }
        }
        let Some(view) = &mut self.view else { return };
        if let Some(drag) = self.drag {
            view.cx = drag.block[0] - f64::from((self.cursor[0] - w / 2.0) / view.zoom);
            view.cz = drag.block[1] - f64::from((self.cursor[1] - h / 2.0) / view.zoom);
            self.trail.push_back((self.clock, view.cx, view.cz));
            while self.trail.front().is_some_and(|s| self.clock - s.0 > TRAIL) {
                self.trail.pop_front();
            }
        } else if let Some(glide) = self.glide {
            // The way covered between the last frame and this one.
            let left = |at: f64| (1.0 - ((at - glide.start) / GLIDE_TIME).clamp(0.0, 1.0)).powi(3);
            let k = GLIDE_TIME / 3.0 * (left(self.clock - dt) - left(self.clock));
            view.cx += glide.v[0] * k;
            view.cz += glide.v[1] * k;
            if self.clock - glide.start >= GLIDE_TIME {
                self.glide = None;
            }
        } else if let (Some(slide), Some(p)) = (self.slide, self.player) {
            let t = ((self.clock - slide.start) / SLIDE_TIME).clamp(0.0, 1.0);
            let left = (1.0 - t).powi(3);
            view.cx = p.pos[0] + slide.off[0] * left;
            view.cz = p.pos[2] + slide.off[1] * left;
            if t >= 1.0 {
                self.slide = None;
            }
        } else if let Some((from, to, start)) = self.goto {
            let t = ((self.clock - start) / SLIDE_TIME).clamp(0.0, 1.0);
            let left = (1.0 - t).powi(3);
            view.cx = to[0] + (from[0] - to[0]) * left;
            view.cz = to[1] + (from[1] - to[1]) * left;
            if t >= 1.0 {
                self.goto = None;
            }
        }
        if self.drag.is_some() || self.glide.is_some() || self.slide.is_some() {
            self.goto = None;
        }
        let view = *view;
        self.vel = match before {
            Some(b) if dt > 1e-4 => [(view.cx - b.cx) / dt, (view.cz - b.cz) / dt],
            _ => [0.0; 2],
        };
        if self.zoom_anim.is_none() && before.is_some_and(|b| (b.cx, b.cz) != (view.cx, view.cz)) {
            self.panned = self.clock;
        }
    }

    // ---- the image ----

    /// What the shown layer looks like now, for telling an image of it stale.
    pub(crate) fn look(&self) -> Look {
        Look {
            cave: if self.shown_cave() { self.band } else { None },
            tints: self.config.tints,
            tint_revision: if self.config.tints { self.legend.revision } else { 0 },
        }
    }

    fn upload(&mut self, host: &mut dyn ClientHost, full: bool, frame: Frame, revision: u64, look: Look) {
        if host.mod_image(frame.w as u32, frame.h as u32, &self.pixels) {
            self.image = Some(Image { full, soft: false, frame, revision, look, at: self.now });
            self.stats.uploads += 1;
            self.stats.texels += (frame.w * frame.h) as u64;
            self.stats.while_panning += u64::from(full && self.panning());
        }
    }

    /// Paints the image slot when what it should show changed. Moving never waits for this:
    /// every frame draws the image it has where the view is now.
    pub(crate) fn refresh_image(&mut self, host: &mut dyn ClientHost) {
        self.refresh_tints();
        let look = self.look();
        let on_map = matches!(self.ui, Ui::Map | Ui::Edit(crate::bubble::Edit { from_map: true, .. }));
        match (on_map, self.view, self.player) {
            (true, Some(view), _) => self.refresh_map(host, view, look),
            (false, _, Some(player)) if self.config.minimap => self.refresh_minimap(host, player, look),
            _ => {}
        }
    }

    fn refresh_map(&mut self, host: &mut dyn ClientHost, view: View, look: Look) {
        // A zoom on its way is painted for where it ends, when that is wider.
        let zooming = self.zoom_anim.is_some();
        let wide = self.zoom_anim.map_or(view.zoom, |a| a.to.min(view.zoom));
        let aim = View { zoom: wide, ..view };
        let panning = self.panning() && !zooming;
        // The regions the view and its surroundings are in, for the image and the mosaic.
        let around = self.map_frame(aim, [0.0; 2], ZOOM_MARGIN);
        let store = if look.cave.is_some() { &mut self.cave } else { &mut self.surface };
        render::prepare(store, around);
        if panning && !(crate::TEST && self.probe.v02) {
            // Never while the view moves: this frame draws what there is.
            return;
        }
        let step = around.step;
        let revision = self.shown().revision;
        let image = self.image.filter(|i| i.full && i.look == look);
        let covered = image.is_some_and(|i| i.frame.step == step && self.covers(i.frame, view));
        let since = image.map_or(u64::MAX, |i| self.now.saturating_sub(i.at));
        let changed = image.is_none_or(|i| i.revision != revision);
        if covered && !(changed && since >= 250) && since < REPAINT {
            return;
        }
        // New terrain under a view the image covers is painted into the same frame. Else: a
        // small margin in the middle of a zoom, where the upload has to be quick, a wide one
        // when the view has come to rest.
        let frame = match image {
            Some(i) if covered => i.frame,
            _ if zooming || (crate::TEST && self.probe.v02) => around,
            _ => self.map_frame(aim, [0.0; 2], MAP_MARGIN),
        };
        let mut pixels = std::mem::take(&mut self.pixels);
        let tints = std::mem::take(&mut self.tints);
        let store = if look.cave.is_some() { &mut self.cave } else { &mut self.surface };
        render::compose(store, &self.palette, if look.tints { &tints } else { &[] }, frame, &mut pixels);
        self.tints = tints;
        self.pixels = pixels;
        self.upload(host, true, frame, revision, look);
    }
}

/// The samples a drag's speed is taken from: the clock and the view's centre.
pub type Trail = VecDeque<(f64, f64, f64)>;

#[cfg(test)]
mod tests {
    use super::*;

    fn state() -> State {
        State { gui: [960.0, 540.0], ..State::default() }
    }

    #[test]
    fn frames_fit_the_slot_and_cover_the_screen() {
        let mut st = State { gui: [1920.0, 1080.0], ..State::default() };
        for zoom in [0.125f32, 0.25, 0.37, 0.5, 1.0, 1.15, 2.0, 8.0] {
            let view = View { cx: -300.5, cz: 77.25, zoom };
            for (lead, most) in [([0.0, 0.0], 160), ([1e6, -1e6], 160), ([-37.0, 12.0], 48)] {
                let f = st.map_frame(view, lead, most);
                assert!(f.w <= IMAGE_MAX && f.h <= IMAGE_MAX, "{f:?}");
                assert!(st.covers(f, view), "{f:?} {lead:?}");
            }
            assert!(!st.covers(st.map_frame(view, [0.0; 2], 160), View { cx: view.cx + 4000.0, ..view }));
        }
        st.config.size = 200.0;
        for scale in crate::MINIMAP_ZOOMS {
            st.config.scale = scale;
            let f = st.minimap_frame(Player { pos: [-5.5, 70.0, 1000.2], yaw: 0.0 }, 200.0);
            assert!(f.w <= IMAGE_MAX);
            assert!(f.w as f32 * f.step as f32 >= 200.0 * scale + 16.0);
            assert_eq!(f.x0.rem_euclid(f.step), 0);
        }
    }

    #[test]
    fn a_drag_keeps_its_block_under_the_cursor() {
        let mut st = state();
        st.view = Some(View { cx: 100.0, cz: -40.0, zoom: 1.7 });
        let grab = [301.25, 207.5];
        let block = st.block_at(st.view.unwrap(), grab);
        st.begin_drag(grab);
        for (i, cursor) in [[310.0, 211.0], [477.3, 96.2], [12.0, 500.9]].into_iter().enumerate() {
            st.cursor = cursor;
            st.clock += 0.007;
            st.animate_view(0.007);
            let under = st.block_at(st.view.unwrap(), cursor);
            assert!((under[0] - block[0]).abs() < 1e-3 && (under[1] - block[1]).abs() < 1e-3, "frame {i}");
        }
    }

    #[test]
    fn a_drag_within_one_frame_still_moves_the_map() {
        // Press, move and release between two frames: the events alone carry it.
        let mut st = state();
        st.view = Some(View { cx: 0.0, cz: 0.0, zoom: 2.0 });
        st.begin_drag([100.0, 100.0]);
        st.drag_to([160.0, 130.0]);
        st.end_drag();
        let v = st.view.unwrap();
        assert!((v.cx + 30.0).abs() < 1e-9 && (v.cz + 15.0).abs() < 1e-9, "{v:?}");
        assert!(st.panning());
        st.clock += 0.2;
        assert!(!st.panning());
    }

    #[test]
    fn a_flick_glides_and_stops_within_a_third_of_a_second() {
        let mut st = state();
        st.view = Some(View { cx: 0.0, cz: 0.0, zoom: 1.0 });
        st.begin_drag([400.0, 300.0]);
        // 1200 GUI pixels a second to the right for a tenth of a second.
        for i in 1..=12 {
            st.cursor = [400.0 + i as f32 * 10.0, 300.0];
            st.clock += 1.0 / 120.0;
            st.animate_view(1.0 / 120.0);
        }
        let let_go = st.view.unwrap().cx;
        st.end_drag();
        assert!(st.glide.is_some());
        let mut frames = 0;
        while st.glide.is_some() {
            st.clock += 1.0 / 120.0;
            st.animate_view(1.0 / 120.0);
            frames += 1;
            assert!(frames < 80);
        }
        let glided = let_go - st.view.unwrap().cx;
        assert!(frames as f64 / 120.0 > 0.2, "{frames}");
        assert!(glided > 60.0 && glided < 140.0, "{glided}");
        // A drag that stopped before the button came up stays put.
        st.begin_drag([400.0, 300.0]);
        for _ in 0..12 {
            st.clock += 1.0 / 120.0;
            st.animate_view(1.0 / 120.0);
        }
        st.end_drag();
        assert!(st.glide.is_none());
    }

    #[test]
    fn space_slides_to_the_player_in_a_quarter_second() {
        let mut st = state();
        st.player = Some(Player { pos: [50.0, 64.0, -20.0], yaw: 0.0 });
        st.view = Some(View { cx: 400.0, cz: 300.0, zoom: 2.0 });
        st.begin_slide();
        st.clock += 0.05;
        st.animate_view(0.05);
        let part = st.view.unwrap();
        assert!(part.cx < 400.0 && part.cx > 50.0, "on its way, not there: {part:?}");
        for _ in 0..30 {
            st.clock += 0.01;
            st.animate_view(0.01);
        }
        let v = st.view.unwrap();
        assert!(st.slide.is_none() && (v.cx - 50.0).abs() < 1e-9 && (v.cz + 20.0).abs() < 1e-9);
    }

    #[test]
    fn a_moving_view_is_painted_ahead() {
        let st = state();
        let view = View { cx: 0.0, cz: 0.0, zoom: 2.0 };
        let still = st.map_frame(view, [0.0; 2], 160);
        let east = st.map_frame(view, [80.0, 0.0], 160);
        assert!(east.x0 > still.x0 && st.covers(east, view));
        assert!(st.covers(east, View { cx: 200.0, ..view }), "{east:?}");
        assert!(!st.covers(still, View { cx: 200.0, ..view }), "{still:?}");
    }
}
