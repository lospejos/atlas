//! The world map screen: the map itself, the bar of switches at its top, the waypoint list on
//! its right and the status lines at its bottom, and what clicks, the wheel and keys do there.
use mod_api::{ClientHost, DrawContext, Flow};

use crate::bubble::Edit;
use crate::gfx::{self, PANEL_BG, argb, centered, fit, inside, marker};
use crate::mapview::{View, ZOOM_MAX, ZOOM_STEP, ZoomAnim};
use crate::overlays::{OverlayMarker, OverlayQuery};
use crate::radar::Class;
use crate::store::UNSEEN;
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
    Row(usize),
    RowDelete(usize),
    /// A waypoint shared in chat: take it, or drop it.
    Inbox(usize, bool),
    Marker(usize),
    /// An explore overlay's switch: 0 grid, 1 slime chunks, 2 structures, 3 the filter panel.
    Explore(usize),
    /// A structure set's switch in the filter panel, and the seed field there.
    Set(usize),
    Seed,
}

/// A biome id in words: `minecraft:dark_forest` is "Dark Forest".
fn biome_words(id: &str) -> String {
    crate::radar::pretty(id)
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

    /// The waypoint list's panel, and how many rows fit.
    fn panel(&self) -> ([f32; 4], usize) {
        let rows = self.waypoints.len() + self.inbox.len();
        let fit = (((self.gui[1] - 60.0) / ROW_H).floor().max(1.0)) as usize;
        let shown = rows.clamp(1, fit);
        ([self.gui[0] - PANEL_W - 4.0, 4.0, PANEL_W, 16.0 + shown as f32 * ROW_H + 2.0], shown)
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
        let mut tip: Option<String> = None;
        draw.fill([0.0, 0.0, w, h], UNSEEN_COLOR);
        self.draw_mosaic(draw, view);
        if let Some(image) = self.image.filter(|i| i.full && i.look == self.look()) {
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

        self.draw_explore(draw, view, mouse, &mut tip);

        // The entities near the player.
        let on_map = |x: f64, z: f64| Some(self.screen_at(view, x, z));
        if let Some(name) = self.draw_blips(draw, f64::from(self.partial), Some(mouse), 1.5, on_map) {
            tip = Some(name);
        }

        // The waypoints of this dimension.
        let mut hovered: Option<usize> = None;
        for (index, wp) in self.here() {
            let at = self.screen_at(view, f64::from(wp.pos[0]) + 0.5, f64::from(wp.pos[2]) + 0.5);
            marker(draw, at, wp.color, 2.5);
            if wp.tracked {
                draw.fill([at[0] - 5.0, at[1] - 5.0, 10.0, 1.0], 0xFFFF_FFFF);
                draw.fill([at[0] - 5.0, at[1] + 4.0, 10.0, 1.0], 0xFFFF_FFFF);
            }
            let over = (mouse[0] - at[0]).abs() <= 5.0 && (mouse[1] - at[1]).abs() <= 5.0;
            if over {
                hovered = Some(index);
            }
            hits.push((Hit::Marker(index), [at[0] - 5.0, at[1] - 5.0, 10.0, 10.0]));
            if if self.config.hover_names { over } else { view.zoom >= 0.5 } {
                let tw = draw.text_width(&wp.name);
                draw.fill([at[0] - tw / 2.0 - 2.0, at[1] + 5.0, tw + 4.0, 10.0], 0x8000_0000);
                centered(draw, &wp.name, at[0], at[1] + 6.0, 0xFFFF_FFFF);
            }
        }
        if let Some(player) = self.player {
            gfx::arrow(draw, self.screen_at(view, player.pos[0], player.pos[2]), player.yaw + 180.0, 5.0, 0xFFFF_FFFF);
        }

        let below = self.draw_bar(draw, mouse, &mut hits, &mut tip);
        self.draw_explore_panel(draw, below, mouse, &mut hits, &mut tip);
        self.draw_list(draw, mouse, &mut hits, &mut tip);
        if inside(self.panel().0, mouse) || hits.iter().any(|(hit, r)| !matches!(hit, Hit::Marker(_)) && inside(*r, mouse))
        {
            hovered = None;
        }

        // Where the player is, and what is under the cursor.
        if let Some(p) = self.player {
            let mut line = format!("You  X {}  Y {}  Z {}", p.pos[0].floor(), p.pos[1].floor(), p.pos[2].floor());
            if let Some((name, x, z)) = self.other_side(p.pos[0], p.pos[2]) {
                line = format!("{line}   {name}  X {x}  Z {z}");
            }
            let tw = draw.text_width(&line);
            draw.fill([2.0, h - 27.0, tw + 8.0, 12.0], PANEL_BG);
            draw.text(&line, [6.0, h - 25.0], 0xFFC8_C8C8, false);
        }
        let [bx, bz] = self.block_at(view, mouse);
        let mut line = String::new();
        if live {
            let (x, z) = (bx.floor() as i32, bz.floor() as i32);
            let store = self.shown();
            line = match store.column(x, z) {
                Some((c, y, _)) if c != UNSEEN && c != crate::store::VOID => format!("X {x}  Z {z}  Y {y}"),
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
            draw.fill([2.0, h - 14.0, tw + 8.0, 12.0], PANEL_BG);
            draw.text(&line, [6.0, h - 12.0], 0xFFFF_FFFF, false);
        }
        let hint = tip.unwrap_or_else(|| "Drag: move   Wheel: zoom   Right-click: waypoint   Space: me".into());
        let hint = fit(draw, &hint, (w - tw - 30.0).max(40.0));
        let hw = draw.text_width(&hint);
        if tw + hw + 24.0 < w {
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
        let held = self.image.filter(|i| i.full && i.look == look).map(|i| {
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
        // The radar: a dot in its colour each.
        let dots = [
            ("radar_hostile", Class::Hostile, cfg.hostile, "Hostile mobs on the maps"),
            ("radar_passive", Class::Passive, cfg.passive, "Animals and other mobs on the maps"),
            ("radar_players", Class::Player, cfg.players, "Other players on the maps"),
            ("radar_items", Class::Item, cfg.items, "Dropped items on the maps"),
        ];
        for (key, class, on, words) in dots {
            let at = place(13.0, &mut x, &mut y);
            let rect = [at[0], at[1], 13.0, 12.0];
            let hot = inside(rect, mouse);
            draw.fill(rect, if on { 0xE034_3A4C } else if hot { 0xD02A_2C36 } else { PANEL_BG });
            let rgb = if on { class.rgb() } else { 0x606060 };
            gfx::dot(draw, [at[0] + 6.5, at[1] + 6.0], rgb, if class == Class::Item { 1.5 } else { 2.5 });
            if on {
                draw.fill([rect[0], rect[1] + 11.0, 13.0, 1.0], argb(class.rgb()));
            }
            hits.push((Hit::Toggle(key), rect));
            if hot {
                *tip = Some(format!("{words}: {}", if on { "on" } else { "off" }));
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
        draw.text(&format!("Waypoints ({count})"), [panel[0] + 4.0, panel[1] + 4.0], 0xFFFF_D75E, true);
        if count == 0 && self.inbox.is_empty() {
            draw.text("Right-click the map", [panel[0] + 4.0, panel[1] + 18.0], 0xFFA0_A0A0, false);
        }
        let order = self.listing();
        let total = self.inbox.len() + order.len();
        let first = (self.list_scroll as usize).min(total.saturating_sub(shown));
        let right = panel[0] + panel[2];
        for (slot, entry) in (first..total).take(shown).enumerate() {
            let y = panel[1] + 16.0 + slot as f32 * ROW_H;
            let row = [panel[0], y, panel[2], ROW_H];
            let delete = [right - 12.0, y, 12.0, ROW_H];
            let hot = inside(row, mouse);
            if hot {
                draw.fill(row, 0x40FF_FFFF);
            }
            let over_delete = inside(delete, mouse);
            let x_color = if over_delete { 0xFFFF_5555 } else { 0xFF90_9090 };
            if entry < self.inbox.len() {
                // Shared in chat, waiting to be taken.
                let wp = &self.inbox[entry];
                let add = [right - 38.0, y, 24.0, ROW_H];
                draw.text("+", [row[0] + 4.0, y + 2.0], 0xFF55_FFFF, false);
                let name = fit(draw, &wp.name, row[2] - 54.0);
                draw.text(&name, [row[0] + 13.0, y + 2.0], 0xFF9F_F0F0, false);
                let over_add = inside(add, mouse);
                draw.text("Add", [add[0] + 2.0, y + 2.0], if over_add { 0xFFFF_FFFF } else { 0xFF55_FFFF }, false);
                draw.text("x", [delete[0] + 3.0, y + 1.0], x_color, false);
                hits.push((Hit::Inbox(entry, true), [row[0], y, row[2] - 12.0, ROW_H]));
                hits.push((Hit::Inbox(entry, false), delete));
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
            let index = order[entry - self.inbox.len()];
            let wp = &self.waypoints[index];
            let here = wp.dim == self.dim;
            let dim = !here || wp.hidden;
            draw.fill([row[0] + 4.0, y + 3.0, 6.0, 6.0], if dim { 0x7000_0000 | wp.color } else { argb(wp.color) });
            if wp.tracked {
                draw.fill([row[0] + 2.0, y + 2.0, 1.0, 8.0], 0xFFFF_FFFF);
            }
            let side = if here {
                self.player.map_or(String::new(), |p| {
                    let (dx, dz) = (f64::from(wp.pos[0]) + 0.5 - p.pos[0], f64::from(wp.pos[2]) + 0.5 - p.pos[2]);
                    distance_text((dx * dx + dz * dz).sqrt())
                })
            } else {
                waypoints::dim_label(&wp.dim)
            };
            let sw = draw.text_width(&side);
            let name = fit(draw, &wp.name, row[2] - 30.0 - sw);
            draw.text(&name, [row[0] + 13.0, y + 2.0], if dim { 0xFF80_8080 } else { 0xFFFF_FFFF }, false);
            if wp.hidden {
                let nw = draw.text_width(&name);
                draw.fill([row[0] + 13.0, y + 6.0, nw, 1.0], 0xFF80_8080);
            }
            draw.text(&side, [delete[0] - sw - 2.0, y + 2.0], if here { 0xFFA0_A0A0 } else { 0xFF70_7890 }, false);
            draw.text("x", [delete[0] + 3.0, y + 1.0], x_color, false);
            hits.push((Hit::Row(index), row));
            hits.push((Hit::RowDelete(index), delete));
            if over_delete {
                *tip = Some("Delete this waypoint".into());
            } else if hot {
                *tip = Some(if here {
                    "Click: go there   Right-click: rename, colour, move, hide, track, link, share".into()
                } else {
                    format!("In the {}: click to edit", waypoints::dim_label(&wp.dim))
                });
            }
        }
    }

    // ---- input ----

    pub(crate) fn map_press(&mut self, host: &dyn ClientHost, mouse: [f32; 2], button: i32) {
        // A click stops whatever was moving the view by itself.
        self.stop_motion();
        self.goto = None;
        let Some(view) = self.view else { return };
        match self.hit(mouse) {
            Some(Hit::Mode(mode)) => {
                self.set_mode(host, mode);
                self.image = None;
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
            Some(Hit::RowDelete(index)) => {
                if button == 0 && index < self.waypoints.len() {
                    self.waypoints.remove(index);
                    self.save_waypoints(host);
                    self.list_scroll = 0.0;
                }
            }
            Some(Hit::Row(index)) | Some(Hit::Marker(index)) => {
                let Some(wp) = self.waypoints.get(index) else { return };
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
                    self.ui = Ui::Edit(Edit::of(index, wp));
                }
            }
            Some(Hit::Inbox(entry, take)) => {
                if entry < self.inbox.len() {
                    let wp = self.inbox.remove(entry);
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
                let y = match self.shown().column(x, z) {
                    Some((c, y, _)) if c != UNSEEN => i32::from(y) + 1,
                    _ => self.player.map_or(64, |p| p.pos[1].floor() as i32),
                };
                self.ui = Ui::Edit(Edit::new([x, y, z], self.next_color, true));
            }
            None => {}
        }
    }

    pub(crate) fn map_scroll(&mut self, mouse: [f32; 2], steps: f32) {
        let Some(view) = self.view else { return };
        let (panel, shown) = self.panel();
        if inside(panel, mouse) {
            let most = (self.waypoints.len() + self.inbox.len()).saturating_sub(shown) as f32;
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
