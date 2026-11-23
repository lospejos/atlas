//! The world map screen: the map itself, the bar of switches at its top, the waypoint list on
//! its right and the status lines at its bottom, and what clicks, the wheel and keys do there.
use mod_api::{ClientHost, DrawContext, Flow};

use crate::bubble::Edit;
use crate::gfx::{self, PANEL_BG, centered, fit, inside, marker};
use crate::mapview::{View, ZOOM_MAX, ZOOM_STEP, ZoomAnim};
use crate::overlays::{OverlayMarker, OverlayQuery};
use crate::store::UNSEEN;
use crate::scrollbar;
use crate::waypoints::{self, NETHER, OVERWORLD, Waypoint, distance_text};
use crate::{ID, KEY_MAP, Mode, SPRITE, State, UNSEEN_COLOR, Ui};

const PANEL_W: f32 = 140.0;
const ROW_H: f32 = 12.0;

/// What a click on the world map lands on.
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Hit {
    Mode(Mode),
    /// An on/off setting, by its key.
    Toggle(&'static str),
    Overlay(usize),
    Export,
    /// The list's panel, where it has nothing else.
    Panel,
    /// A waypoint's row, its `x` and its beam switch, by the waypoint's id (`Waypoint::id`):
    /// a click lands on the waypoint drawn there even when the list changed since.
    Row(u64),
    RowDelete(u64),
    RowBeam(u64),
    /// The list's scrollbar.
    ListBar,
    /// The list's search box, and its clear button.
    Search,
    SearchClear,
    /// A structure marker of the explore overlay, by its place in `Explore::picks`.
    Structure(usize),
    /// A waypoint shared in chat, by its id: take it, or drop it.
    Inbox(u64, bool),
    Marker(u64),
    /// An explore overlay's switch: 0 grid, 1 slime chunks, 2 structures, 3 the filter panel.
    Explore(usize),
    /// A structure set's switch in the filter panel, and the seed field there.
    Set(usize),
    Seed,
}

/// Pixels either side of the list's scrollbar that take the mouse too.
const LIST_BAR_PAD: f32 = 2.0;

/// The top of the list's first row, below its title and search box.
const LIST_TOP: f32 = 30.0;

/// A biome id in words: `minecraft:dark_forest` is "Dark Forest".
fn biome_words(id: &str) -> String {
    crate::radar::pretty(id)
}

/// The waypoint list's search box: what is typed, and whether it takes the keys.
#[derive(Default)]
pub struct Search {
    pub text: String,
    pub focus: bool,
}

impl Search {
    pub fn type_char(&mut self, c: char) {
        if !c.is_control() && self.text.chars().count() < waypoints::NAME_MAX {
            self.text.push(c);
        }
    }

    /// Whether a waypoint called `name` is shown: its name holds the text, any case.
    pub fn matches(&self, name: &str) -> bool {
        let needle = self.text.trim();
        needle.is_empty() || name.to_lowercase().contains(&needle.to_lowercase())
    }
}

/// A beam as a small glyph, `x` its left edge, `y` its top (9 x 10 pixels): a light shaft
/// standing on a block. Lit in `rgb`; dark when off.
fn beam_glyph(draw: &mut dyn DrawContext, x: f32, y: f32, rgb: u32, on: bool, hot: bool) {
    let (core, halo, base) = match (on, hot) {
        (true, _) => (0xFFFF_FFFF, 0xB000_0000 | rgb, if hot { 0xFFD0_D0D0 } else { 0xFFA8_A8A8 }),
        (false, true) => (0xFF90_90A0, 0xFF50_5060, 0xFF90_90A0),
        (false, false) => (0xFF4C_4C58, 0, 0xFF4C_4C58),
    };
    if halo != 0 {
        draw.fill([x + 3.0, y, 3.0, 7.0], halo);
    }
    draw.fill([x + 4.0, y, 1.0, 7.0], core);
    draw.fill([x + 1.0, y + 7.0, 7.0, 2.0], base);
    // The block's dark lower edge.
    draw.fill([x + 1.0, y + 9.0, 7.0, 1.0], 0xA000_0000);
}

/// The side of a row's beam switch: a square with the beam inside, as tall as a line of text.
const BEAM_BOX: f32 = 11.0;

/// A waypoint's beam switch in the list: a dark square with a one-pixel rim and the beam inside
/// it, `at` its top left (`BEAM_BOX` square). On: the shaft lit in the waypoint's colour `rgb`
/// over a light block. Off: the shaft dark, the block dim. Lighter under the cursor.
fn beam_button(draw: &mut dyn DrawContext, at: [f32; 2], rgb: u32, on: bool, hot: bool) {
    let [x, y] = at;
    let (rim, plate) = match (on, hot) {
        (true, false) => (0xFF5C_6070, 0xFF24_2834),
        (true, true) => (0xFFA8_ACBC, 0xFF34_3A4C),
        (false, false) => (0xFF38_3840, 0xFF10_1014),
        (false, true) => (0xFF80_8090, 0xFF24_2630),
    };
    draw.fill([x, y, BEAM_BOX, BEAM_BOX], rim);
    draw.fill([x + 1.0, y + 1.0, BEAM_BOX - 2.0, BEAM_BOX - 2.0], plate);
    // Inside, 9 x 9: the shaft over rows 1 to 5, the block on rows 6 and 7.
    let (ix, iy) = (x + 1.0, y + 1.0);
    let (core, halo, block) = match (on, hot) {
        (true, _) => (0xFFFF_FFFF, 0xC000_0000 | rgb, if hot { 0xFFE0_E0E0 } else { 0xFFB4_B4B4 }),
        (false, true) => (0xFF90_90A0, 0, 0xFF70_7080),
        (false, false) => (0xFF4C_4C58, 0, 0xFF40_4048),
    };
    if halo != 0 {
        draw.fill([ix + 3.0, iy + 1.0, 3.0, 5.0], halo);
    }
    draw.fill([ix + 4.0, iy + 1.0, 1.0, 5.0], core);
    draw.fill([ix + 1.0, iy + 6.0, 7.0, 2.0], block);
}

/// A magnifying glass, `x`, `y` its top left (7 x 7 pixels).
fn search_glyph(draw: &mut dyn DrawContext, x: f32, y: f32, color: u32) {
    draw.fill([x + 1.0, y, 3.0, 1.0], color);
    draw.fill([x + 1.0, y + 4.0, 3.0, 1.0], color);
    draw.fill([x, y + 1.0, 1.0, 3.0], color);
    draw.fill([x + 4.0, y + 1.0, 1.0, 3.0], color);
    draw.fill([x + 4.0, y + 4.0, 1.0, 1.0], color);
    draw.fill([x + 5.0, y + 5.0, 1.0, 1.0], color);
    draw.fill([x + 6.0, y + 6.0, 1.0, 1.0], color);
}

impl State {
    /// The waypoints in the list's order: this dimension's nearest first, then the other
    /// dimensions' by dimension and name.
    pub(crate) fn listing(&self) -> Vec<usize> {
        let at = self.player.map_or([0.0; 3], |p| p.pos);
        let far = |w: &Waypoint| {
            let (dx, dz) = (f64::from(w.pos[0]) + 0.5 - at[0], f64::from(w.pos[2]) + 0.5 - at[2]);
            dx * dx + dz * dz
        };
        let mut here: Vec<usize> = (0..self.waypoints.len()).filter(|i| self.waypoints[*i].dim == self.dim).collect();
        here.sort_by(|a, b| far(&self.waypoints[*a]).total_cmp(&far(&self.waypoints[*b])));
        let mut other: Vec<usize> = (0..self.waypoints.len()).filter(|i| self.waypoints[*i].dim != self.dim).collect();
        other.sort_by(|a, b| {
            let (a, b) = (&self.waypoints[*a], &self.waypoints[*b]);
            (&a.dim, &a.name).cmp(&(&b.dim, &b.name))
        });
        here.extend(other);
        here
    }

    /// The list's rows as the search leaves them: the shared ones waiting (by their place in
    /// the inbox), then the waypoints in `listing`'s order.
    fn rows(&self) -> (Vec<usize>, Vec<usize>) {
        let inbox = (0..self.inbox.len()).filter(|i| self.search.matches(&self.inbox[*i].wp.name)).collect();
        let list = self.listing().into_iter().filter(|i| self.search.matches(&self.waypoints[*i].name)).collect();
        (inbox, list)
    }

    /// The waypoint list's panel, and how many rows fit.
    fn panel(&self) -> ([f32; 4], usize) {
        let (inbox, list) = self.rows();
        let rows = inbox.len() + list.len();
        let fit = (((self.gui[1] - 74.0) / ROW_H).floor().max(1.0)) as usize;
        let shown = rows.clamp(1, fit);
        ([self.gui[0] - PANEL_W - 4.0, 4.0, PANEL_W, LIST_TOP + shown as f32 * ROW_H + 2.0], shown)
    }

    /// The other dimension's name and the block there that `x`, `z` here leads to.
    pub(crate) fn other_side(&self, x: f64, z: f64) -> Option<(&'static str, i32, i32)> {
        match self.dim.as_str() {
            OVERWORLD => Some(("Nether", (x / 8.0).floor() as i32, (z / 8.0).floor() as i32)),
            NETHER => Some(("Overworld", (x * 8.0).floor() as i32, (z * 8.0).floor() as i32)),
            _ => None,
        }
    }

    /// Shows `text` over the world map for a few seconds.
    pub(crate) fn note(&mut self, text: String) {
        self.notice = Some((text, self.now + 6000));
    }

    /// Whether a marker at `at` would lie under a panel, the bar or a status line.
    pub(crate) fn covered(&self, at: [f32; 2]) -> bool {
        self.covers.iter().any(|r| inside([r[0] - 6.0, r[1] - 6.0, r[2] + 12.0, r[3] + 12.0], at))
    }

    /// Whether rectangle `r` reaches under a panel, a button or a status line.
    fn covers_rect(&self, r: [f32; 4]) -> bool {
        self.covers.iter().any(|c| r[0] < c[0] + c[2] && c[0] < r[0] + r[2] && r[1] < c[1] + c[3] && c[1] < r[1] + r[3])
    }

    fn hit(&self, mouse: [f32; 2]) -> Option<Hit> {
        self.hits.iter().rev().find(|(_, rect)| inside(*rect, mouse)).map(|(hit, _)| *hit)
    }

    // ---- drawing ----

    /// Draws the world map. `live` is off while the bubble is over it: nothing is under the
    /// cursor then.
    pub(crate) fn draw_map(&mut self, host: &dyn ClientHost, draw: &mut dyn DrawContext, mouse: [f32; 2], live: bool) {
        let Some(view) = self.view else { return };
        let [w, h] = self.gui;
        let mouse = if live { mouse } else { [-1000.0; 2] };
        let mut hits = std::mem::take(&mut self.hits);
        hits.clear();
        if self.bar_item.is_none() {
            self.bar_item = mod_api::Identifier::parse("minecraft:diamond").ok().and_then(|id| host.item_key(&id));
        }
        if self.radar.sprites.is_empty() {
            self.radar.sprites = (0..crate::faces::FACES.len()).map(crate::faces::sprites).collect();
        }
        let mut tip: Option<String> = None;
        // The name of the radar icon or structure under the cursor, shown beside it.
        let mut label: Option<String> = None;
        draw.fill([0.0, 0.0, w, h], UNSEEN_COLOR);
        self.draw_mosaic(draw, view);
        // The image in the slot, even one of the layer just left: the new one replaces it whole.
        if let Some(image) = self.image.filter(|i| i.full) {
            let f = image.frame;
            // The image as it lies under this frame's view, to a fraction of a pixel.
            let at = self.screen_at(view, f64::from(f.x0), f64::from(f.z0));
            let k = f.step as f32 * view.zoom;
            let _ = draw.push_clip([0.0, 0.0, w, h]);
            draw.sprite(SPRITE, [0.0, 0.0, f.w as f32, f.h as f32], [at[0], at[1], f.w as f32 * k, f.h as f32 * k], None);
            let _ = draw.pop_clip();
        }

        // Overlays: extra marker sets, over the terrain and under everything else.
        let corner = self.block_at(view, [0.0, 0.0]);
        let far_corner = self.block_at(view, [w, h]);
        let query = OverlayQuery {
            dimension: &self.dim,
            bounds: [corner[0], corner[1], far_corner[0], far_corner[1]],
            tick: self.scan_surface.tick,
        };
        let mut marks: Vec<OverlayMarker> = Vec::new();
        for slot in self.overlays.iter_mut().filter(|s| s.on) {
            slot.overlay.markers(&query, &mut marks);
        }
        for mark in &marks {
            let at = self.screen_at(view, mark.at[0], mark.at[1]);
            match mark.area {
                Some(side) => draw.fill([at[0], at[1], side as f32 * view.zoom, side as f32 * view.zoom], 0x5000_0000 | mark.rgb),
                None => marker(draw, at, mark.rgb, 2.0),
            }
            let reach = mark.area.map_or(5.0, |s| s as f32 * view.zoom);
            let origin = if mark.area.is_some() { at } else { [at[0] - 5.0, at[1] - 5.0] };
            if inside([origin[0], origin[1], reach.max(10.0), reach.max(10.0)], mouse) {
                tip = Some(mark.label.clone());
            }
        }

        self.draw_explore(draw, view, mouse, &mut tip, &mut label);
        // A structure marker is a click away from a temporary waypoint there.
        for (i, (at, _, _)) in self.explore.picks.iter().enumerate() {
            hits.push((Hit::Structure(i), [at[0] - 5.0, at[1] - 5.0, 10.0, 10.0]));
            if inside([at[0] - 5.0, at[1] - 5.0, 10.0, 10.0], mouse)
                && let Some(words) = &tip
            {
                tip = Some(format!("{words}   Click: temporary waypoint here"));
            }
        }

        // The entities near the player.
        let on_map = |x: f64, z: f64| Some(self.screen_at(view, x, z)).filter(|at| !self.covered(*at));
        if let Some(name) = self.draw_blips(draw, f64::from(self.partial), Some(mouse), 1.5, on_map) {
            label = Some(name.clone());
            tip = Some(name);
        }

        // The waypoints of this dimension.
        let mut hovered: Option<usize> = None;
        // Those on the screen, nearest the view's centre first, as many as a frame has time for.
        let until = host.millis() + 2 * crate::DRAW_WORK_MS;
        let mut drawn = 0;
        for (n, (index, wp)) in self.here_nearest([view.cx, view.cz]).into_iter().enumerate() {
            if drawn >= crate::MAP_WAYPOINTS || (n % 32 == 31 && host.millis() >= until) {
                break;
            }
            let at = self.screen_at(view, f64::from(wp.pos[0]) + 0.5, f64::from(wp.pos[2]) + 0.5);
            if !(-8.0..=w + 8.0).contains(&at[0]) || !(-8.0..=h + 8.0).contains(&at[1]) || self.covered(at) {
                continue;
            }
            drawn += 1;
            gfx::waypoint_icon(draw, wp, at, 3.0, false);
            let over = (mouse[0] - at[0]).abs() <= 5.0 && (mouse[1] - at[1]).abs() <= 5.0;
            if over {
                hovered = Some(index);
            }
            hits.push((Hit::Marker(wp.id), [at[0] - 5.0, at[1] - 5.0, 10.0, 10.0]));
            if if self.config.hover_names { over } else { view.zoom >= 0.5 } {
                let tw = draw.text_width(&wp.name);
                let plate = [(at[0] - tw / 2.0 - 2.0).round(), (at[1] + 6.0).round(), tw + 4.0, 10.0];
                // Not under a panel: its dark plate would show through the panel's. Opaque: the
                // game draws a see-through fill under the mobs' faces drawn before it.
                if !self.covers_rect(plate) {
                    draw.fill(plate, 0xFF18_1A1C);
                    draw.text(&wp.name, [plate[0] + 2.0, plate[1] + 1.0], 0xFFFF_FFFF, false);
                }
            }
        }
        if let Some(player) = self.player {
            gfx::player_marker(draw, self.screen_at(view, player.pos[0], player.pos[2]), player.yaw + 180.0, 1.25);
        }

        let below = self.draw_bar(draw, mouse, &mut hits, &mut tip, &mut label);
        self.draw_explore_panel(draw, below, mouse, &mut hits, &mut tip);
        self.draw_list(draw, mouse, &mut hits, &mut tip);
        if inside(self.panel().0, mouse)
            || hits.iter().any(|(hit, r)| !matches!(hit, Hit::Marker(_) | Hit::Structure(_)) && inside(*r, mouse))
        {
            hovered = None;
        }

        // Where the player is, and what is under the cursor.
        let mut status: Vec<[f32; 4]> = Vec::new();
        if let Some(p) = self.player {
            let mut line = format!("You  X {}  Y {}  Z {}", p.pos[0].floor(), p.pos[1].floor(), p.pos[2].floor());
            if let Some((name, x, z)) = self.other_side(p.pos[0], p.pos[2]) {
                line = format!("{line}   {name}  X {x}  Z {z}");
            }
            let tw = draw.text_width(&line);
            status.push([2.0, h - 27.0, tw + 8.0, 12.0]);
            draw.fill([2.0, h - 27.0, tw + 8.0, 12.0], PANEL_BG);
            draw.text(&line, [6.0, h - 25.0], 0xFFC8_C8C8, false);
        }
        let [bx, bz] = self.block_at(view, mouse);
        let mut line = String::new();
        if live {
            let (x, z) = (bx.floor() as i32, bz.floor() as i32);
            let store = self.shown();
            line = match store.column(x, z) {
                Some((c, y, _)) if c != UNSEEN && c != crate::store::VOID => {
                    format!("X {x}  Z {z}  Y {y}")
                }
                _ => format!("X {x}  Z {z}"),
            };
            if let Some(biome) = self.legend.biome_id(store.biome(x, z)) {
                line = format!("{line}   {}", biome_words(biome));
            }
            if let Some((name, x, z)) = self.other_side(bx, bz) {
                line = format!("{line}   {name}  X {x}  Z {z}");
            }
            if let Some(wp) = hovered.and_then(|i| self.waypoints.get(i)) {
                line = format!("{line}   {} ({} {} {})", wp.name, wp.pos[0], wp.pos[1], wp.pos[2]);
                tip = Some("Click: edit this waypoint".into());
            }
        } else if let Some(text) = self.tip.take() {
            // The bubble's own words for what the cursor is on.
            tip = Some(text);
        }
        let tw = draw.text_width(&line);
        if !line.is_empty() {
            status.push([2.0, h - 14.0, tw + 8.0, 12.0]);
            draw.fill([2.0, h - 14.0, tw + 8.0, 12.0], PANEL_BG);
            draw.text(&line, [6.0, h - 12.0], 0xFFFF_FFFF, false);
        }
        let hint = tip.unwrap_or_else(|| "Drag: move   Wheel: zoom   Right-click: waypoint   Space: me".into());
        let hint = fit(draw, &hint, (w - tw - 30.0).max(40.0));
        let hw = draw.text_width(&hint);
        if tw + hw + 24.0 < w {
            status.push([w - hw - 10.0, h - 14.0, hw + 8.0, 12.0]);
            draw.fill([w - hw - 10.0, h - 14.0, hw + 8.0, 12.0], PANEL_BG);
            draw.text(&hint, [w - hw - 6.0, h - 12.0], 0xFFB0_B0B0, false);
        }

        // What just happened, and an export on its way.
        let mut said = self.notice.as_ref().filter(|n| n.1 > self.now).map(|n| n.0.clone());
        if let Some(progress) = self.export_progress() {
            said = Some(progress);
        }
        if let Some(text) = said {
            let text = fit(draw, &text, w - 40.0);
            let tw = draw.text_width(&text);
            draw.fill([(w - tw) / 2.0 - 5.0, 34.0, tw + 10.0, 13.0], 0xE010_1014);
            centered(draw, &text, w / 2.0, 37.0, 0xFFFF_E080);
        }
        let covered = hits
            .iter()
            .any(|(hit, r)| !matches!(hit, Hit::Marker(_) | Hit::Toggle(_) | Hit::Structure(_)) && inside(*r, mouse));
        if let Some(text) = label.filter(|_| live && !covered) {
            gfx::hover_label(draw, mouse, &text, self.gui);
        }
        // What the next draw keeps its markers out from under.
        self.covers.clear();
        self.covers
            .extend(hits.iter().filter(|(hit, _)| !matches!(hit, Hit::Marker(_) | Hit::Structure(_))).map(|(_, r)| *r));
        self.covers.extend(status);
        self.hits = hits;
        let _ = host;
    }

    /// What the image doesn't hold of the view (a pan has run past it and it is not painted
    /// while the view moves) as a coarse mosaic straight from the store: a square of the
    /// colour at its middle every few pixels.
    fn draw_mosaic(&mut self, draw: &mut dyn DrawContext, view: View) {
        let [w, h] = self.gui;
        self.stats.cells = 0;
        let look = self.look();
        // The image's rectangle on the screen: cells wholly inside it are its to draw.
        let held = self.image.filter(|i| i.full).map(|i| {
            let at = self.screen_at(view, f64::from(i.frame.x0), f64::from(i.frame.z0));
            let k = i.frame.step as f32 * view.zoom;
            [at[0], at[1], at[0] + i.frame.w as f32 * k, at[1] + i.frame.h as f32 * k]
        });
        if held.is_some_and(|r| r[0] <= 0.0 && r[1] <= 0.0 && r[2] >= w && r[3] >= h) {
            return;
        }
        // Cells of whole blocks where a block is big enough, at most a few thousand of them.
        let mut cell = 6.0f32.max(view.zoom);
        while (w / cell) * (h / cell) > 3500.0 {
            cell *= 1.5;
        }
        let store = self.shown();
        let tints: &[crate::render::Tints] = if look.tints { &self.tints } else { &[] };
        let mut cells = 0;
        // Anchored to the world, so the mosaic moves with the map instead of crawling.
        let blocks = f64::from(cell / view.zoom);
        let origin = self.block_at(view, [0.0, 0.0]);
        let (first_x, first_z) = ((origin[0] / blocks).floor(), (origin[1] / blocks).floor());
        let (across, down) = ((w / cell).ceil() as i32 + 1, (h / cell).ceil() as i32 + 1);
        for j in 0..=down {
            for i in 0..=across {
                let (bx, bz) = ((first_x + f64::from(i)) * blocks, (first_z + f64::from(j)) * blocks);
                let at = self.screen_at(view, bx, bz);
                if held.is_some_and(|r| at[0] >= r[0] && at[1] >= r[1] && at[0] + cell <= r[2] && at[1] + cell <= r[3]) {
                    continue;
                }
                let middle = ((bx + blocks / 2.0).floor() as i32, (bz + blocks / 2.0).floor() as i32);
                if let Some(argb) = crate::render::flat(store, &self.palette, tints, middle.0, middle.1) {
                    draw.fill([at[0], at[1], cell + 0.05, cell + 0.05], argb);
                    cells += 1;
                }
            }
        }
        self.stats.cells = cells;
    }

    /// The bar at the top left: which layer, what is shown on the map, the export.
    fn draw_bar(
        &mut self,
        draw: &mut dyn DrawContext,
        mouse: [f32; 2],
        hits: &mut Vec<(Hit, [f32; 4])>,
        tip: &mut Option<String>,
        label: &mut Option<String>,
    ) -> f32 {
        let cfg = self.config;
        let edge = self.gui[0] - PANEL_W - 12.0;
        let (mut x, mut y) = (4.0f32, 4.0f32);
        let place = |width: f32, x: &mut f32, y: &mut f32| {
            if *x + width > edge && *x > 4.0 {
                *x = 4.0;
                *y += 14.0;
            }
            let at = [*x, *y];
            *x += width + 1.0;
            at
        };
        // The layer.
        let surface = if self.dim_info.ceiling { "Roof" } else { "Surface" };
        let now = if self.shown_cave() { "the cave layer" } else { "the surface" };
        let modes = [
            (Mode::Auto, "Auto", format!("Cave layer under ground, surface above: now {now}")),
            (Mode::Surface, surface, "The top of the world, whatever is built on it included".to_string()),
            (
                Mode::Cave,
                "Cave",
                match self.band {
                    Some(band) => format!("The caves at your height (Y {} to {})", band * 16 - 16, band * 16 + 24),
                    None => "The caves at your height".to_string(),
                },
            ),
        ];
        for (mode, label, words) in modes {
            let at = place(draw.text_width(label) + 8.0, &mut x, &mut y);
            let rect = gfx::button(draw, at, label, cfg.mode == mode, mouse, 0x8FB4FF);
            hits.push((Hit::Mode(mode), rect));
            if inside(rect, mouse) {
                *tip = Some(words);
            }
        }
        x += 5.0;
        // The radar: a face each (a zombie for the hostile mobs, a cow for the others, the
        // player's for players) and an item for the dropped items, dimmed while off.
        let toggles = [
            ("radar_hostile", "zombie", cfg.hostile, "Hostile mobs"),
            ("radar_passive", "cow", cfg.passive, "Passive mobs"),
            ("radar_players", "player", cfg.players, "Players"),
            ("radar_items", "", cfg.items, "Items"),
        ];
        for (key, face, on, words) in toggles {
            let at = place(14.0, &mut x, &mut y);
            let rect = [at[0], at[1], 14.0, 12.0];
            let hot = inside(rect, mouse);
            draw.fill(rect, if on { 0xE034_3A4C } else if hot { 0xD02A_2C36 } else { PANEL_BG });
            let middle = [at[0] + 7.0, at[1] + 5.5];
            let tint = if on { 0xFFFF_FFFF } else { 0xFF50_5050 };
            match crate::faces::index(face) {
                Some(index) => {
                    if self.draw_face(draw, index, middle, 8.0, tint).is_none() {
                        draw.text(&words[..1], [at[0] + 4.0, at[1] + 2.0], tint, false);
                    }
                }
                None => {
                    // An item: the game's own icon of one, a dark veil over it while off.
                    if !self.bar_item.is_some_and(|item| draw.item(item, [at[0] + 3.0, at[1] + 1.5], 8.0)) {
                        draw.text("I", [at[0] + 5.0, at[1] + 2.0], tint, false);
                    }
                    if !on {
                        draw.fill([at[0] + 2.0, at[1] + 1.0, 10.0, 9.0], 0xB010_1014);
                    }
                }
            }
            if on {
                draw.fill([rect[0], rect[1] + 11.0, 14.0, 1.0], 0xFFFF_FFFF);
            }
            hits.push((Hit::Toggle(key), rect));
            if hot {
                *tip = Some(format!("{words}: {}", if on { "shown" } else { "hidden" }));
                *label = Some(words.to_string());
            }
        }
        x += 5.0;
        // Every waypoint's beam in the world at once; each keeps its own choice under it.
        {
            let on = cfg.beams;
            let at = place(14.0, &mut x, &mut y);
            let rect = [at[0], at[1], 14.0, 12.0];
            let hot = inside(rect, mouse);
            draw.fill(rect, if on { 0xE034_3A4C } else if hot { 0xD02A_2C36 } else { PANEL_BG });
            beam_glyph(draw, at[0] + 3.0, at[1] + 1.0, 0x9FD8FF, on, hot);
            if on {
                draw.fill([rect[0], rect[1] + 11.0, 14.0, 1.0], 0xFFFF_FFFF);
            }
            hits.push((Hit::Toggle("beams"), rect));
            if hot {
                *tip = Some(if on {
                    "All beams: shown (each waypoint's own Beam switch still counts). Click: hide them all".into()
                } else {
                    "All beams: hidden. Click: show them again, as each waypoint has it".into()
                });
                *label = Some("Beams".to_string());
            }
        }
        x += 5.0;
        let mut switch = |label: &str, hit: Hit, on: bool, words: String, x: &mut f32, y: &mut f32| {
            let at = place(draw.text_width(label) + 8.0, x, y);
            let rect = gfx::button(draw, at, label, on, mouse, 0x9FE070);
            hits.push((hit, rect));
            if inside(rect, mouse) {
                *tip = Some(words);
            }
        };
        switch(
            "Biomes",
            Hit::Toggle("biome_colors"),
            cfg.tints,
            "Grass, leaves and water in their biome's colours".into(),
            &mut x,
            &mut y,
        );
        // One switch per overlay: where a separate module's marker sets plug in.
        for (i, slot) in self.overlays.iter().enumerate() {
            switch(slot.overlay.label(), Hit::Overlay(i), slot.on, slot.overlay.tip().to_string(), &mut x, &mut y);
        }
        // The explore overlays: grid, slime chunks, structures, and their filter panel.
        x += 5.0;
        for (i, (label, on, words)) in self.explore_bar().into_iter().enumerate() {
            switch(label, Hit::Explore(i), on, words, &mut x, &mut y);
        }
        x += 5.0;
        switch(
            "PNG",
            Hit::Export,
            self.export.is_some(),
            "Save everything explored on this layer as a picture".into(),
            &mut x,
            &mut y,
        );
        y + 14.0
    }

    /// The list on the right: waypoints shared in chat, this dimension's waypoints nearest
    /// first, then the other dimensions', greyed.
    fn draw_list(
        &mut self,
        draw: &mut dyn DrawContext,
        mouse: [f32; 2],
        hits: &mut Vec<(Hit, [f32; 4])>,
        tip: &mut Option<String>,
    ) {
        let (panel, shown) = self.panel();
        draw.fill(panel, PANEL_BG);
        hits.push((Hit::Panel, panel));
        let count = self.waypoints.len();
        let (inbox, order) = self.rows();
        let title = if self.search.text.trim().is_empty() {
            format!("Waypoints ({count})")
        } else {
            format!("Waypoints ({} of {count})", order.len())
        };
        draw.text(&title, [panel[0] + 4.0, panel[1] + 4.0], 0xFFFF_D75E, true);
        let right = panel[0] + panel[2];

        // The search box: grey rimmed, white rimmed while it takes the keys.
        let field = [panel[0] + 4.0, panel[1] + 15.0, panel[2] - 8.0, 12.0];
        let focus = self.search.focus;
        let over_field = inside(field, mouse);
        draw.fill(field, if focus { 0xFFFF_FFFF } else if over_field { 0xFF70_7078 } else { 0xFF40_4048 });
        draw.fill([field[0] + 1.0, field[1] + 1.0, field[2] - 2.0, field[3] - 2.0], 0xFF00_0000);
        search_glyph(draw, field[0] + 3.0, field[1] + 2.0, if focus { 0xFFC0_C0C0 } else { 0xFF70_7070 });
        let text_x = field[0] + 13.0;
        let clear = [field[0] + field[2] - 11.0, field[1], 11.0, 12.0];
        let has_text = !self.search.text.is_empty();
        if has_text {
            let mut shown_text = self.search.text.clone();
            while !shown_text.is_empty() && draw.text_width(&shown_text) > field[2] - 27.0 {
                shown_text.remove(0);
            }
            draw.text(&shown_text, [text_x, field[1] + 2.0], 0xFFE0_E0E0, false);
            if focus && (self.now / 300).is_multiple_of(2) {
                let cx = text_x + draw.text_width(&shown_text);
                draw.fill([cx, field[1] + 1.0, 1.0, 10.0], 0xFFD0_D0D0);
            }
            let over_clear = inside(clear, mouse);
            draw.text("x", [clear[0] + 3.0, field[1] + 1.0], if over_clear { 0xFFFF_5555 } else { 0xFF90_9090 }, false);
            hits.push((Hit::SearchClear, clear));
            if over_clear {
                *tip = Some("Clear the search".into());
            }
        } else if focus {
            if (self.now / 300).is_multiple_of(2) {
                draw.fill([text_x, field[1] + 1.0, 1.0, 10.0], 0xFFD0_D0D0);
            }
        } else {
            draw.text("Search", [text_x, field[1] + 2.0], 0xFF70_7070, false);
        }
        hits.push((Hit::Search, if has_text { [field[0], field[1], field[2] - 11.0, field[3]] } else { field }));
        if over_field && !(has_text && inside(clear, mouse)) {
            *tip = Some("Type to find waypoints by name. Esc or x: clear".into());
        }

        let top = panel[1] + LIST_TOP;
        if count == 0 && self.inbox.is_empty() {
            draw.text("Right-click the map", [panel[0] + 4.0, top + 2.0], 0xFFA0_A0A0, false);
        } else if inbox.is_empty() && order.is_empty() {
            draw.text("No waypoint by that name", [panel[0] + 4.0, top + 2.0], 0xFFA0_A0A0, false);
        }
        let total = inbox.len() + order.len();
        // More rows than fit: a scrollbar at the right edge, the rows a little narrower.
        self.list_bar = (total > shown).then_some(scrollbar::Bar {
            track: [right - 2.0 - scrollbar::WIDTH, top, scrollbar::WIDTH, shown as f32 * ROW_H],
            first: self.list_scroll as usize,
            shown,
            total,
        });
        if self.list_bar.is_none() {
            self.list_held = None;
        }
        if let (Some(bar), Some(grab)) = (self.list_bar, self.list_held) {
            self.list_scroll = bar.first_at(mouse[1], grab) as f32;
        }
        let held = self.list_held.is_some();
        // While the bar is held, no row is under the cursor.
        let mouse = if held { [-1000.0; 2] } else { mouse };
        let gutter = if self.list_bar.is_some() { 4.0 } else { 0.0 };
        let right = right - gutter;
        let first = (self.list_scroll as usize).min(total.saturating_sub(shown));
        if let Some(bar) = &mut self.list_bar {
            bar.first = first;
        }
        for (slot, entry) in (first..total).take(shown).enumerate() {
            let y = top + slot as f32 * ROW_H;
            let row = [panel[0], y, panel[2] - gutter, ROW_H];
            let delete = [right - 12.0, y, 12.0, ROW_H];
            let hot = inside(row, mouse);
            if hot {
                draw.fill(row, 0x40FF_FFFF);
            }
            let over_delete = inside(delete, mouse);
            let x_color = if over_delete { 0xFFFF_5555 } else { 0xFF90_9090 };
            if entry < inbox.len() {
                // Shared in chat, waiting to be taken.
                let entry = inbox[entry];
                let wp = &self.inbox[entry].wp;
                let add = [right - 38.0, y, 24.0, ROW_H];
                draw.text("+", [row[0] + 4.0, y + 2.0], 0xFF55_FFFF, false);
                let name = fit(draw, &wp.name, row[2] - 54.0);
                draw.text(&name, [row[0] + 13.0, y + 2.0], 0xFF9F_F0F0, false);
                let over_add = inside(add, mouse);
                draw.text("Add", [add[0] + 2.0, y + 2.0], if over_add { 0xFFFF_FFFF } else { 0xFF55_FFFF }, false);
                draw.text("x", [delete[0] + 3.0, y + 1.0], x_color, false);
                hits.push((Hit::Inbox(wp.id, true), [row[0], y, row[2] - 12.0, ROW_H]));
                hits.push((Hit::Inbox(wp.id, false), delete));
                if hot {
                    *tip = Some(format!(
                        "Shared in chat: {} at {} {} {} in the {}",
                        wp.name,
                        wp.pos[0],
                        wp.pos[1],
                        wp.pos[2],
                        waypoints::dim_label(&wp.dim)
                    ));
                }
                continue;
            }
            let index = order[entry - inbox.len()];
            let wp = &self.waypoints[index];
            let here = wp.dim == self.dim;
            let dim = !here || wp.hidden;
            gfx::waypoint_icon(draw, wp, [row[0] + 7.0, y + 6.0], 3.0, dim);
            // Its beam switch, a square level with the `x`: a click switches it.
            let beam = [delete[0] - BEAM_BOX - 1.0, y, BEAM_BOX + 1.0, ROW_H];
            let over_beam = inside(beam, mouse);
            beam_button(draw, [delete[0] - BEAM_BOX, y], wp.color, wp.beam, over_beam);
            let side = if here {
                self.player.map_or(String::new(), |p| {
                    let (dx, dz) = (f64::from(wp.pos[0]) + 0.5 - p.pos[0], f64::from(wp.pos[2]) + 0.5 - p.pos[2]);
                    distance_text((dx * dx + dz * dz).sqrt())
                })
            } else {
                waypoints::dim_label(&wp.dim)
            };
            let sw = draw.text_width(&side);
            let name = fit(draw, &wp.name, row[2] - 43.0 - sw);
            draw.text(&name, [row[0] + 13.0, y + 2.0], if dim { 0xFF80_8080 } else { 0xFFFF_FFFF }, false);
            let nw = draw.text_width(&name);
            if wp.hidden {
                draw.fill([row[0] + 13.0, y + 6.0, nw, 1.0], 0xFF80_8080);
            }
            if wp.temporary {
                // Temporary: a dotted line under the name.
                let mut dx = 0.0;
                while dx < nw - 1.0 {
                    draw.fill([row[0] + 13.0 + dx, y + 10.0, 1.0, 1.0], 0xFF9A_9AB0);
                    dx += 2.0;
                }
            }
            draw.text(&side, [beam[0] - sw - 2.0, y + 2.0], if here { 0xFFA0_A0A0 } else { 0xFF70_7890 }, false);
            draw.text("x", [delete[0] + 3.0, y + 1.0], x_color, false);
            hits.push((Hit::Row(wp.id), row));
            hits.push((Hit::RowBeam(wp.id), beam));
            hits.push((Hit::RowDelete(wp.id), delete));
            if over_delete {
                *tip = Some("Delete this waypoint".into());
            } else if over_beam {
                let all_off = if self.config.beams { "" } else { " (all beams are off on the bar)" };
                *tip = Some(if wp.beam {
                    format!("Beam on: click to hide it{all_off}")
                } else {
                    format!("Beam off: click to show it{all_off}")
                });
            } else if hot && wp.temporary {
                *tip = Some("Temporary: it deletes itself when you get there   Click: go there   Right-click: edit".into());
            } else if hot {
                *tip = Some(if here {
                    "Click: go there   Right-click: rename, colour, move, hide, track, link, share".into()
                } else {
                    format!("In the {}: click to edit", waypoints::dim_label(&wp.dim))
                });
            }
        }
        if let Some(bar) = self.list_bar {
            let real = if held { self.cursor } else { mouse };
            bar.draw(draw, real, held, LIST_BAR_PAD);
            hits.push((Hit::ListBar, bar.grip(LIST_BAR_PAD)));
        }
    }

    // ---- input ----

    pub(crate) fn map_press(&mut self, host: &dyn ClientHost, mouse: [f32; 2], button: i32) {
        // A click stops whatever was moving the view by itself.
        self.stop_motion();
        self.goto = None;
        let Some(view) = self.view else { return };
        let hit = self.hit(mouse);
        // A click anywhere else lets go of the search box (what it holds stays).
        self.search.focus = matches!(hit, Some(Hit::Search));
        match hit {
            Some(Hit::Search) => {}
            Some(Hit::SearchClear) => {
                self.search.text.clear();
                self.list_scroll = 0.0;
            }
            Some(Hit::ListBar) => {
                if let Some(bar) = self.list_bar {
                    let (first, grab) = bar.press(mouse[1]);
                    self.list_scroll = first as f32;
                    self.list_held = Some(grab);
                }
            }
            Some(Hit::RowBeam(id)) => {
                if let Some(w) = self.by_id_mut(id) {
                    w.beam = !w.beam;
                    self.save_waypoints(host);
                }
            }
            Some(Hit::Structure(i)) => {
                let Some((_, pos, name)) = self.explore.picks.get(i).cloned() else { return };
                // On the ground there, when it is explored; else at the player's height.
                let y = match self.surface.column(pos[0], pos[2]) {
                    Some((c, y, _)) if c != UNSEEN && c != crate::store::VOID => i32::from(y) + 1,
                    _ => self.player.map_or(64, |p| p.pos[1].floor() as i32),
                };
                let mut edit = Edit::new([pos[0], y, pos[2]], self.next_color, true);
                edit.name = name;
                edit.temporary = true;
                self.zoom_anim = None;
                self.ui = Ui::Edit(edit);
            }
            Some(Hit::Mode(mode)) => {
                self.set_mode(host, mode);
            }
            Some(Hit::Toggle(key)) => self.flip(host, key),
            Some(Hit::Overlay(i)) => {
                if let Some(slot) = self.overlays.get(i) {
                    let key = format!("overlay_{}", slot.overlay.id());
                    let now = !slot.on;
                    host.set_setting(ID, &key, if now { "true" } else { "false" });
                    self.overlays[i].on = now;
                }
            }
            Some(Hit::Export) => self.start_export(),
            Some(Hit::Explore(which)) => {
                let ex = &mut self.explore;
                match which {
                    0 => ex.grid = !ex.grid,
                    1 => ex.slime = !ex.slime,
                    2 => ex.structures = !ex.structures,
                    _ => ex.panel = !ex.panel,
                }
                if which < 3 {
                    ex.flip_set(usize::MAX);
                    if ex.seed_missing() && which > 0 {
                        ex.panel = true;
                    }
                }
            }
            Some(Hit::Set(index)) => self.explore.flip_set(index),
            Some(Hit::Seed) => self.explore.begin_entry(),
            Some(Hit::RowDelete(id)) => {
                if button == 0
                    && let Some(index) = self.find(id)
                {
                    self.waypoints.remove(index);
                    self.save_waypoints(host);
                    self.list_scroll = 0.0;
                }
            }
            Some(Hit::Row(id)) | Some(Hit::Marker(id)) => {
                let Some(wp) = self.waypoints.iter().find(|w| w.id == id) else { return };
                let row = matches!(self.hit(mouse), Some(Hit::Row(_)));
                if row && button == 0 && wp.dim == self.dim {
                    self.begin_slide_to(f64::from(wp.pos[0]) + 0.5, f64::from(wp.pos[2]) + 0.5);
                } else {
                    // The bubble points at the waypoint: bring it on the screen first.
                    if wp.dim == self.dim {
                        let at = self.screen_at(view, f64::from(wp.pos[0]) + 0.5, f64::from(wp.pos[2]) + 0.5);
                        if !inside([20.0, 90.0, self.gui[0] - PANEL_W - 40.0, self.gui[1] - 130.0], at) {
                            self.view = Some(View { cx: f64::from(wp.pos[0]) + 0.5, cz: f64::from(wp.pos[2]) + 0.5, ..view });
                        }
                    }
                    self.zoom_anim = None;
                    self.ui = Ui::Edit(Edit::of(wp));
                }
            }
            Some(Hit::Inbox(id, take)) => {
                if let Some(entry) = self.inbox.iter().position(|e| e.wp.id == id) {
                    let wp = self.inbox.remove(entry).wp;
                    if take {
                        self.note(format!("Added \"{}\" ({})", wp.name, waypoints::dim_label(&wp.dim)));
                        self.add_waypoint(host, wp);
                    }
                }
            }
            Some(Hit::Panel) => {}
            None if button == 0 => {
                self.zoom_anim = None;
                self.begin_drag(mouse);
            }
            None if button == 1 => {
                let [x, z] = self.block_at(view, mouse);
                let (x, z) = (x.floor() as i32, z.floor() as i32);
                let (y, guess) = match self.shown().column(x, z) {
                    Some((c, y, _)) if c != UNSEEN => (i32::from(y) + 1, false),
                    _ => (self.player.map_or(64, |p| p.pos[1].floor() as i32), true),
                };
                let mut edit = Edit::new([x, y, z], self.next_color, true);
                edit.guess = guess;
                self.ui = Ui::Edit(edit);
            }
            None => {}
        }
    }

    /// A drag: a held scrollbar's thumb follows the cursor, wherever it is. True when one is held.
    pub(crate) fn drag_bars(&mut self, mouse: [f32; 2]) -> bool {
        if let Ui::Edit(edit) = &mut self.ui
            && let Some(picker) = edit.picker.as_mut()
            && picker.drag_bar(mouse)
        {
            return true;
        }
        if let (Some(bar), Some(grab)) = (self.list_bar, self.list_held) {
            self.list_scroll = bar.first_at(mouse[1], grab) as f32;
            return true;
        }
        false
    }

    /// The mouse button let go: a held scrollbar too. True when one was held.
    pub(crate) fn release_bars(&mut self) -> bool {
        let picker = match &mut self.ui {
            Ui::Edit(edit) => edit.picker.as_mut().is_some_and(|p| p.release_bar()),
            _ => false,
        };
        self.list_held.take().is_some() || picker
    }

    pub(crate) fn map_scroll(&mut self, mouse: [f32; 2], steps: f32) {
        let Some(view) = self.view else { return };
        let (panel, shown) = self.panel();
        if inside(panel, mouse) {
            let (inbox, list) = self.rows();
            let most = (inbox.len() + list.len()).saturating_sub(shown) as f32;
            self.list_scroll = (self.list_scroll - steps.signum()).clamp(0.0, most);
            return;
        }
        self.stop_motion();
        self.goto = None;
        // A notch is a small ratio, eased in over a few frames; notches in a row add up
        // from where the last one was heading.
        let aim = self.zoom_anim.map_or(view.zoom, |a| a.to);
        let to = (aim * ZOOM_STEP.powf(steps)).clamp(self.zoom_min(), ZOOM_MAX);
        self.zoom_anim =
            Some(ZoomAnim { from: view.zoom, to, start: self.clock, block: self.block_at(view, mouse), mouse });
    }

    pub(crate) fn map_key(&mut self, host: &dyn ClientHost, key: &str) -> Flow {
        let short = key.strip_prefix("key.keyboard.").unwrap_or(key);
        if self.explore_key(short) {
            return Flow::Cancel;
        }
        if self.search.focus {
            // The search box takes every key: none moves the map or closes it.
            match short {
                "escape" => {
                    self.search.text.clear();
                    self.search.focus = false;
                }
                "enter" | "keypad.enter" => self.search.focus = false,
                "backspace" => {
                    self.search.text.pop();
                }
                "space" if crate::TEST => self.search.type_char(' '),
                _ if crate::TEST && short.len() == 1 => {
                    if let Some(c) = short.chars().next() {
                        self.search.type_char(c);
                    }
                }
                _ => {}
            }
            self.list_scroll = 0.0;
            return Flow::Cancel;
        }
        let bound = |id: &str| host.mod_key(ID, id).is_some_and(|(name, _)| name == key);
        match short {
            "escape" => return Flow::Continue,
            "space" => self.begin_slide(),
            _ if crate::TEST && self.probe_key(host, short) => {}
            _ if bound(KEY_MAP) => self.close = true,
            _ => {
                self.new_here(host, key);
            }
        }
        Flow::Cancel
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_list_is_this_dimension_nearest_first_then_the_rest() {
        let mut st = State { dim: OVERWORLD.into(), ..State::default() };
        st.player = Some(crate::Player { pos: [0.0, 64.0, 0.0], yaw: 0.0 });
        st.waypoints = vec![
            Waypoint::new("far", [500, 64, 0], 0, OVERWORLD),
            Waypoint::new("hub", [1, 64, 1], 0, NETHER),
            Waypoint::new("near", [10, 64, 0], 0, OVERWORLD),
            Waypoint::new("city", [0, 64, 0], 0, "minecraft:the_end"),
        ];
        let names: Vec<&str> = st.listing().into_iter().map(|i| st.waypoints[i].name.as_str()).collect();
        assert_eq!(names, ["near", "far", "city", "hub"]);
    }

    #[test]
    fn the_other_dimension_is_an_eighth_or_eight_times() {
        let mut st = State { dim: OVERWORLD.into(), ..State::default() };
        assert_eq!(st.other_side(-17.0, 800.9), Some(("Nether", -3, 100)));
        st.dim = NETHER.into();
        assert_eq!(st.other_side(-3.5, 100.0), Some(("Overworld", -28, 800)));
        st.dim = "minecraft:the_end".into();
        assert_eq!(st.other_side(1.0, 1.0), None);
        assert_eq!(biome_words("minecraft:dark_forest"), "Dark Forest");
    }
}
