//! The waypoint bubble: a small panel above the spot it is about, pointing down at it. It asks
//! for a name, the exact X Y Z, a colour, and whether the waypoint has a beam and is temporary;
//! for an existing one it also carries its actions (move here, hide, track, link, share, teleport, delete).
//! The square at the end of the colours' row shows the waypoint's icon; a click there opens the
//! icon picker beside the bubble.
use mod_api::{ClientHost, DrawContext, Flow, Heightmap};

use crate::gfx::{self, POINTER, argb, inside};
use crate::icons::{Picker, Reply};
use crate::waypoints::{self, COLORS, NAME_MAX, Waypoint};
use crate::{KEY_WAYPOINT, KEY_WAYPOINT_TEMP, State, TEST, Ui, block_of};

const W: f32 = 176.0;
const NEW_H: f32 = 72.0;
const EDIT_H: f32 = 114.0;
/// A new waypoint's bubble on the map: one row of actions (Teleport).
const NEW_MAP_H: f32 = 88.0;
const SWATCH: f32 = 11.0;
/// The top of each row, from the panel's top: name, X Y Z, colours, switches, actions.
const ROW_XYZ: f32 = 22.0;
const ROW_COLORS: f32 = 39.0;
const ROW_SWITCHES: f32 = 55.0;
const ROW_ACTIONS: f32 = 71.0;
/// The square at the end of the colours' row: the icon, a click away from the picker.
const ICON_SLOT: f32 = 13.0;
/// The icon picker's size, GUI pixels, and its gap from the bubble.
const PICKER_W: f32 = 166.0;
const PICKER_H: f32 = 130.0;
const PICKER_GAP: f32 = 3.0;

/// What a button of the bubble does to its waypoint.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    Move,
    Hide,
    Track,
    Link,
    Share,
    Teleport,
    Delete,
}

pub const ACTS: [Act; 7] = [Act::Move, Act::Hide, Act::Track, Act::Link, Act::Share, Act::Teleport, Act::Delete];

/// The two switches under the colours.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Switch {
    Beam,
    Temporary,
}

const SWITCHES: [Switch; 2] = [Switch::Beam, Switch::Temporary];

/// The bubble's contents.
pub struct Edit {
    pub name: String,
    pub color: usize,
    /// Where the waypoint is (an existing one's place until a typed one is saved): the bubble
    /// points at it.
    pub pos: [i32; 3],
    /// The X, Y and Z fields as typed.
    pub xyz: [String; 3],
    /// The field that takes typing: 0 the name, 1 to 3 X, Y, Z.
    pub focus: usize,
    /// The focused coordinate field was just entered: the first key typed replaces it.
    fresh: bool,
    /// Enter was pressed with a field that is no number: those show red.
    tried: bool,
    pub beam: bool,
    pub temporary: bool,
    /// The icon id (`Waypoint::icon`).
    pub icon: String,
    /// The icon picker, while it is open.
    pub picker: Option<Box<Picker>>,
    /// Opened from the full map: closing goes back to it.
    pub from_map: bool,
    /// The waypoint being edited, by its id (`Waypoint::id`); none for a new one. Not by its
    /// place in the list: a death or a reached temporary waypoint can move it while open.
    pub target: Option<u64>,
    /// Whether the name or a coordinate was typed in since the bubble opened.
    pub typed: bool,
    /// Whether the name was: an untouched one is not written back (a death renamed by a newer
    /// one keeps its new name).
    pub named: bool,
    /// Opened in the world on the block the crosshair was on: where the bubble points.
    pub anchor: Option<[f64; 3]>,
    /// The map had not seen this column: the Y is the player's, a guess.
    pub guess: bool,
}

/// A teleport to a column the map has not seen: the player was sent to the top of the world and
/// lands on the ground once the column's chunk has loaded, or stays up there at `until`.
pub struct PendingTp {
    column: [i32; 2],
    until: u64,
}

impl Edit {
    pub fn new(pos: [i32; 3], color: usize, from_map: bool) -> Self {
        Self {
            name: String::new(),
            color,
            pos,
            xyz: pos.map(|v| v.to_string()),
            focus: 0,
            fresh: false,
            tried: false,
            beam: true,
            temporary: false,
            icon: String::new(),
            picker: None,
            from_map,
            target: None,
            typed: false,
            named: false,
            anchor: None,
            guess: false,
        }
    }

    /// The bubble for waypoint `w`.
    pub fn of(w: &Waypoint) -> Self {
        let color = COLORS.iter().position(|c| *c == w.color).unwrap_or(COLORS.len());
        Self {
            name: w.name.clone(),
            beam: w.beam,
            temporary: w.temporary,
            icon: w.icon.clone(),
            target: Some(w.id),
            ..Self::new(w.pos, color, true)
        }
    }

    /// Moves the bubble's waypoint to `pos`, fields and all.
    pub fn place(&mut self, pos: [i32; 3]) {
        self.pos = pos;
        self.xyz = pos.map(|v| v.to_string());
    }

    /// The typed X Y Z, when all three are numbers.
    pub fn coords(&self) -> Option<[i32; 3]> {
        Some([
            waypoints::parse_field(&self.xyz[0], 0)?,
            waypoints::parse_field(&self.xyz[1], 1)?,
            waypoints::parse_field(&self.xyz[2], 2)?,
        ])
    }

    pub fn type_char(&mut self, c: char) {
        if self.focus == 0 {
            if waypoints::allowed_char(c) && self.name.chars().count() < NAME_MAX {
                self.name.push(c);
                self.typed = true;
                self.named = true;
            }
            return;
        }
        let field = &mut self.xyz[self.focus - 1];
        if self.fresh && (c.is_ascii_digit() || c == '-') {
            field.clear();
        }
        if waypoints::field_takes(field, c) {
            field.push(c);
            self.fresh = false;
            self.typed = true;
        }
    }

    fn backspace(&mut self) {
        if self.focus == 0 {
            self.name.pop();
            self.named = true;
        } else if self.fresh {
            self.xyz[self.focus - 1].clear();
        } else {
            self.xyz[self.focus - 1].pop();
        }
        self.fresh = false;
        self.typed = true;
    }

    /// Focuses field `focus` (0 the name, 1 to 3 X Y Z).
    fn focus_on(&mut self, focus: usize) {
        self.focus = focus % 4;
        self.fresh = self.focus > 0;
    }
}

/// Where the bubble is: its panel, the tip of its pointer, and whether it hangs below the
/// spot (pointer up) because there was no room above.
pub struct Bubble {
    pub panel: [f32; 4],
    pub tip: [f32; 2],
    pub below: bool,
}

fn field(panel: [f32; 4]) -> [f32; 4] {
    [panel[0] + 5.0, panel[1] + 5.0, panel[2] - 10.0, 14.0]
}

/// The X, Y or Z field (`axis` 0 to 2): a letter, then the box.
fn coord_field(panel: [f32; 4], axis: usize) -> [f32; 4] {
    let group = ((panel[2] - 10.0 - 10.0) / 3.0).floor();
    [panel[0] + 5.0 + axis as f32 * (group + 5.0) + 8.0, panel[1] + ROW_XYZ, group - 8.0, 12.0]
}

fn swatch(panel: [f32; 4], index: usize) -> [f32; 4] {
    let room = panel[2] - 10.0 - ICON_SLOT - 6.0;
    let gap = (room - SWATCH * COLORS.len() as f32) / (COLORS.len() - 1) as f32;
    [(panel[0] + 5.0 + index as f32 * (SWATCH + gap)).round(), panel[1] + ROW_COLORS + 1.0, SWATCH, SWATCH]
}

/// The square at the right end of the colours' row: the icon button.
pub fn icon_slot(panel: [f32; 4]) -> [f32; 4] {
    [panel[0] + panel[2] - 5.0 - ICON_SLOT, panel[1] + ROW_COLORS, ICON_SLOT, ICON_SLOT]
}

/// The Beam and Temporary switches, side by side.
fn switch(panel: [f32; 4], index: usize) -> [f32; 4] {
    let w = ((panel[2] - 10.0 - 2.0) / 2.0).floor();
    [panel[0] + 5.0 + index as f32 * (w + 2.0), panel[1] + ROW_SWITCHES, w, 12.0]
}

/// The rectangle of action `index`: three a row, two rows.
fn action(panel: [f32; 4], index: usize) -> [f32; 4] {
    let w = (panel[2] - 10.0 - 4.0) / 3.0;
    let (col, row) = ((index % 3) as f32, (index / 3) as f32);
    [(panel[0] + 5.0 + col * (w + 2.0)).round(), panel[1] + ROW_ACTIONS + row * 13.0, w.floor(), 12.0]
}

/// A button of the bubble: lit while on, lighter under the cursor, its label in the middle.
fn bubble_button(draw: &mut dyn DrawContext, r: [f32; 4], label: &str, lit: bool, hot: bool, color: u32) {
    draw.fill(r, if hot { 0xFF3A_2A7A } else if lit { 0xFF2A_2050 } else { 0xFF1C_1430 });
    draw.fill([r[0], r[1] + r[3] - 1.0, r[2], 1.0], 0x5000_0000);
    if lit {
        // A thin light edge on top marks the switch that is on.
        draw.fill([r[0], r[1], r[2], 1.0], 0xFF6A_58C8);
    }
    let tw = draw.text_width(label);
    draw.text(label, [(r[0] + (r[2] - tw) / 2.0).round(), r[1] + 2.0], color, false);
}

impl State {
    /// The waypoint with id `id`, wherever it is in the list now.
    pub(crate) fn find(&self, id: u64) -> Option<usize> {
        self.waypoints.iter().position(|w| w.id == id)
    }

    pub(crate) fn by_id(&self, id: u64) -> Option<&Waypoint> {
        self.waypoints.iter().find(|w| w.id == id)
    }

    pub(crate) fn by_id_mut(&mut self, id: u64) -> Option<&mut Waypoint> {
        self.waypoints.iter_mut().find(|w| w.id == id)
    }

    /// Where the bubble goes: above the spot it is about (the block on the map, the crosshair
    /// in the world), pointing down at it; below, pointing up, when there is no room above.
    pub(crate) fn bubble(&self, edit: &Edit) -> Bubble {
        let [w, h] = self.gui;
        let (bw, bh) = match (edit.target.is_some(), edit.from_map) {
            (true, _) => (W, EDIT_H),
            (false, true) => (W, NEW_MAP_H),
            (false, false) => (W, NEW_H),
        };
        let spot = match (edit.from_map, self.view) {
            (true, Some(view)) => self.screen_at(view, f64::from(edit.pos[0]) + 0.5, f64::from(edit.pos[2]) + 0.5),
            // In the world: the aimed block as the view shows it, else the crosshair.
            _ => self.edit_spot.unwrap_or([w / 2.0, h / 2.0]),
        };
        let spot = [spot[0].clamp(12.0, w - 12.0).round(), spot[1].clamp(0.0, h).round()];
        // Clear of the marker or the crosshair.
        let gap = 5.0;
        let below = spot[1] - gap - POINTER - bh < 4.0;
        let x = (spot[0] - bw / 2.0).clamp(4.0, (w - bw - 4.0).max(4.0)).round();
        let (y, tip) = if below {
            (spot[1] + gap + POINTER, spot[1] + gap)
        } else {
            (spot[1] - gap - POINTER - bh, spot[1] - gap)
        };
        Bubble { panel: [x, y, bw, bh], tip: [spot[0], tip], below }
    }

    /// Where the icon picker goes: beside the bubble (right, else left), its top level with
    /// the bubble's; under or over it when neither side has room. Always on the screen.
    pub(crate) fn picker_rect(&self, edit: &Edit) -> [f32; 4] {
        let [w, h] = self.gui;
        let Bubble { panel: p, below, .. } = self.bubble(edit);
        let right = p[0] + p[2] + PICKER_GAP;
        let left = p[0] - PICKER_GAP - PICKER_W;
        let (x, y) = if right + PICKER_W <= w - 4.0 {
            (right, p[1])
        } else if left >= 4.0 {
            (left, p[1])
        } else {
            // Away from the spot the bubble points at.
            let x = p[0] + p[2] - PICKER_W;
            if below { (x, p[1] + p[3] + PICKER_GAP) } else { (x, p[1] - PICKER_GAP - PICKER_H) }
        };
        let x = x.clamp(4.0, (w - PICKER_W - 4.0).max(4.0)).round();
        let y = y.clamp(4.0, (h - PICKER_H - 4.0).max(4.0)).round();
        [x, y, PICKER_W, PICKER_H]
    }

    /// The bubble's waypoint as it would be saved now, to draw its icon.
    fn edit_look(&self, edit: &Edit) -> Waypoint {
        let mut wp = edit
            .target
            .and_then(|id| self.by_id(id).cloned())
            .unwrap_or_else(|| Waypoint::new(String::new(), edit.pos, 0, self.dim.clone()));
        wp.color = COLORS.get(edit.color).copied().unwrap_or(wp.color);
        wp.icon = edit.icon.clone();
        // The button shows the icon itself, without the tracked ring or the temporary hollow.
        (wp.tracked, wp.temporary) = (false, false);
        wp
    }

    /// Gives the bubble's waypoint icon `id` (an existing one at once, saved, as a colour) and
    /// closes the picker.
    fn choose_icon(&mut self, host: &dyn ClientHost, id: String) {
        let Ui::Edit(edit) = &mut self.ui else { return };
        edit.picker = None;
        edit.icon = id.clone();
        if let Some(target) = edit.target
            && let Some(w) = self.by_id_mut(target)
        {
            w.icon = id;
            self.save_waypoints(host);
        }
    }

    /// An action's label and the status line's words for it, and whether it can be used.
    fn action_words(&self, act: Act, w: &Waypoint) -> (&'static str, String, bool) {
        match act {
            Act::Move => ("Move here", "Move this waypoint to where you stand".into(), true),
            Act::Hide if w.hidden => ("Show", "Show it on the maps and in the world again".into(), true),
            Act::Hide => ("Hide", "Keep it in the list, off the maps and the world".into(), true),
            Act::Track if w.tracked => ("Untrack", "Stop the direction arrow".into(), true),
            Act::Track => ("Track", "Point the direction arrow at the top of the screen to it".into(), true),
            Act::Link => match waypoints::linked(w.pos, &w.dim) {
                Some((p, dim)) => (
                    "Link",
                    format!(
                        "Create linked waypoint in the {} at {} {} {} (portal planning)",
                        waypoints::dim_label(dim),
                        p[0],
                        p[1],
                        p[2]
                    ),
                    true,
                ),
                None => ("Link", "Only between the Overworld and the Nether".into(), false),
            },
            Act::Share => ("Share", format!("Send to chat: {}", waypoints::share_line(w)), true),
            Act::Teleport if !self.config.teleport => {
                ("Teleport", "Off: turn on Teleport in Options > Mods > World Atlas (needs cheats or op)".into(), false)
            }
            Act::Teleport => (
                "Teleport",
                format!("Go there with /tp, in the {} (needs cheats or op)", waypoints::dim_label(&w.dim)),
                true,
            ),
            Act::Delete => ("Delete", "Delete this waypoint".into(), true),
        }
    }

    /// A switch's label and the status line's words for it.
    fn switch_words(&self, switch: Switch, edit: &Edit) -> (&'static str, String) {
        let all_off = if self.config.beams { "" } else { " (all beams are off on the map's bar)" };
        match (switch, edit.beam, edit.temporary) {
            (Switch::Beam, true, _) => ("Beam: On", format!("Beam on: a coloured beam stands here in the world. Click to hide it{all_off}")),
            (Switch::Beam, false, _) => ("Beam: Off", format!("Beam off: no beam in the world for this one. Click to show it{all_off}")),
            (Switch::Temporary, _, true) => {
                ("Temporary: On", "Temporary: it deletes itself when you get within 3 blocks of it".into())
            }
            (Switch::Temporary, _, false) => {
                ("Temporary: Off", "Make it temporary: it deletes itself when you get there".into())
            }
        }
    }

    pub(crate) fn draw_edit(&mut self, host: &dyn ClientHost, draw: &mut dyn DrawContext, edit: &mut Edit, mouse: [f32; 2]) {
        let Bubble { panel: p, tip, below } = self.bubble(edit);
        let picking = edit.picker.is_some();
        let picker_rect = self.picker_rect(edit);
        let mouse_raw = mouse;
        // Under the open picker, or while its scrollbar is held, the bubble takes no hover.
        let holding = edit.picker.as_ref().is_some_and(|p| p.holding_bar());
        let mouse = if holding || (picking && inside(picker_rect, mouse)) { [-1000.0; 2] } else { mouse };
        if edit.from_map || edit.anchor.is_some() {
            gfx::bubble(draw, p, Some((tip, below)));
        } else {
            // Nothing was aimed at: a plain box that says where the waypoint goes.
            gfx::bubble(draw, p, None);
            let words = "Where you stand";
            let tw = draw.text_width(words);
            let x = (p[0] + (p[2] - tw) / 2.0).round();
            draw.fill([x - 4.0, p[1] + p[3], tw + 8.0, 11.0], gfx::BUBBLE_BG);
            draw.text(words, [x, p[1] + p[3] + 1.0], 0xFFC8_C8D8, false);
        }
        // The typing goes to the picker's search while it is open: no caret here then.
        let blink = !picking && (self.now / 300).is_multiple_of(2);
        let mut tip_line = None;

        // The name, as a vanilla text box: white rimmed in focus, grey rimmed out of it.
        let f = field(p);
        draw.fill(f, if edit.focus == 0 { 0xFFFF_FFFF } else { 0xFFA0_A0A0 });
        draw.fill([f[0] + 1.0, f[1] + 1.0, f[2] - 2.0, f[3] - 2.0], 0xFF00_0000);
        let (tx, ty) = (f[0] + 4.0, f[1] + 3.0);
        let shown = if edit.name.is_empty() { self.default_name() } else { edit.name.clone() };
        // A long name shows its end, where the typing is.
        let mut shown = shown;
        while draw.text_width(&shown) > f[2] - 10.0 {
            shown.remove(0);
        }
        draw.text(&shown, [tx, ty], if edit.name.is_empty() { 0xFF70_7070 } else { 0xFFE0_E0E0 }, false);
        if edit.focus == 0 && blink {
            let cx = tx + if edit.name.is_empty() { 0.0 } else { draw.text_width(&shown) };
            draw.fill([cx, ty - 1.0, 1.0, 10.0], 0xFFD0_D0D0);
        }
        if inside(f, mouse) {
            tip_line = Some("The name: type it".to_string());
        }

        // X Y Z.
        for axis in 0..3 {
            let r = coord_field(p, axis);
            let text = &edit.xyz[axis];
            let focused = edit.focus == axis + 1;
            let bad = waypoints::parse_field(text, axis).is_none() && (edit.tried || !focused);
            let rim = match (bad, focused) {
                (true, _) => 0xFFFF_5555,
                (false, true) => 0xFFFF_FFFF,
                (false, false) => 0xFFA0_A0A0,
            };
            draw.text(["X", "Y", "Z"][axis], [r[0] - 7.0, r[1] + 2.0], 0xFFC8_C8D8, false);
            draw.fill(r, rim);
            draw.fill([r[0] + 1.0, r[1] + 1.0, r[2] - 2.0, r[3] - 2.0], 0xFF00_0000);
            let mut shown = text.clone();
            while !shown.is_empty() && draw.text_width(&shown) > r[2] - 6.0 {
                shown.remove(0);
            }
            let tw = draw.text_width(&shown);
            if focused && edit.fresh && !picking && !shown.is_empty() {
                // Entered, not typed in yet: all of it is selected, as a vanilla box shows it.
                draw.fill([r[0] + 3.0, r[1] + 2.0, tw + 1.0, 8.0], 0xFF00_00C0);
            }
            draw.text(&shown, [r[0] + 3.0, r[1] + 2.0], if bad { 0xFFFF_8080 } else { 0xFFE0_E0E0 }, false);
            if focused && !edit.fresh && blink {
                draw.fill([r[0] + 3.0 + tw, r[1] + 1.0, 1.0, 10.0], 0xFFD0_D0D0);
            }
            if inside(r, mouse) {
                tip_line = Some(if bad {
                    "Not a number: type a whole number (a minus in front for below zero)".to_string()
                } else {
                    format!("{}: click and type it. Tab: next field, Enter: save", ["X", "Y", "Z"][axis])
                });
            }
        }

        // The colours.
        for (i, rgb) in COLORS.iter().enumerate() {
            let s = swatch(p, i);
            let ring = if i == edit.color { 0xFFFF_FFFF } else { 0xFF00_0000 };
            draw.fill([s[0] - 1.0, s[1] - 1.0, s[2] + 2.0, s[3] + 2.0], ring);
            draw.fill(s, argb(*rgb));
            // A darker lower edge, as a button has.
            draw.fill([s[0], s[1] + s[3] - 2.0, s[2], 2.0], 0x4000_0000);
        }

        // The icon: a button with the waypoint's icon on it, white rimmed while the picker is
        // open, as a chosen colour.
        let slot = icon_slot(p);
        let hot = inside(slot, mouse);
        draw.fill(slot, if picking { 0xFFFF_FFFF } else { 0xFF00_0000 });
        draw.fill([slot[0] + 1.0, slot[1] + 1.0, slot[2] - 2.0, slot[3] - 2.0], if hot { 0xFF3A_2A7A } else { 0xFF1C_1430 });
        let look = self.edit_look(edit);
        gfx::waypoint_icon(draw, &look, [slot[0] + slot[2] / 2.0, slot[1] + slot[3] / 2.0], 4.0, false);
        if hot {
            tip_line = Some(if picking {
                "Icon: click to close the picker".to_string()
            } else {
                "Icon: click to choose a shape, an item or a mob's face".to_string()
            });
        }

        // Beam and Temporary.
        for (i, sw) in SWITCHES.into_iter().enumerate() {
            let r = switch(p, i);
            let on = match sw {
                Switch::Beam => edit.beam,
                Switch::Temporary => edit.temporary,
            };
            let hot = inside(r, mouse);
            let (label, words) = self.switch_words(sw, edit);
            let color = if hot { 0xFFFF_FFFF } else if on { 0xFFE8_E8F8 } else { 0xFF90_90A8 };
            bubble_button(draw, r, label, on, hot, color);
            if hot {
                tip_line = Some(words);
            }
        }

        // An existing waypoint's actions.
        if let Some(wp) = edit.target.and_then(|id| self.by_id(id)) {
            for (i, act) in ACTS.into_iter().enumerate() {
                let r = action(p, i);
                let (label, words, usable) = self.action_words(act, wp);
                let hot = usable && inside(r, mouse);
                let lit = matches!(act, Act::Hide if wp.hidden) || matches!(act, Act::Track if wp.tracked);
                let color = match (usable, act, hot) {
                    (false, ..) => 0xFF60_6060,
                    (_, Act::Delete, true) => 0xFFFF_6060,
                    (_, _, true) => 0xFFFF_FFFF,
                    _ => 0xFFC8_C8D8,
                };
                bubble_button(draw, r, label, lit, hot, color);
                if inside(r, mouse) {
                    tip_line = Some(words);
                }
            }
        }
        // A new waypoint on the map: Teleport to the typed place.
        if edit.target.is_none() && edit.from_map {
            let r = action(p, 0);
            let usable = self.config.teleport;
            let hot = usable && inside(r, mouse);
            let color = match (usable, hot) {
                (false, _) => 0xFF60_6060,
                (_, true) => 0xFFFF_FFFF,
                _ => 0xFFC8_C8D8,
            };
            bubble_button(draw, r, "Teleport", false, hot, color);
            if inside(r, mouse) {
                tip_line = Some(if usable {
                    "Go to the place above with /tp (needs cheats or op)".to_string()
                } else {
                    "Off: turn on Teleport in Options > Mods > World Atlas (needs cheats or op)".to_string()
                });
            }
        }
        if let Some(picker) = edit.picker.as_mut() {
            let rect = picker_rect;
            let rgb = COLORS.get(edit.color).copied().unwrap_or(look.color);
            picker.draw(draw, rect, mouse_raw, host, rgb);
            if inside(rect, mouse_raw) {
                tip_line = Some("Click: this icon   Type: search   Enter: the first found   Esc: close".to_string());
            }
        }
        self.tip = tip_line;
    }

    /// Ends the dialog: back to the map it came from, else the screen closes. With `save`, a
    /// typed place that is not three numbers is not taken: false then, and the bubble stays.
    pub(crate) fn finish_edit(&mut self, host: &dyn ClientHost, save: bool) -> bool {
        if save && let Ui::Edit(edit) = &mut self.ui && edit.coords().is_none() {
            edit.tried = true;
            edit.focus_on((0..3).find(|a| waypoints::parse_field(&edit.xyz[*a], *a).is_none()).map_or(0, |a| a + 1));
            return false;
        }
        let Ui::Edit(edit) = std::mem::take(&mut self.ui) else { return true };
        if save {
            let name = waypoints::clean(edit.name.trim());
            let name = if name.is_empty() { self.default_name() } else { name };
            let pos = edit.coords().unwrap_or(edit.pos);
            match edit.target {
                Some(_) => {
                    self.store_edit(&edit, name, pos);
                    self.save_waypoints(host);
                }
                None => {
                    self.next_color = (edit.color + 1) % COLORS.len();
                    let color = COLORS[edit.color % COLORS.len()];
                    let mut wp = Waypoint::new(name, pos, color, self.dim.clone());
                    wp.beam = edit.beam;
                    wp.temporary = edit.temporary;
                    wp.icon = edit.icon.clone();
                    self.add_waypoint(host, wp);
                }
            }
        }
        if edit.from_map {
            self.ui = Ui::Map;
        } else {
            self.close = true;
        }
        true
    }

    /// Writes the bubble of an existing waypoint into it, found by its id: false when it is
    /// gone meanwhile (a death past the ones kept, a temporary one reached), and then nothing
    /// else takes its place. An untouched name is left as the waypoint has it now.
    pub(crate) fn store_edit(&mut self, edit: &Edit, name: String, pos: [i32; 3]) -> bool {
        let Some(w) = edit.target.and_then(|id| self.by_id_mut(id)) else { return false };
        if edit.named {
            w.name = name;
        }
        w.pos = pos;
        w.beam = edit.beam;
        w.temporary = edit.temporary;
        w.icon = edit.icon.clone();
        if let Some(color) = COLORS.get(edit.color) {
            w.color = *color;
        }
        true
    }

    /// Does `act` to waypoint `id`. True when the bubble should close.
    pub(crate) fn act(&mut self, host: &dyn ClientHost, id: u64, act: Act) -> bool {
        let Some(index) = self.find(id) else { return true };
        let wp = self.waypoints[index].clone();
        let mut close = true;
        match act {
            Act::Move => {
                let Some(p) = self.player else { return false };
                let pos = block_of(p.pos);
                self.waypoints[index].pos = pos;
                self.waypoints[index].dim = self.dim.clone();
                self.note(format!("Moved \"{}\" to {} {} {}", wp.name, pos[0], pos[1], pos[2]));
            }
            Act::Hide => {
                self.waypoints[index].hidden = !wp.hidden;
                close = false;
            }
            Act::Track => {
                for w in &mut self.waypoints {
                    w.tracked = w.id == id && !wp.tracked;
                }
                if !wp.tracked && !self.config.arrow {
                    self.flip(host, "arrow");
                }
                close = false;
            }
            Act::Link => {
                let Some((pos, dim)) = waypoints::linked(wp.pos, &wp.dim) else { return false };
                if !self.waypoints.iter().any(|w| w.dim == dim && w.pos == pos) {
                    self.waypoints.push(Waypoint::new(wp.name.clone(), pos, wp.color, dim));
                }
                self.note(format!(
                    "Linked waypoint \"{}\" in the {} at {} {} {}",
                    wp.name,
                    waypoints::dim_label(dim),
                    pos[0],
                    pos[1],
                    pos[2]
                ));
            }
            Act::Share => {
                self.outbox.push(waypoints::share_line(&wp));
                self.note(format!("Shared \"{}\" to chat", wp.name));
            }
            Act::Teleport => {
                if !self.config.teleport {
                    return false;
                }
                self.outbox.push(waypoints::tp_command(wp.pos, &wp.dim, &self.dim));
                self.note(format!("Teleporting to \"{}\"", wp.name));
            }
            Act::Delete => {
                self.waypoints.remove(index);
                self.list_scroll = 0.0;
            }
        }
        self.save_waypoints(host);
        close
    }

    /// Teleports to the place typed in a new waypoint's bubble. Where the map had not seen the
    /// column (the Y is a guess) and the dimension has no roof: up to the top of the world with
    /// slow falling, then onto the ground when the chunk is there (`run_pending_tp`).
    pub(crate) fn teleport_here(&mut self, host: &dyn ClientHost) -> bool {
        let Ui::Edit(edit) = &self.ui else { return false };
        let Some(pos) = edit.coords().filter(|_| self.config.teleport) else { return false };
        let blind = edit.guess && pos[1] == edit.pos[1] && !self.dim_info.ceiling;
        if blind {
            self.outbox.push("/effect give @s minecraft:slow_falling 20 0 true".into());
            self.outbox.push(waypoints::tp_command([pos[0], self.dim_info.roof, pos[2]], &self.dim, &self.dim));
            self.pending_tp = Some(PendingTp { column: [pos[0], pos[2]], until: host.millis() + 15_000 });
        } else {
            self.outbox.push(waypoints::tp_command(pos, &self.dim, &self.dim));
        }
        self.note(format!("Teleporting to {} {} {}", pos[0], pos[1], pos[2]));
        true
    }

    /// Finishes a blind teleport: onto the ground of its column once the chunk is loaded.
    pub(crate) fn run_pending_tp(&mut self, host: &dyn ClientHost) {
        let Some(wait) = &self.pending_tp else { return };
        let [x, z] = wait.column;
        if host.millis() > wait.until {
            self.pending_tp = None;
            self.said.push("[Atlas] That place did not load in time: you stay high up".into());
            return;
        }
        let top = host
            .client_surface_heights(&[(x, z)], Heightmap::WorldSurface)
            .ok()
            .and_then(|h| h.first().copied().flatten());
        if let Some(top) = top {
            self.pending_tp = None;
            self.outbox.push(waypoints::tp_command([x, top, z], &self.dim, &self.dim));
        }
    }

    pub(crate) fn edit_press(&mut self, host: &dyn ClientHost, mouse: [f32; 2], _button: i32) {
        let Ui::Edit(edit) = &self.ui else { return };
        let panel = self.bubble(edit).panel;
        let target = edit.target;
        let slot = icon_slot(panel);
        // The open picker first: a click on an icon takes it, anywhere else on the picker does
        // nothing; a click off it closes it (and is used for nothing more off the bubble).
        if edit.picker.is_some() {
            let rect = self.picker_rect(edit);
            if inside(rect, mouse) {
                // Its scrollbar: held from here on.
                if let Ui::Edit(edit) = &mut self.ui
                    && let Some(picker) = edit.picker.as_mut()
                    && picker.press_bar(mouse)
                {
                    return;
                }
                let Ui::Edit(edit) = &self.ui else { return };
                let Some(picker) = &edit.picker else { return };
                if let Some(id) = picker.click(mouse) {
                    self.choose_icon(host, id);
                }
                return;
            }
            if let Ui::Edit(edit) = &mut self.ui {
                edit.picker = None;
            }
            if !inside(panel, mouse) || inside(slot, mouse) {
                return;
            }
        } else if inside(slot, mouse) {
            if let Ui::Edit(edit) = &mut self.ui {
                edit.picker = Some(Box::new(Picker::new(&edit.icon)));
            }
            return;
        }
        // A field takes the typing.
        let clicked = (0..4).find(|i| inside(if *i == 0 { field(panel) } else { coord_field(panel, i - 1) }, mouse));
        if let (Some(i), Ui::Edit(edit)) = (clicked, &mut self.ui) {
            edit.focus_on(i);
            return;
        }
        let picked = (0..COLORS.len()).find(|i| {
            let s = swatch(panel, *i);
            inside([s[0] - 2.0, s[1] - 2.0, s[2] + 4.0, s[3] + 4.0], mouse)
        });
        if let (Some(i), Ui::Edit(edit)) = (picked, &mut self.ui) {
            edit.color = i;
            // An existing waypoint takes the colour at once.
            if let Some(w) = target.and_then(|t| self.by_id_mut(t)) {
                w.color = COLORS[i];
                self.save_waypoints(host);
            }
            return;
        }
        if let (Some(sw), Ui::Edit(edit)) =
            (SWITCHES.into_iter().enumerate().find(|(i, _)| inside(switch(panel, *i), mouse)).map(|s| s.1), &mut self.ui)
        {
            let (beam, temporary) = match sw {
                Switch::Beam => (!edit.beam, edit.temporary),
                Switch::Temporary => (edit.beam, !edit.temporary),
            };
            (edit.beam, edit.temporary) = (beam, temporary);
            // An existing waypoint takes it at once, as a colour.
            if let Some(w) = target.and_then(|t| self.by_id_mut(t)) {
                (w.beam, w.temporary) = (beam, temporary);
                self.save_waypoints(host);
            }
            return;
        }
        if target.is_none() && matches!(&self.ui, Ui::Edit(e) if e.from_map) && inside(action(panel, 0), mouse) {
            if self.teleport_here(host) {
                self.ui = Ui::Map;
            }
            return;
        }
        if let Some(id) = target
            && let Some(act) = ACTS.into_iter().enumerate().find(|(i, _)| inside(action(panel, *i), mouse)).map(|a| a.1)
        {
            // A name typed so far is kept before the action.
            let typed = match &self.ui {
                Ui::Edit(edit) if edit.named => Some(waypoints::clean(edit.name.trim())),
                _ => None,
            };
            if let Some(name) = typed.filter(|n| !n.is_empty())
                && let Some(w) = self.by_id_mut(id)
            {
                w.name = name;
            }
            if self.act(host, id, act) {
                self.ui = Ui::Map;
            } else if let (Ui::Edit(edit), Some(w)) = (&mut self.ui, self.waypoints.iter().find(|w| w.id == id))
                && act == Act::Move
            {
                edit.place(w.pos);
            }
            return;
        }
        if !inside(panel, mouse) {
            // A click outside keeps what was typed for an existing waypoint (a place only when
            // it is three numbers), drops a new one.
            let keep = target.is_some() && matches!(&self.ui, Ui::Edit(e) if e.typed);
            if keep && let Ui::Edit(edit) = &mut self.ui && edit.coords().is_none() {
                edit.place(edit.pos);
            }
            self.finish_edit(host, keep);
        }
    }

    pub(crate) fn edit_key(&mut self, host: &dyn ClientHost, key: &str) -> Flow {
        let short = key.strip_prefix("key.keyboard.").unwrap_or(key);
        let back = host.modifiers().shift();
        let Ui::Edit(edit) = &mut self.ui else { return Flow::Continue };
        // The open picker takes the keys it knows; Escape closes it before the bubble.
        if let Some(picker) = edit.picker.as_mut() {
            match (short, picker.key(key)) {
                ("escape", _) => edit.picker = None,
                (_, Reply::Chose(id)) => self.choose_icon(host, id),
                (_, Reply::Handled) => {}
                // An empty search's Enter saves the bubble, as without the picker.
                ("enter" | "keypad.enter", Reply::Ignored) => {
                    self.finish_edit(host, true);
                }
                (_, Reply::Ignored) if TEST && short.len() == 1 => {
                    if let Some(c) = short.chars().next() {
                        picker.type_char(c);
                    }
                }
                // Backspace on an empty search and the rest: nothing, the name stays as it is.
                (_, Reply::Ignored) => {}
            }
            return Flow::Cancel;
        }
        match short {
            "enter" | "keypad.enter" => {
                self.finish_edit(host, true);
            }
            "escape" => {
                if !edit.from_map {
                    // The game closes the screen itself.
                    return Flow::Continue;
                }
                self.finish_edit(host, false);
            }
            "backspace" => edit.backspace(),
            "tab" => {
                let next = if back { edit.focus + 3 } else { edit.focus + 1 };
                edit.focus_on(next);
            }
            "delete" => {
                if let Some(id) = edit.target
                    && edit.focus == 0
                {
                    self.act(host, id, Act::Delete);
                    self.ui = Ui::Map;
                }
            }
            "minus" | "keypad.subtract" if TEST => edit.type_char('-'),
            _ if TEST && short.len() == 1 => {
                if let Some(c) = short.chars().next() {
                    edit.type_char(c);
                }
            }
            _ => {}
        }
        Flow::Cancel
    }

    /// Opens the bubble for a new waypoint where the player stands, from the map: the
    /// waypoint key, or the temporary waypoint key (Shift+J) with Temporary on.
    pub(crate) fn new_here(&mut self, host: &dyn ClientHost, key: &str) -> bool {
        let held = host.modifiers();
        let temp = host
            .mod_key(crate::ID, KEY_WAYPOINT_TEMP)
            .is_some_and(|(name, m)| name == key && m.bits != 0 && held.bits & m.bits == m.bits);
        if !temp && !host.mod_key(crate::ID, KEY_WAYPOINT).is_some_and(|(name, _)| name == key) {
            return false;
        }
        if let Some(p) = self.player {
            let mut edit = Edit::new(block_of(p.pos), self.next_color, true);
            edit.temporary = temp;
            self.ui = Ui::Edit(edit);
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_fields_take_numbers_only() {
        let mut e = Edit::new([10, 64, -5], 0, true);
        assert_eq!(e.coords(), Some([10, 64, -5]));
        e.type_char('B');
        assert_eq!(e.name, "B");
        // Entered with Tab: the first key replaces the field.
        e.focus_on(1);
        for c in "-12x3".chars() {
            e.type_char(c);
        }
        assert_eq!(e.xyz[0], "-123");
        e.focus_on(2);
        e.backspace();
        assert_eq!(e.xyz[1], "", "a fresh field clears");
        assert_eq!(e.coords(), None, "an empty field is never saved");
        e.type_char('-');
        assert_eq!(e.coords(), None);
        e.type_char('7');
        e.type_char('-');
        assert_eq!(e.coords(), Some([-123, -7, -5]));
        e.focus_on(3);
        e.type_char('a');
        assert_eq!(e.xyz[2], "-5", "a letter leaves a fresh field as it was");
        e.focus_on(4);
        assert_eq!(e.focus, 0, "Tab past Z is the name again");
        e.place([1, 2, 3]);
        assert_eq!(e.xyz, ["1", "2", "3"]);
    }

    #[test]
    fn an_open_bubble_keeps_its_waypoint_when_the_list_moves() {
        use crate::waypoints::{Kind, OVERWORLD, add_death};
        let mut st = State { dim: OVERWORLD.into(), ..State::default() };
        let a = Waypoint::new("A", [1, 64, 1], COLORS[0], OVERWORLD);
        let old = Waypoint::auto(Kind::Death, [5, 64, 5], OVERWORLD);
        let b = Waypoint::new("B", [2, 64, 2], COLORS[1], OVERWORLD);
        st.waypoints = vec![a.clone(), old.clone(), b.clone()];
        let mut edit = Edit::of(&st.waypoints[2]);
        for c in "Base".chars() {
            edit.focus = 0;
            edit.type_char(c);
        }
        edit.color = 3;
        // A death while the bubble is open goes in before B: B moves down the list.
        add_death(&mut st.waypoints, Waypoint::auto(Kind::Death, [9, 64, 9], OVERWORLD), 3);
        assert_eq!(st.waypoints[2].name, "Death 2");
        assert!(st.store_edit(&edit, "B2".into(), [7, 70, 7]));
        let b2 = st.waypoints.iter().find(|w| w.id == b.id).unwrap();
        assert_eq!((b2.name.as_str(), b2.pos, b2.color), ("B2", [7, 70, 7], COLORS[3]));
        assert_eq!(st.waypoints[0], a);
        let deaths: Vec<_> = st.waypoints.iter().filter(|w| w.kind == Kind::Death).map(|w| (w.name.as_str(), w.pos)).collect();
        assert_eq!(deaths, [("Death", [9, 64, 9]), ("Death 2", [5, 64, 5])]);
        // An untouched name is not written back: the death renamed by the newer one keeps it.
        let death2 = st.waypoints.iter().find(|w| w.id == old.id).unwrap().clone();
        let mut touch = Edit::of(&death2);
        touch.name = "Death".into();
        assert!(st.store_edit(&touch, "Death".into(), death2.pos));
        assert_eq!(st.waypoints.iter().find(|w| w.id == old.id).unwrap().name, "Death 2");
        // Gone meanwhile: nothing else is changed.
        let before = st.waypoints.clone();
        let gone = Edit::of(&Waypoint::new("X", [0, 0, 0], 0, OVERWORLD));
        assert!(!st.store_edit(&gone, "X".into(), [0, 0, 0]));
        assert_eq!(st.waypoints, before);
    }

    #[test]
    fn a_name_takes_no_section_sign() {
        let mut e = Edit::new([0, 0, 0], 0, true);
        for c in "a\u{a7}cb\u{7f}\n".chars() {
            e.type_char(c);
        }
        assert_eq!(e.name, "acb");
    }

    #[test]
    fn the_rows_fit_the_bubble() {
        let p = [0.0, 0.0, W, EDIT_H];
        let last = coord_field(p, 2);
        assert!(last[0] + last[2] <= W - 5.0, "{last:?}");
        assert!(swatch(p, COLORS.len() - 1)[0] + SWATCH < icon_slot(p)[0] - 2.0);
        assert_eq!(switch(p, 1)[0] + switch(p, 1)[2], W - 5.0);
        assert!(switch(p, 0)[1] + 12.0 <= NEW_H - 5.0);
        assert!(action(p, ACTS.len() - 1)[1] + 12.0 <= EDIT_H - 5.0);
        assert!(action(p, 0)[1] + 12.0 <= NEW_MAP_H - 5.0);
    }
}
