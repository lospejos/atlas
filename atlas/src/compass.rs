//! The compass bar: a slim strip centred at the top of the screen with the four directions
//! (and ticks between them) sliding past as you turn, and this dimension's waypoints at their
//! bearings. The tracked one is drawn larger with its name and distance under it in the small
//! type of the in-world labels, or, while it is out of view, as an arrow at the end of the bar
//! to turn towards. While the enlarged minimap grows over it, the bar fades out of its way.
//!
//! Everything is placed on whole screen pixels and worked out from this frame's view alone, so
//! nothing shimmers or lags while turning. The ends fade out.
use mod_api::DrawContext;

use crate::State;
use crate::gfx::{self, argb};
use crate::waypoints::{Waypoint, distance_text};

/// The bar's width in GUI pixels: as wide as the hotbar, a GUI pixel a degree.
const WIDTH: f32 = 180.0;
/// Degrees shown either side of straight ahead.
const HALF_FOV: f32 = 90.0;
/// The bar's top edge and height, GUI pixels.
const TOP: f32 = 3.0;
const HEIGHT: f32 = 12.0;
/// The stretch at each end over which the bar fades out, GUI pixels.
const FADE: f32 = 40.0;
/// The band's opacity in its middle (0 to 255); 0 draws no band.
const BAND: f32 = 64.0;
/// The waypoints in a row of their own under the band (true), or on it (false).
const MARKS_BELOW: bool = true;
/// Small ticks every 15 degrees between the named directions.
const MINOR_TICKS: bool = false;
const NAMES: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];

/// A waypoint on the bar.
struct Mark<'a> {
    wp: &'a Waypoint,
    x: f32,
    half: f32,
    /// Out of view to the left (-1) or right (1): drawn as an arrow at that end.
    off: i8,
}

impl State {
    /// Draws the compass bar; returns how far down the screen it reaches, in GUI pixels.
    pub(crate) fn draw_compass(&self, draw: &mut dyn DrawContext) -> f32 {
        let Some(player) = self.player else { return 0.0 };
        let scale = self.scale.max(1.0);
        // On whole screen pixels: the finest step there is, so nothing blurs or wobbles.
        let snap = |v: f32| (v * scale).round() / scale;
        let [w, _] = draw.size();
        let cx = (w / 2.0).round();
        let left = cx - WIDTH / 2.0;
        let bottom = TOP + HEIGHT;
        let mid = TOP + HEIGHT / 2.0;
        // The waypoints' row.
        let row = if MARKS_BELOW { bottom + 6.0 } else { mid };
        // Degrees clockwise from north that the view faces.
        let heading = (player.yaw + 180.0).rem_euclid(360.0);
        let per_degree = WIDTH / (2.0 * HALF_FOV);
        let rel = |bearing: f32| (bearing - heading + 540.0).rem_euclid(360.0) - 180.0;
        // How much of the bar shows at `x`: all of it in the middle, nothing at the ends.
        let fade = |x: f32| {
            let t = ((WIDTH / 2.0 - (x - cx).abs()) / FADE).clamp(0.0, 1.0);
            t * t * (3.0 - 2.0 * t)
        };
        // Out of the way of the enlarged minimap when it reaches the bar.
        let show = 1.0 - self.enlarged_over([left - 2.0, 0.0, WIDTH + 4.0, bottom + 24.0]);
        if show <= 0.02 {
            return 0.0;
        }
        let alpha = |a: f32, most: f32| ((a * most * show).round().clamp(0.0, 255.0) as u32) << 24;

        if BAND > 0.0 {
            draw.fill([left + FADE, TOP, WIDTH - 2.0 * FADE, HEIGHT], alpha(1.0, BAND));
            for i in 0..FADE as i32 {
                let a = fade(left + i as f32 + 0.5);
                draw.fill([left + i as f32, TOP, 1.0, HEIGHT], alpha(a, BAND));
                draw.fill([left + WIDTH - 1.0 - i as f32, TOP, 1.0, HEIGHT], alpha(a, BAND));
            }
        }

        // The waypoints: the tracked one first, then nearest first; one that would overlap a
        // marker already placed is left out rather than piled on it.
        let mut list: Vec<(&Waypoint, f32, f64)> = self
            .here()
            .map(|(_, wp)| {
                let dx = f64::from(wp.pos[0]) + 0.5 - player.pos[0];
                let dz = f64::from(wp.pos[2]) + 0.5 - player.pos[2];
                let bearing = dx.atan2(-dz).to_degrees() as f32;
                (wp, rel(bearing), (dx * dx + dz * dz).sqrt())
            })
            .collect();
        list.sort_by(|a, b| b.0.tracked.cmp(&a.0.tracked).then(a.2.total_cmp(&b.2)));
        let edge = WIDTH / 2.0 - FADE + 4.0;
        let mut marks: Vec<Mark> = Vec::new();
        let mut label = None;
        for (wp, r, dist) in list {
            let (x, off) = if wp.tracked && (r * per_degree).abs() > edge {
                (cx + edge * r.signum(), r.signum() as i8)
            } else {
                (cx + r * per_degree, 0)
            };
            if off == 0 && fade(x) < 0.15 {
                continue;
            }
            let x = snap(x);
            // An arrow at an end is wider than a marker.
            let half = if off != 0 { 5.0 } else if wp.tracked { 3.0 } else { 2.0 };
            if marks.iter().any(|m| (m.x - x).abs() < m.half + half + 3.0) {
                continue;
            }
            if wp.tracked {
                label = Some((wp, x, dist));
            }
            marks.push(Mark { wp, x, half, off });
        }

        // The directions, faint where a marker sits on them.
        for step in 0..(if MINOR_TICKS { 24 } else { 8 }) {
            let deg = step as f32 * if MINOR_TICKS { 15.0 } else { 45.0 };
            let r = rel(deg);
            if r.abs() > HALF_FOV {
                continue;
            }
            let x = snap(cx + r * per_degree);
            let mut a = fade(x);
            if a <= 0.02 {
                continue;
            }
            if !MARKS_BELOW && marks.iter().any(|m| m.off == 0 && (m.x - x).abs() < m.half + 5.0) {
                a *= 0.25;
            }
            let whole = deg as i32;
            if whole % 90 == 0 {
                let name = NAMES[(whole / 45) as usize];
                let tw = draw.text_width(name);
                if a * 255.0 >= 8.0 {
                    let color = if whole == 0 { 0x00FF_6B6B } else { 0x00FF_FFFF };
                    draw.text(name, [snap(x - (tw - 1.0) / 2.0), TOP + 2.0], alpha(a, 255.0) | color, true);
                }
            } else if whole % 45 == 0 {
                draw.fill([snap(x - 0.5), mid - 2.0, 1.0, 4.0], alpha(a, 200.0) | 0x00C8_C8C8);
            } else {
                draw.fill([snap(x - 0.5), bottom - 3.0, 1.0, 2.0], alpha(a, 150.0) | 0x00A0_A0A0);
            }
        }

        // Straight ahead: a small notch under the bar.
        draw.fill([cx - 1.5, bottom, 3.0, 1.0], alpha(1.0, 192.0) | 0x00FF_FFFF);
        draw.fill([cx - 0.5, bottom + 1.0, 1.0, 1.0], alpha(1.0, 192.0) | 0x00FF_FFFF);

        // The markers can't fade: they go once the minimap is half over the bar.
        if show < 0.5 {
            return bottom + 2.0;
        }
        for m in marks.iter().rev() {
            if m.off != 0 {
                let angle = if m.off < 0 { -90.0 } else { 90.0 };
                gfx::arrow(draw, [m.x, row], angle, 4.0, argb(m.wp.color));
            } else {
                gfx::waypoint_icon(draw, m.wp, [m.x, row], m.half, fade(m.x) < 0.6);
            }
        }

        let mut reach = if MARKS_BELOW && !marks.is_empty() { row + 4.0 } else { bottom + 2.0 };
        if let Some((wp, x, dist)) = label {
            let off = marks.iter().any(|m| m.off != 0 && std::ptr::eq(m.wp, wp));
            reach = self.compass_caption(draw, wp, x, reach + 2.0, dist, off, [left, left + WIDTH]);
        }
        reach
    }

    /// The tracked waypoint's name and distance as one quiet line under its marker at `x`,
    /// its top at `top`: the small type of the in-world labels, on whole screen pixels, kept
    /// between `span`'s ends. Its icon leads only while the marker is an arrow at an end (else
    /// the marker right above is the icon). Answers where the line ends below.
    #[allow(clippy::too_many_arguments)]
    fn compass_caption(
        &self,
        draw: &mut dyn DrawContext,
        wp: &Waypoint,
        x: f32,
        top: f32,
        dist: f64,
        with_icon: bool,
        span: [f32; 2],
    ) -> f32 {
        use mod_api::client::hud_layout::PoseOp;
        use mod_api::{GuiDraw, Text};
        let scale = self.scale.max(1.0);
        let font = crate::label_font(self.gui[1], scale);
        let k = font / scale;
        let snap = |v: f32| (v * scale).round() / scale;
        let name = gfx::fit(draw, &wp.name, 90.0);
        let text = format!("{name} {}", distance_text(dist));
        let tw = draw.text_width(&text).ceil() as i32;
        let icon = with_icon.then(|| crate::icons::LabelIcon::of(wp, font));
        let lead = icon.as_ref().map_or(0, |i| i.width + 2);
        let width = lead + tw;
        // In label units, the line's left end is -width/2 from the origin.
        let half = (width / 2) as f32 * k;
        let ox = x.clamp(span[0] + 2.0 + half, span[1] - 2.0 - half);
        let origin = [snap(ox), snap(top + 2.0 * k)];
        let left = -(width / 2);
        let mut list = vec![
            GuiDraw::Push(vec![PoseOp::Translate(origin[0], origin[1]), PoseOp::Scale(k, k)]),
            GuiDraw::Fill { x0: left - 2, y0: -2, x1: left + width + 2, y1: 9, color: 0x5000_0000 },
        ];
        if let Some(icon) = &icon {
            icon.paint(&mut list, left, 3, wp.tracked);
        }
        list.push(GuiDraw::Text { text: Text::literal(text), x: left + lead, y: 0, color: 0xFFFF_FFFF, shadow: false });
        list.push(GuiDraw::Pop);
        let _ = draw.gui_paint(&list);
        if let Some(icon) = &icon {
            icon.draw_after(draw, origin, k, left, 3, wp.tracked);
        }
        origin[1] + 9.0 * k
    }
}

#[cfg(test)]
mod tests {
    /// Bearings: north is straight up the screen's middle, east to the right.
    #[test]
    fn bearings_go_clockwise_from_north() {
        let bearing = |dx: f64, dz: f64| dx.atan2(-dz).to_degrees().rem_euclid(360.0);
        assert!((bearing(0.0, -1.0) - 0.0).abs() < 1e-6);
        assert!((bearing(1.0, 0.0) - 90.0).abs() < 1e-6);
        assert!((bearing(0.0, 1.0) - 180.0).abs() < 1e-6);
        assert!((bearing(-1.0, 0.0) - 270.0).abs() < 1e-6);
        // The view's yaw: 180 faces north, -90 east.
        let heading = |yaw: f32| (yaw + 180.0).rem_euclid(360.0);
        assert_eq!(heading(180.0), 0.0);
        assert_eq!(heading(-90.0), 90.0);
    }
}
