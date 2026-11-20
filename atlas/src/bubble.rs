//! The waypoint bubble: a small panel above the spot it is about, pointing down at it. For a
//! new waypoint it asks for a name and a colour; for an existing one it also carries its
//! actions (move here, hide, track, link, share, delete).
use mod_api::{ClientHost, DrawContext, Flow};

use crate::gfx::{self, POINTER, argb, inside};
use crate::waypoints::{self, COLORS, NAME_MAX, Waypoint};
use crate::{KEY_WAYPOINT, State, TEST, Ui, block_of};

const NEW_W: f32 = 130.0;
const NEW_H: f32 = 39.0;
const EDIT_W: f32 = 164.0;
const EDIT_H: f32 = 69.0;
const SWATCH: f32 = 11.0;

/// What a button of the bubble does to its waypoint.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    Move,
    Hide,
    Track,
    Link,
    Share,
    Delete,
}

pub const ACTS: [Act; 6] = [Act::Move, Act::Hide, Act::Track, Act::Link, Act::Share, Act::Delete];

/// The bubble's contents.
pub struct Edit {
    pub name: String,
    pub color: usize,
    pub pos: [i32; 3],
    /// Opened from the full map: closing goes back to it.
    pub from_map: bool,
    /// The waypoint being edited, by its index; none for a new one.
    pub target: Option<usize>,
    /// Whether the name was typed in since the bubble opened.
    pub typed: bool,
}

impl Edit {
    pub fn new(pos: [i32; 3], color: usize, from_map: bool) -> Self {
        Self { name: String::new(), color, pos, from_map, target: None, typed: false }
    }

    /// The bubble for waypoint `index`.
    pub fn of(index: usize, w: &Waypoint) -> Self {
        let color = COLORS.iter().position(|c| *c == w.color).unwrap_or(COLORS.len());
        Self { name: w.name.clone(), color, pos: w.pos, from_map: true, target: Some(index), typed: false }
    }

    pub fn type_char(&mut self, c: char) {
        if !c.is_control() && self.name.chars().count() < NAME_MAX {
            self.name.push(c);
            self.typed = true;
        }
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

fn swatch(panel: [f32; 4], index: usize) -> [f32; 4] {
    let gap = (panel[2] - 10.0 - SWATCH * COLORS.len() as f32) / (COLORS.len() - 1) as f32;
    [(panel[0] + 5.0 + index as f32 * (SWATCH + gap)).round(), panel[1] + 23.0, SWATCH, SWATCH]
}

/// The rectangle of action `index`: three a row, two rows.
fn action(panel: [f32; 4], index: usize) -> [f32; 4] {
    let w = (panel[2] - 10.0 - 4.0) / 3.0;
    let (col, row) = ((index % 3) as f32, (index / 3) as f32);
    [(panel[0] + 5.0 + col * (w + 2.0)).round(), panel[1] + 39.0 + row * 13.0, w.floor(), 12.0]
}

impl State {
    /// Where the bubble goes: above the spot it is about (the block on the map, the crosshair
    /// in the world), pointing down at it; below, pointing up, when there is no room above.
    pub(crate) fn bubble(&self, edit: &Edit) -> Bubble {
        let [w, h] = self.gui;
        let (bw, bh) = if edit.target.is_some() { (EDIT_W, EDIT_H) } else { (NEW_W, NEW_H) };
        let spot = match (edit.from_map, self.view) {
            (true, Some(view)) => self.screen_at(view, f64::from(edit.pos[0]) + 0.5, f64::from(edit.pos[2]) + 0.5),
            _ => [w / 2.0, h / 2.0],
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
            Act::Delete => ("Delete", "Delete this waypoint".into(), true),
        }
    }

    pub(crate) fn draw_edit(&mut self, draw: &mut dyn DrawContext, edit: &Edit, mouse: [f32; 2]) {
        let Bubble { panel: p, tip, below } = self.bubble(edit);
        gfx::bubble(draw, p, Some((tip, below)));

        // The name, as a vanilla text box in focus.
        let f = field(p);
        draw.fill(f, 0xFFFF_FFFF);
        draw.fill([f[0] + 1.0, f[1] + 1.0, f[2] - 2.0, f[3] - 2.0], 0xFF00_0000);
        let (tx, ty) = (f[0] + 4.0, f[1] + 3.0);
        let coords = format!("{} {} {}", edit.pos[0], edit.pos[1], edit.pos[2]);
        let cw = draw.text_width(&coords);
        let shown = if edit.name.is_empty() { self.default_name() } else { edit.name.clone() };
        // Where the block is, as long as the name leaves room for it.
        let room = f[2] - 8.0 - cw - 6.0;
        let shown = if draw.text_width(&shown) <= room {
            draw.text(&coords, [f[0] + f[2] - 4.0 - cw, ty], 0xFF60_6060, false);
            shown
        } else {
            // A long name shows its end, where the typing is.
            let mut tail = shown;
            while draw.text_width(&tail) > f[2] - 10.0 {
                tail.remove(0);
            }
            tail
        };
        draw.text(&shown, [tx, ty], if edit.name.is_empty() { 0xFF70_7070 } else { 0xFFE0_E0E0 }, false);
        if (self.now / 300).is_multiple_of(2) {
            let cx = tx + if edit.name.is_empty() { 0.0 } else { draw.text_width(&shown) };
            draw.fill([cx, ty - 1.0, 1.0, 10.0], 0xFFD0_D0D0);
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
        // An existing waypoint's actions.
        let Some(wp) = edit.target.and_then(|i| self.waypoints.get(i)) else { return };
        let mut tip_line = None;
        for (i, act) in ACTS.into_iter().enumerate() {
            let r = action(p, i);
            let (label, words, usable) = self.action_words(act, wp);
            let hot = usable && inside(r, mouse);
            let lit = matches!(act, Act::Hide if wp.hidden) || matches!(act, Act::Track if wp.tracked);
            draw.fill(r, if hot { 0xFF3A_2A7A } else if lit { 0xFF2A_2050 } else { 0xFF1C_1430 });
            draw.fill([r[0], r[1] + r[3] - 1.0, r[2], 1.0], 0x5000_0000);
            let color = match (usable, act, hot) {
                (false, ..) => 0xFF60_6060,
                (_, Act::Delete, true) => 0xFFFF_6060,
                (_, _, true) => 0xFFFF_FFFF,
                _ => 0xFFC8_C8D8,
            };
            let tw = draw.text_width(label);
            draw.text(label, [(r[0] + (r[2] - tw) / 2.0).round(), r[1] + 2.0], color, false);
            if inside(r, mouse) {
                tip_line = Some(words);
            }
        }
        self.tip = tip_line;
    }

    /// Ends the dialog: back to the map it came from, else the screen closes.
    pub(crate) fn finish_edit(&mut self, host: &dyn ClientHost, save: bool) {
        let Ui::Edit(edit) = std::mem::take(&mut self.ui) else { return };
        if save {
            let name = edit.name.trim();
            let name = if name.is_empty() { self.default_name() } else { name.to_string() };
            match edit.target {
                Some(index) => {
                    if let Some(w) = self.waypoints.get_mut(index) {
                        w.name = name;
                        if let Some(color) = COLORS.get(edit.color) {
                            w.color = *color;
                        }
                    }
                    self.save_waypoints(host);
                }
                None => {
                    self.next_color = (edit.color + 1) % COLORS.len();
                    let color = COLORS[edit.color % COLORS.len()];
                    self.add_waypoint(host, Waypoint::new(name, edit.pos, color, self.dim.clone()));
                }
            }
        }
        if edit.from_map {
            self.ui = Ui::Map;
        } else {
            self.close = true;
        }
    }

    /// Does `act` to waypoint `index`. True when the bubble should close.
    pub(crate) fn act(&mut self, host: &dyn ClientHost, index: usize, act: Act) -> bool {
        let Some(wp) = self.waypoints.get(index).cloned() else { return true };
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
                for (i, w) in self.waypoints.iter_mut().enumerate() {
                    w.tracked = i == index && !wp.tracked;
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
            Act::Delete => {
                self.waypoints.remove(index);
                self.list_scroll = 0.0;
            }
        }
        self.save_waypoints(host);
        close
    }

    pub(crate) fn edit_press(&mut self, host: &dyn ClientHost, mouse: [f32; 2], _button: i32) {
        let Ui::Edit(edit) = &self.ui else { return };
        let panel = self.bubble(edit).panel;
        let target = edit.target;
        let picked = (0..COLORS.len()).find(|i| {
            let s = swatch(panel, *i);
            inside([s[0] - 2.0, s[1] - 2.0, s[2] + 4.0, s[3] + 4.0], mouse)
        });
        if let (Some(i), Ui::Edit(edit)) = (picked, &mut self.ui) {
            edit.color = i;
            // An existing waypoint takes the colour at once.
            if let Some(w) = target.and_then(|t| self.waypoints.get_mut(t)) {
                w.color = COLORS[i];
                self.save_waypoints(host);
            }
            return;
        }
        if let Some(index) = target
            && let Some(act) = ACTS.into_iter().enumerate().find(|(i, _)| inside(action(panel, *i), mouse)).map(|a| a.1)
        {
            // A name typed so far is kept before the action.
            if let Ui::Edit(edit) = &self.ui
                && edit.typed
                && !edit.name.trim().is_empty()
                && let Some(w) = self.waypoints.get_mut(index)
            {
                w.name = edit.name.trim().to_string();
            }
            if self.act(host, index, act) {
                self.ui = Ui::Map;
            } else if let (Ui::Edit(edit), Some(w)) = (&mut self.ui, self.waypoints.get(index)) {
                edit.pos = w.pos;
            }
            return;
        }
        if !inside(panel, mouse) {
            // A click outside keeps what was typed for an existing waypoint, drops a new one.
            let keep = target.is_some() && matches!(&self.ui, Ui::Edit(e) if e.typed);
            self.finish_edit(host, keep);
        }
    }

    pub(crate) fn edit_key(&mut self, host: &dyn ClientHost, key: &str) -> Flow {
        let short = key.strip_prefix("key.keyboard.").unwrap_or(key);
        let Ui::Edit(edit) = &mut self.ui else { return Flow::Continue };
        match short {
            "enter" | "keypad.enter" => self.finish_edit(host, true),
            "escape" => {
                if !edit.from_map {
                    // The game closes the screen itself.
                    return Flow::Continue;
                }
                self.finish_edit(host, false);
            }
            "backspace" => {
                edit.name.pop();
                edit.typed = true;
            }
            "tab" => edit.color = (edit.color + 1) % COLORS.len(),
            "delete" => {
                if let Some(index) = edit.target {
                    self.act(host, index, Act::Delete);
                    self.ui = Ui::Map;
                }
            }
            _ if TEST && short.len() == 1 => {
                if let Some(c) = short.chars().next() {
                    edit.type_char(c);
                }
            }
            _ => {}
        }
        Flow::Cancel
    }

    /// Opens the bubble for a new waypoint where the player stands, from the map.
    pub(crate) fn new_here(&mut self, host: &dyn ClientHost, key: &str) -> bool {
        if !host.mod_key(crate::ID, KEY_WAYPOINT).is_some_and(|(name, _)| name == key) {
            return false;
        }
        if let Some(p) = self.player {
            self.ui = Ui::Edit(Edit::new(block_of(p.pos), self.next_color, true));
        }
        true
    }
}
