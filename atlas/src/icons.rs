//! Waypoint icons: what a waypoint is drawn as, in every view, and the picker that chooses it.
//!
//! An icon is `""` (the default: a diamond, a cross for a death), `shape:<name>` (one of the
//! pixel shapes below, drawn in the waypoint's colour), `item:<item id>` (the game's own item
//! icon, drawn at runtime) or `mob:<entity id>` (the mob's face from the radar's atlas). No
//! picture is shipped: the shapes are this file's own patterns, items and faces the game's.
//!
//! The shapes are drawn on whole screen pixels: a pattern's texel is `texel(scale)` screen
//! pixels (one at GUI scale 1 and 2, two at 3 and 4, ...), its size the largest drawn pattern
//! that fits the place, so each size has its own real pixel pattern, symmetric about the
//! waypoint's pixel. A dark rim one texel wide goes round every shape; a tracked waypoint
//! gets a white ring outside that, a temporary one is hollow, a faded one dimmed.
use std::cell::RefCell;
use std::collections::HashMap;

use mod_api::client::hud_layout::GuiDraw;
use mod_api::{ClientHost, DrawContext, Identifier, ItemKey, ItemStack};

use crate::faces::{self, FACES};
use crate::gfx::{self, inside};
use crate::scrollbar::{self, Bar};
use crate::waypoints::{Kind, Waypoint};

/// The pixel shapes a waypoint can be.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape {
    Diamond,
    Cross,
    Home,
    Star,
    Flag,
    Portal,
    Skull,
    Pin,
    Heart,
    Pickaxe,
}

/// Every shape by its name in an icon id (`shape:home`) and in words.
pub const SHAPES: [(&str, &str, Shape); 10] = [
    ("diamond", "Diamond", Shape::Diamond),
    ("home", "Home", Shape::Home),
    ("star", "Star", Shape::Star),
    ("flag", "Flag", Shape::Flag),
    ("portal", "Portal", Shape::Portal),
    ("skull", "Skull", Shape::Skull),
    ("pin", "Pin", Shape::Pin),
    ("heart", "Heart", Shape::Heart),
    ("pickaxe", "Pickaxe", Shape::Pickaxe),
    ("cross", "Cross", Shape::Cross),
];

/// What an icon id means.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Icon {
    Default,
    Shape(Shape),
    /// A full item id, `minecraft:ender_pearl`.
    Item(String),
    /// A face: its place in `faces::FACES`.
    Mob(usize),
}

/// A namespaced id: `ender_pearl` is `minecraft:ender_pearl`. None for one that can't be an id.
fn full_id(id: &str) -> Option<String> {
    let id = id.trim();
    let (ns, path) = id.split_once(':').unwrap_or(("minecraft", id));
    let ok = |s: &str, extra: &str| {
        !s.is_empty() && s.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || "_-.".contains(c) || extra.contains(c))
    };
    (ok(ns, "") && ok(path, "/")).then(|| format!("{ns}:{path}"))
}

/// Reads an icon id; anything unknown is the default.
pub fn parse(id: &str) -> Icon {
    let id = id.trim();
    if let Some(name) = id.strip_prefix("shape:") {
        return SHAPES.iter().find(|s| s.0 == name).map_or(Icon::Default, |s| Icon::Shape(s.2));
    }
    if let Some(item) = id.strip_prefix("item:") {
        return full_id(item).map_or(Icon::Default, Icon::Item);
    }
    if let Some(mob) = id.strip_prefix("mob:") {
        // The faces are vanilla's mobs.
        let path = mob.strip_prefix("minecraft:").unwrap_or(mob);
        return faces::index(path).map_or(Icon::Default, Icon::Mob);
    }
    Icon::Default
}

/// The id an icon is written as.
#[cfg(test)]
pub fn id_of(icon: &Icon) -> String {
    match icon {
        Icon::Default => String::new(),
        Icon::Shape(shape) => {
            format!("shape:{}", SHAPES.iter().find(|s| s.2 == *shape).map_or("diamond", |s| s.0))
        }
        Icon::Item(id) => format!("item:{id}"),
        Icon::Mob(index) => format!("mob:minecraft:{}", FACES[*index].name),
    }
}

/// What waypoint `wp` is drawn as: its icon, else a diamond (a cross for a death).
pub fn look(wp: &Waypoint) -> Icon {
    match parse(&wp.icon) {
        Icon::Default if wp.kind == Kind::Death => Icon::Shape(Shape::Cross),
        Icon::Default => Icon::Shape(Shape::Diamond),
        other => other,
    }
}

// ---------------------------------------------------------------------------------------------
// What the drawing needs from the game: the GUI scale, item keys, face sprite names. Waypoints
// are drawn from many places that have no host at hand; `frame` fills this once a frame.

#[derive(Default)]
struct Cache {
    scale: f32,
    items: HashMap<String, Option<ItemKey>>,
    pending: Vec<String>,
    sprites: Vec<Vec<String>>,
}

thread_local! {
    static CACHE: RefCell<Cache> = RefCell::new(Cache { scale: 2.0, ..Cache::default() });
}

/// Once a frame: the GUI scale, and the item ids drawn last frame looked up.
pub fn frame(host: &dyn ClientHost, scale: f32) {
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        c.scale = scale.max(1.0);
        let pending = std::mem::take(&mut c.pending);
        for id in pending.into_iter().take(64) {
            let key = Identifier::parse(&id).ok().and_then(|item| host.item_key(&item));
            if c.items.len() > 4096 {
                c.items.clear();
            }
            c.items.insert(id, key);
        }
    });
}

fn scale() -> f32 {
    CACHE.with(|c| c.borrow().scale)
}

/// Item `id`'s key, once `frame` has looked it up.
fn item_key(id: &str) -> Option<ItemKey> {
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        match c.items.get(id) {
            Some(key) => *key,
            None => {
                if !c.pending.iter().any(|p| p == id) && c.pending.len() < 256 {
                    c.pending.push(id.to_string());
                }
                None
            }
        }
    })
}

fn sprite_names(index: usize) -> Vec<String> {
    CACHE.with(|c| {
        let mut c = c.borrow_mut();
        if c.sprites.is_empty() {
            c.sprites = (0..FACES.len()).map(faces::sprites).collect();
        }
        c.sprites.get(index).cloned().unwrap_or_default()
    })
}

/// Screen pixels a shape's texel is at GUI scale `scale`: about half a GUI pixel.
pub fn texel(scale: f32) -> i32 {
    ((scale / 2.0).round() as i32).max(1)
}

// ---------------------------------------------------------------------------------------------
// The shapes' patterns: `#` the waypoint's colour, `h` lighter, `s` darker, `o` the rim's dark,
// `.` nothing. Each comes 7, 9 and 11 texels wide; the diamond and the cross are worked out for
// any size.

/// The sides a shape is drawn at, smallest first; none for one worked out at any odd side.
fn sides(shape: Shape) -> &'static [usize] {
    match shape {
        Shape::Diamond | Shape::Cross => &[],
        _ => &[7, 9, 11],
    }
}

fn design(shape: Shape, side: usize) -> &'static [&'static str] {
    let all: [&'static [&'static str]; 3] = match shape {
        Shape::Home => [HOME_7, HOME_9, HOME_11],
        Shape::Star => [STAR_7, STAR_9, STAR_11],
        Shape::Flag => [FLAG_7, FLAG_9, FLAG_11],
        Shape::Portal => [PORTAL_7, PORTAL_9, PORTAL_11],
        Shape::Skull => [SKULL_7, SKULL_9, SKULL_11],
        Shape::Pin => [PIN_7, PIN_9, PIN_11],
        Shape::Heart => [HEART_7, HEART_9, HEART_11],
        Shape::Pickaxe => [PICKAXE_7, PICKAXE_9, PICKAXE_11],
        Shape::Diamond | Shape::Cross => return &[],
    };
    match side {
        0..=7 => all[0],
        8..=9 => all[1],
        _ => all[2],
    }
}

/// A shape's texels, `w` x `h`, one byte each as the patterns have them.
#[derive(Debug, PartialEq, Eq)]
pub struct Pattern {
    pub w: usize,
    pub h: usize,
    pub cells: Vec<u8>,
}

/// Shape `shape` about `side` texels wide.
pub fn pattern(shape: Shape, side: usize) -> Pattern {
    let side = side.max(5) | 1;
    let r = (side / 2) as i32;
    let worked = |f: &dyn Fn(i32, i32) -> bool| Pattern {
        w: side,
        h: side,
        cells: (0..side * side)
            .map(|i| {
                let (dx, dy) = (((i % side) as i32 - r).abs(), ((i / side) as i32 - r).abs());
                if f(dx, dy) { b'#' } else { b'.' }
            })
            .collect(),
    };
    match shape {
        Shape::Diamond => worked(&|dx, dy| dx + dy <= r),
        // Two strokes three texels across, on the diagonals.
        Shape::Cross => worked(&|dx, dy| (dx - dy).abs() <= 1 && dx.max(dy) <= r),
        _ => {
            let rows = design(shape, side);
            let w = rows.iter().map(|r| r.len()).max().unwrap_or(0);
            let mut cells = vec![b'.'; w * rows.len()];
            for (y, row) in rows.iter().enumerate() {
                for (x, b) in row.bytes().enumerate() {
                    cells[y * w + x] = b;
                }
            }
            Pattern { w, h: rows.len(), cells }
        }
    }
}

/// The side shape `shape` is drawn at so that it and its rim fit `room` screen pixels with
/// texels of `t`: the largest that does, else the smallest.
pub fn fit(shape: Shape, room: i32, t: i32) -> usize {
    let most = (room / t.max(1) - 2).max(0) as usize;
    match sides(shape) {
        [] => {
            let side = if most % 2 == 1 { most } else { most.saturating_sub(1) };
            side.max(5)
        }
        list => list.iter().rev().copied().find(|s| *s <= most).unwrap_or(list[0]),
    }
}

/// How a pattern is drawn.
#[derive(Clone, Copy, Debug, Default)]
pub struct Style {
    pub rgb: u32,
    pub tracked: bool,
    pub faded: bool,
    pub hollow: bool,
}

const WHITE: u32 = 0xFFFF_FFFF;
const DARK: u32 = 0xFF00_0000;

fn with_alpha(argb: u32, alpha: u32) -> u32 {
    (argb & 0x00FF_FFFF) | alpha << 24
}

fn mix(rgb: u32, to: u32, k: u32) -> u32 {
    let ch = |shift: u32| {
        let (a, b) = ((rgb >> shift) & 0xFF, (to >> shift) & 0xFF);
        (a * (256 - k) + b * k) / 256
    };
    ch(16) << 16 | ch(8) << 8 | ch(0)
}

/// One run of texels of one colour: x, y, width (in texels), colour.
pub type Run = (i32, i32, i32, u32);

/// Pattern `pat` with its rim (and ring) as runs of texels in a grid `pad` texels bigger on
/// every side than the pattern, `pad` being 1, 2 when tracked. Answers the grid's size.
pub fn paint(pat: &Pattern, style: Style) -> ([i32; 2], Vec<Run>) {
    let pad = 1 + usize::from(style.tracked);
    let (gw, gh) = (pat.w + 2 * pad, pat.h + 2 * pad);
    let mut cell = vec![b'.'; gw * gh];
    for y in 0..pat.h {
        for x in 0..pat.w {
            cell[(y + pad) * gw + x + pad] = pat.cells[y * pat.w + x];
        }
    }
    let shape: Vec<bool> = cell.iter().map(|c| *c != b'.').collect();
    let grow = |mask: &[bool], eight: bool| -> Vec<bool> {
        (0..gw * gh)
            .map(|i| {
                let (x, y) = ((i % gw) as i32, (i / gw) as i32);
                (-1..=1).any(|dy: i32| {
                    (-1..=1).any(|dx: i32| {
                        let (nx, ny) = (x + dx, y + dy);
                        (eight || dx == 0 || dy == 0)
                            && nx >= 0
                            && ny >= 0
                            && (nx as usize) < gw
                            && (ny as usize) < gh
                            && mask[ny as usize * gw + nx as usize]
                    })
                })
            })
            .collect()
    };
    let shrink = |mask: &[bool]| -> Vec<bool> {
        let not: Vec<bool> = mask.iter().map(|m| !m).collect();
        grow(&not, false).iter().map(|m| !m).collect()
    };
    let rim = grow(&shape, false);
    let ring = if style.tracked { grow(&rim, true) } else { rim.clone() };
    // Hollow (temporary): an outline of the colour, a texel wide on a small shape and two on a
    // big one, round a dark middle; a big one's middle shows through behind a dark edge.
    let (inner, hole) = if style.hollow {
        let inner = if pat.w >= 11 { shrink(&shrink(&shape)) } else { shrink(&shape) };
        let hole = if pat.w >= 9 { shrink(&inner) } else { vec![false; gw * gh] };
        (inner, hole)
    } else {
        (vec![false; gw * gh], vec![false; gw * gh])
    };
    let (fill_a, dark_a, white_a) = if style.faded { (0x80, 0x60, 0x90) } else { (0xFF, 0xFF, 0xFF) };
    let color = |i: usize| -> u32 {
        if hole[i] {
            return 0;
        }
        if inner[i] {
            return with_alpha(DARK, dark_a);
        }
        if shape[i] {
            return match cell[i] {
                b'h' => with_alpha(mix(style.rgb, 0xFFFFFF, 115), fill_a),
                b's' => with_alpha(mix(style.rgb, 0x000000, 110), fill_a),
                b'o' => with_alpha(DARK, dark_a),
                _ => with_alpha(style.rgb, fill_a),
            };
        }
        if rim[i] {
            return with_alpha(DARK, dark_a);
        }
        if ring[i] {
            return with_alpha(WHITE, white_a);
        }
        0
    };
    let mut runs = Vec::new();
    for y in 0..gh {
        let mut x = 0;
        while x < gw {
            let c = color(y * gw + x);
            let mut end = x + 1;
            while end < gw && color(y * gw + end) == c {
                end += 1;
            }
            if c != 0 {
                runs.push((x as i32, y as i32, (end - x) as i32, c));
            }
            x = end;
        }
    }
    ([gw as i32, gh as i32], runs)
}

/// The screen pixel `at` (GUI pixels) is in, at scale `s`.
fn pixel(at: [f32; 2], s: f32) -> [i32; 2] {
    [(at[0] * s).floor() as i32, (at[1] * s).floor() as i32]
}

/// Draws runs of texels `t` screen pixels each, the grid's top left at screen pixel `corner`.
fn draw_runs(draw: &mut dyn DrawContext, runs: &[Run], corner: [i32; 2], t: i32, s: f32) {
    for &(x, y, w, c) in runs {
        let (px, py) = (corner[0] + x * t, corner[1] + y * t);
        draw.fill([px as f32 / s, py as f32 / s, (w * t) as f32 / s, t as f32 / s], c);
    }
}

/// Shape `shape` centred on `at`, it and its rim `room` screen pixels across, texels `t`.
fn shape_at(draw: &mut dyn DrawContext, shape: Shape, at: [f32; 2], room: i32, t: i32, s: f32, style: Style) {
    let pat = pattern(shape, fit(shape, room, t));
    let ([gw, gh], runs) = paint(&pat, style);
    let c = pixel(at, s);
    draw_runs(draw, &runs, [c[0] - gw * t / 2, c[1] - gh * t / 2], t, s);
}

/// A ring one texel wide round the screen rectangle `r` (pixels), its corners left out: the
/// tracked mark round an item or a face.
fn ring_px(draw: &mut dyn DrawContext, r: [i32; 4], t: i32, s: f32, color: u32) {
    let f = |draw: &mut dyn DrawContext, x: i32, y: i32, w: i32, h: i32| {
        draw.fill([x as f32 / s, y as f32 / s, w as f32 / s, h as f32 / s], color);
    };
    let [x, y, w, h] = r;
    f(draw, x, y - t, w, t);
    f(draw, x, y + h, w, t);
    f(draw, x - t, y, t, h);
    f(draw, x + w, y, t, h);
}

/// Item `key` centred on `at` on a small dark plate, at a whole number of screen pixels a
/// texel (16 texels), about `room` screen pixels across.
fn item_at(draw: &mut dyn DrawContext, key: ItemKey, at: [f32; 2], room: i32, t: i32, s: f32, style: Style) -> bool {
    let side = 16 * ((room + 4) / 16).max(1);
    let c = pixel(at, s);
    let (x, y) = (c[0] - side / 2, c[1] - side / 2);
    // Opaque: the game draws a see-through fill under the mobs' faces drawn before it.
    let plate = if style.faded { 0x6000_0000 } else { 0xFF16_1619 };
    let f = |draw: &mut dyn DrawContext, r: [i32; 4], color: u32| {
        draw.fill([r[0] as f32 / s, r[1] as f32 / s, r[2] as f32 / s, r[3] as f32 / s], color);
    };
    // Three strips, its corners cut, none over another.
    f(draw, [x, y - t, side, side + 2 * t], plate);
    f(draw, [x - t, y, t, side], plate);
    f(draw, [x + side, y, t, side], plate);
    if style.hollow {
        // Temporary: the plate's edge in the waypoint's colour, dotted.
        let edge = with_alpha(style.rgb, if style.faded { 0x80 } else { 0xFF });
        let mut i = 0;
        while i < side {
            f(draw, [x + i, y - t, t, t], edge);
            f(draw, [x + i, y + side, t, t], edge);
            f(draw, [x - t, y + i, t, t], edge);
            f(draw, [x + side, y + i, t, t], edge);
            i += 3 * t;
        }
    }
    if style.tracked {
        ring_px(draw, [x - t, y - t, side + 2 * t, side + 2 * t], t, s, WHITE);
    }
    let gui = [x as f32 / s, y as f32 / s];
    if !draw.item(key, gui, side as f32 / s) {
        return false;
    }
    if style.faded {
        f(draw, [x, y, side, side], 0x9018_1818);
    }
    true
}

/// Face `index` centred on `at`, it and its rim about `room` screen pixels across, on a dark
/// edge `t` wide as the radar draws faces.
fn face_at(draw: &mut dyn DrawContext, index: usize, at: [f32; 2], room: i32, t: i32, s: f32, style: Style) -> bool {
    let face = &FACES[index];
    let (w, h) = (f32::from(face.w), f32::from(face.h));
    let most = w.max(h);
    let inner = (room - 2 * t).max(4) as f32;
    // Whole screen pixels a texel when that is near the room; a big face is shrunk to fit.
    let whole = (inner / most).round().max(1.0);
    let k = if most * whole > inner * 1.5 { inner / most } else { whole };
    let (fw, fh) = ((w * k).round() as i32, (h * k).round() as i32);
    let c = pixel(at, s);
    let corner = [c[0] - fw / 2, c[1] - fh / 2];
    let names = sprite_names(index);
    let edge = |v: u8| (f32::from(v) * k).round() as i32;
    let rects: Vec<[i32; 4]> = face
        .parts
        .iter()
        .map(|p| {
            let (x0, y0, x1, y1) = (edge(p[0]), edge(p[1]), edge(p[0] + p[2]), edge(p[1] + p[3]));
            [corner[0] + x0, corner[1] + y0, (x1 - x0).max(1), (y1 - y0).max(1)]
        })
        .collect();
    let g = |r: [i32; 4]| [r[0] as f32 / s, r[1] as f32 / s, r[2] as f32 / s, r[3] as f32 / s];
    if style.tracked {
        ring_px(draw, [corner[0] - t, corner[1] - t, fw + 2 * t, fh + 2 * t], t, s, WHITE);
    }
    let rim = if style.faded { 0x7000_0000 } else if style.hollow { with_alpha(style.rgb, 0xFF) } else { DARK };
    for r in &rects {
        draw.fill(g([r[0] - t, r[1] - t, r[2] + 2 * t, r[3] + 2 * t]), rim);
    }
    let tint = style.faded.then_some(0x80FF_FFFF);
    for ((part, r), name) in face.parts.iter().zip(&rects).zip(&names) {
        if !draw.sprite(name, [0.0, 0.0, f32::from(part[2]), f32::from(part[3])], g(*r), tint) {
            return false;
        }
    }
    // The eyes, over the face.
    if let Some(eyes) = names.get(face.parts.len()) {
        let part = face.parts[0];
        draw.sprite(eyes, [0.0, 0.0, f32::from(part[2]), f32::from(part[3])], g(rects[0]), tint);
    }
    true
}

/// Draws waypoint `wp` centred on `at`, about `half` GUI pixels from its middle to an edge
/// (the rim one GUI pixel past that), dimmed when `faded`. `gfx::waypoint_icon` is this.
pub fn draw_waypoint(draw: &mut dyn DrawContext, wp: &Waypoint, at: [f32; 2], half: f32, faded: bool) {
    let s = scale();
    let t = texel(s);
    let room = ((2.0 * half + 2.0) * s).round() as i32;
    let style = Style { rgb: wp.color, tracked: wp.tracked, faded, hollow: wp.temporary };
    draw_icon(draw, &look(wp), at, room, t, s, style);
}

/// How far, in GUI pixels, waypoint `wp` drawn at `half` reaches from its middle to its
/// furthest edge (rim and tracked ring included): an item or a face can be wider than a shape.
pub fn reach(wp: &Waypoint, half: f32) -> f32 {
    let s = scale();
    let t = texel(s);
    let room = ((2.0 * half + 2.0) * s).round() as i32;
    let ring = if wp.tracked { t } else { 0 };
    let shape = |shape: Shape| (fit(shape, room, t) as i32 + 2) * t / 2 + ring;
    let px = match look(wp) {
        Icon::Shape(sh) => shape(sh),
        Icon::Item(id) if item_key(&id).is_some() => 16 * ((room + 4) / 16).max(1) / 2 + t + ring,
        Icon::Mob(index) => {
            let face = &FACES[index];
            let most = f32::from(face.w.max(face.h));
            let inner = (room - 2 * t).max(4) as f32;
            let whole = (inner / most).round().max(1.0);
            let k = if most * whole > inner * 1.5 { inner / most } else { whole };
            (most * k).round() as i32 / 2 + t + ring
        }
        _ => shape(Shape::Diamond),
    };
    px as f32 / s
}

/// Icon `icon` centred on `at` in a room of `room` screen pixels; the diamond for an item or
/// a face the game doesn't have.
fn draw_icon(draw: &mut dyn DrawContext, icon: &Icon, at: [f32; 2], room: i32, t: i32, s: f32, style: Style) {
    let drawn = match icon {
        Icon::Shape(shape) => {
            shape_at(draw, *shape, at, room, t, s, style);
            true
        }
        Icon::Item(id) => item_key(id).is_some_and(|key| item_at(draw, key, at, room, t, s, style)),
        Icon::Mob(index) => face_at(draw, *index, at, room, t, s, style),
        Icon::Default => false,
    };
    if !drawn {
        shape_at(draw, Shape::Diamond, at, room, t, s, style);
    }
}

// ---------------------------------------------------------------------------------------------
// In the in-world labels: drawn in the label's own pose, whose unit is `font` screen pixels.

/// The icon at the left of a waypoint's label.
pub struct LabelIcon {
    /// Its width in label units.
    pub width: i32,
    what: LabelWhat,
}

enum LabelWhat {
    Shape([i32; 2], Vec<Run>),
    Item(ItemKey, i32),
    /// Its place in `FACES`, size in label units, texels a unit, and its rim's colour.
    Face(usize, [i32; 2], f32, u32),
}

impl LabelIcon {
    /// Waypoint `wp`'s label icon, a label unit being `font` screen pixels.
    pub fn of(wp: &Waypoint, font: f32) -> Self {
        let style = Style { rgb: wp.color, tracked: wp.tracked, faded: false, hollow: wp.temporary };
        let ring = 2 * i32::from(wp.tracked);
        match look(wp) {
            // An item's sixteen texels are sharp only at two screen pixels a unit or more.
            Icon::Item(id) if font >= 2.0 => {
                if let Some(key) = item_key(&id) {
                    let side = (16.0 / font).round() as i32;
                    return Self { width: side + ring, what: LabelWhat::Item(key, side) };
                }
            }
            Icon::Mob(index) => {
                let face = &FACES[index];
                let most = f32::from(face.w.max(face.h));
                let k = if most <= 9.0 { 1.0 } else { 8.0 / most };
                let size = [(f32::from(face.w) * k).round() as i32, (f32::from(face.h) * k).round() as i32];
                // A temporary one's rim is its colour, as on the maps.
                let rim = if wp.temporary { with_alpha(wp.color, 0xFF) } else { 0xE000_0000 };
                return Self { width: size[0] + 2 + ring, what: LabelWhat::Face(index, size, k, rim) };
            }
            Icon::Shape(shape) => {
                let (grid, runs) = paint(&pattern(shape, 7), style);
                return Self { width: grid[0], what: LabelWhat::Shape(grid, runs) };
            }
            _ => {}
        }
        let (grid, runs) = paint(&pattern(Shape::Diamond, 7), style);
        Self { width: grid[0], what: LabelWhat::Shape(grid, runs) }
    }

    /// Its fills, in label units, its left edge at `x` and its middle at height `mid`.
    pub fn paint(&self, list: &mut Vec<GuiDraw>, x: i32, mid: i32, tracked: bool) {
        let fill = |list: &mut Vec<GuiDraw>, r: [i32; 4], color: u32| {
            list.push(GuiDraw::Fill { x0: r[0], y0: r[1], x1: r[0] + r[2], y1: r[1] + r[3], color });
        };
        let ring = |list: &mut Vec<GuiDraw>, r: [i32; 4]| {
            fill(list, [r[0], r[1] - 1, r[2], 1], WHITE);
            fill(list, [r[0], r[1] + r[3], r[2], 1], WHITE);
            fill(list, [r[0] - 1, r[1], 1, r[3]], WHITE);
            fill(list, [r[0] + r[2], r[1], 1, r[3]], WHITE);
        };
        let pad = i32::from(tracked);
        match &self.what {
            LabelWhat::Shape(grid, runs) => {
                let top = mid - grid[1] / 2;
                for &(rx, ry, w, c) in runs {
                    fill(list, [x + rx, top + ry, w, 1], c);
                }
            }
            LabelWhat::Item(_, side) => {
                if tracked {
                    ring(list, [x + pad, mid - side / 2, *side, *side]);
                }
            }
            LabelWhat::Face(_, size, _, rim) => {
                let r = [x + pad + 1, mid - size[1] / 2, size[0], size[1]];
                if tracked {
                    ring(list, [r[0] - 1, r[1] - 1, r[2] + 2, r[3] + 2]);
                }
                fill(list, [r[0] - 1, r[1] - 1, r[2] + 2, r[3] + 2], *rim);
            }
        }
    }

    /// What the label's pose can't draw (an item, a face's sprites), drawn after it in GUI
    /// pixels: the pose's origin `origin`, `k` GUI pixels a unit.
    pub fn draw_after(&self, draw: &mut dyn DrawContext, origin: [f32; 2], k: f32, x: i32, mid: i32, tracked: bool) {
        let pad = i32::from(tracked);
        let at = |ux: i32, uy: i32| [origin[0] + ux as f32 * k, origin[1] + uy as f32 * k];
        match &self.what {
            LabelWhat::Shape(..) => {}
            LabelWhat::Item(key, side) => {
                draw.item(*key, at(x + pad, mid - side / 2), *side as f32 * k);
            }
            LabelWhat::Face(index, size, scale, _) => {
                let face = &FACES[*index];
                let corner = [x + pad + 1, mid - size[1] / 2];
                // The parts, then the eyes over the first.
                let names = sprite_names(*index);
                let parts = face.parts.iter().chain(face.parts.first().filter(|_| names.len() > face.parts.len()));
                for (part, name) in parts.zip(&names) {
                    let edge = |v: u8| (f32::from(v) * scale).round() as i32;
                    let (x0, y0, x1, y1) = (edge(part[0]), edge(part[1]), edge(part[0] + part[2]), edge(part[1] + part[3]));
                    let p = at(corner[0] + x0, corner[1] + y0);
                    let dst = [p[0], p[1], ((x1 - x0).max(1)) as f32 * k, ((y1 - y0).max(1)) as f32 * k];
                    draw.sprite(name, [0.0, 0.0, f32::from(part[2]), f32::from(part[3])], dst, None);
                }
            }
        }
    }
}

// ---------------------------------------------------------------------------------------------
// The picker: a search box over a grid of every icon there is.

/// One icon the picker offers.
pub struct Choice {
    /// Its icon id (`""` for the default).
    pub id: String,
    /// What it is called (the game's name for a mob or an item).
    pub name: String,
    /// Its name and id in lower case: what a search looks in.
    hay: String,
    what: What,
}

#[derive(Clone, Copy)]
enum What {
    Default,
    Shape(Shape),
    Mob(usize),
    Item(ItemKey),
}

impl Choice {
    fn new(id: String, name: String, what: What) -> Self {
        let hay = format!("{} {}", name.to_lowercase(), id.to_lowercase());
        Self { id, name, hay, what }
    }
}

/// Whether every word of `query` (lower case) is in `hay`.
pub fn matches(hay: &str, query: &str) -> bool {
    query.split_whitespace().all(|word| hay.contains(word))
}

/// The places in `all` of those that search `query` finds, in their order.
fn search(all: &[Choice], query: &str) -> Vec<usize> {
    let query = query.trim().to_lowercase();
    all.iter().enumerate().filter(|(_, c)| matches(&c.hay, &query)).map(|(i, _)| i).collect()
}

/// A cell of the grid, GUI pixels.
const CELL: f32 = 18.0;
/// Pixels either side of the scrollbar that take the mouse too: the gap to the grid and the rim.
const BAR_PAD: f32 = 3.0;
/// Items named a frame while the list is filled.
const NAMES_A_FRAME: usize = 192;

/// What a key did in the picker.
#[derive(Debug, PartialEq, Eq)]
pub enum Reply {
    /// Not the picker's: the caller handles it.
    Ignored,
    Handled,
    /// Enter on a search: its first icon.
    Chose(String),
}

pub struct Picker {
    query: String,
    choices: Vec<Choice>,
    /// The item registry, named a few each frame into `choices`.
    items: Option<Vec<ItemKey>>,
    named: usize,
    shown: Vec<usize>,
    stale: bool,
    /// The first row shown.
    row: usize,
    /// The icon id the waypoint has: outlined.
    pub current: String,
    /// Where the last frame drew the grid: its top left, columns, rows.
    grid: Option<([f32; 2], usize, usize)>,
    hovered: Option<usize>,
    /// The scrollbar as the last frame drew it (none when everything fits), and where its thumb
    /// is held while it is dragged.
    bar: Option<Bar>,
    held: Option<f32>,
}

impl Default for Picker {
    fn default() -> Self {
        Self::new("")
    }
}

impl Picker {
    /// A picker for a waypoint whose icon is `current`.
    pub fn new(current: &str) -> Self {
        Self {
            query: String::new(),
            choices: Vec::new(),
            items: None,
            named: 0,
            shown: Vec::new(),
            stale: true,
            row: 0,
            current: current.to_string(),
            grid: None,
            hovered: None,
            bar: None,
            held: None,
        }
    }

    #[cfg(test)]
    pub fn query(&self) -> &str {
        &self.query
    }

    #[cfg(test)]
    pub fn set_query(&mut self, query: &str) {
        self.query = query.chars().take(40).collect();
        self.stale = true;
        self.row = 0;
    }

    pub fn type_char(&mut self, c: char) {
        if !c.is_control() && self.query.chars().count() < 40 {
            self.query.push(c);
            self.stale = true;
            self.row = 0;
        }
    }

    /// A key (`key.keyboard.backspace`): Backspace edits the search, Page Up / Down and the
    /// arrows scroll, Enter takes the first icon found.
    pub fn key(&mut self, key: &str) -> Reply {
        let short = key.strip_prefix("key.keyboard.").unwrap_or(key);
        match short {
            "backspace" => {
                if self.query.pop().is_none() {
                    return Reply::Ignored;
                }
                self.stale = true;
                self.row = 0;
            }
            "page.up" | "up" => self.scroll(if short == "up" { 1.0 } else { 4.0 }),
            "page.down" | "down" => self.scroll(if short == "down" { -1.0 } else { -4.0 }),
            "enter" | "keypad.enter" if !self.query.is_empty() => {
                return match self.shown.first() {
                    Some(i) => Reply::Chose(self.choices[*i].id.clone()),
                    None => Reply::Handled,
                };
            }
            _ => return Reply::Ignored,
        }
        Reply::Handled
    }

    /// Scrolls by `steps` rows, up for positive as a wheel turns.
    pub fn scroll(&mut self, steps: f32) {
        let rows = self.grid.map_or(1, |g| g.2);
        let cols = self.grid.map_or(1, |g| g.1).max(1);
        let total = self.shown.len().div_ceil(cols);
        let most = total.saturating_sub(rows);
        let to = self.row as f32 - steps.round();
        self.row = (to.max(0.0) as usize).min(most);
    }

    /// A press at `at`: on the scrollbar it moves the list there and holds the thumb. True when
    /// the bar took it.
    pub fn press_bar(&mut self, at: [f32; 2]) -> bool {
        let Some(bar) = self.bar.filter(|b| inside(b.grip(BAR_PAD), at)) else { return false };
        let (row, grab) = bar.press(at[1]);
        self.row = row;
        self.held = Some(grab);
        true
    }

    /// The held thumb follows the cursor at `at`, wherever it is. True while it is held.
    pub fn drag_bar(&mut self, at: [f32; 2]) -> bool {
        let (Some(bar), Some(grab)) = (self.bar, self.held) else { return false };
        self.row = bar.first_at(at[1], grab);
        true
    }

    /// Lets the thumb go. True when it was held.
    pub fn release_bar(&mut self) -> bool {
        self.held.take().is_some()
    }

    pub fn holding_bar(&self) -> bool {
        self.held.is_some()
    }

    /// The icon id under `at`, if a click there chose one (`""` is the default).
    pub fn click(&self, at: [f32; 2]) -> Option<String> {
        let index = self.cell_at(at)?;
        Some(self.choices[self.shown[index]].id.clone())
    }

    /// The place in `shown` of the cell under `at`.
    fn cell_at(&self, at: [f32; 2]) -> Option<usize> {
        let (origin, cols, rows) = self.grid?;
        if !inside([origin[0], origin[1], cols as f32 * CELL, rows as f32 * CELL], at) {
            return None;
        }
        let (c, r) = (((at[0] - origin[0]) / CELL) as usize, ((at[1] - origin[1]) / CELL) as usize);
        let index = (self.row + r) * cols + c;
        (index < self.shown.len()).then_some(index)
    }

    /// Fills the list: the default and the shapes, the faces, then the items, a few a frame.
    fn fill(&mut self, host: &dyn ClientHost) {
        if self.choices.is_empty() {
            self.choices.push(Choice::new(String::new(), "Default".into(), What::Default));
            // The default is the diamond: it is not offered twice.
            for (name, words, shape) in SHAPES.iter().filter(|s| s.2 != Shape::Diamond) {
                self.choices.push(Choice::new(format!("shape:{name}"), (*words).into(), What::Shape(*shape)));
            }
            for (i, face) in FACES.iter().enumerate() {
                let name = host
                    .translate(&format!("entity.minecraft.{}", face.name))
                    .unwrap_or_else(|| crate::radar::pretty(face.name));
                self.choices.push(Choice::new(format!("mob:minecraft:{}", face.name), name, What::Mob(i)));
            }
            self.stale = true;
        }
        let items = self.items.get_or_insert_with(|| host.item_keys());
        let end = (self.named + NAMES_A_FRAME).min(items.len());
        for &key in &items[self.named..end] {
            let id = key.id(host);
            if id == "minecraft:air" {
                continue;
            }
            let name = host.hover_name(&ItemStack::of(key));
            let name = if name.is_empty() { crate::radar::pretty(&id) } else { name };
            self.choices.push(Choice::new(format!("item:{id}"), name, What::Item(key)));
        }
        if end > self.named {
            self.named = end;
            self.stale = true;
        }
    }

    fn loading(&self) -> bool {
        self.items.as_ref().is_none_or(|items| self.named < items.len())
    }

    /// Draws the picker in `rect` (GUI pixels): a tooltip-coloured panel with the search box at
    /// the top, the grid under it and the name of what the cursor is on at the bottom. Shapes
    /// are shown in colour `rgb`. About 150 x 120 or more reads well.
    pub fn draw(&mut self, draw: &mut dyn DrawContext, rect: [f32; 4], mouse: [f32; 2], host: &dyn ClientHost, rgb: u32) {
        self.fill(host);
        if self.stale {
            self.shown = search(&self.choices, &self.query);
            self.stale = false;
        }
        let [x, y, w, h] = rect;
        gfx::bubble(draw, rect, None);
        // The search, as a vanilla text box in focus.
        let f = [x + 5.0, y + 5.0, w - 10.0, 14.0];
        draw.fill(f, WHITE);
        draw.fill([f[0] + 1.0, f[1] + 1.0, f[2] - 2.0, f[3] - 2.0], DARK);
        let (tx, ty) = (f[0] + 4.0, f[1] + 3.0);
        if self.query.is_empty() {
            draw.text("Search icons", [tx, ty], 0xFF70_7070, false);
        } else {
            let mut shown = self.query.clone();
            while draw.text_width(&shown) > f[2] - 10.0 {
                shown.remove(0);
            }
            draw.text(&shown, [tx, ty], 0xFFE0_E0E0, false);
        }
        if !self.query.is_empty() && (host.millis() / 300).is_multiple_of(2) {
            let cx = tx + draw.text_width(&self.query).min(f[2] - 10.0);
            draw.fill([cx, ty - 1.0, 1.0, 10.0], 0xFFD0_D0D0);
        }
        // The grid.
        let cols = (((w - 12.0) / CELL).floor() as usize).max(1);
        let rows = (((h - 22.0 - 15.0) / CELL).floor() as usize).max(1);
        let origin = [(x + (w - 4.0 - cols as f32 * CELL) / 2.0).round(), y + 22.0];
        self.grid = Some((origin, cols, rows));
        let total = self.shown.len().div_ceil(cols);
        // A thin bar at the right when there is more than shows, in the middle of the room
        // between the grid and the rim.
        let grid_right = origin[0] + cols as f32 * CELL;
        let bar_x = grid_right + ((x + w - 2.0 - grid_right - scrollbar::WIDTH) / 2.0).floor();
        self.bar = (total > rows).then(|| Bar {
            track: [bar_x, origin[1], scrollbar::WIDTH, rows as f32 * CELL],
            first: self.row,
            shown: rows,
            total,
        });
        if self.bar.is_none() {
            self.held = None;
        }
        // A held thumb goes where the cursor is.
        self.drag_bar(mouse);
        self.row = self.row.min(total.saturating_sub(rows));
        if let Some(bar) = &mut self.bar {
            bar.first = self.row;
        }
        // While the thumb is held no icon is under the cursor.
        self.hovered = if self.held.is_some() { None } else { self.cell_at(mouse) };
        let s = scale();
        let t = texel(s);
        for r in 0..rows {
            for c in 0..cols {
                let index = (self.row + r) * cols + c;
                let Some(&choice) = self.shown.get(index) else { break };
                let cell = [origin[0] + c as f32 * CELL, origin[1] + r as f32 * CELL];
                let choice = &self.choices[choice];
                if self.hovered == Some(index) {
                    draw.fill([cell[0], cell[1], CELL, CELL], 0x40FF_FFFF);
                }
                if choice.id == self.current {
                    let o = [cell[0], cell[1], CELL, CELL];
                    draw.fill([o[0], o[1], o[2], 1.0], WHITE);
                    draw.fill([o[0], o[1] + o[3] - 1.0, o[2], 1.0], WHITE);
                    draw.fill([o[0], o[1] + 1.0, 1.0, o[3] - 2.0], WHITE);
                    draw.fill([o[0] + o[2] - 1.0, o[1] + 1.0, 1.0, o[3] - 2.0], WHITE);
                }
                let mid = [cell[0] + CELL / 2.0, cell[1] + CELL / 2.0];
                let style = Style { rgb, ..Style::default() };
                let room = (14.0 * s).round() as i32;
                match choice.what {
                    What::Default => shape_at(draw, Shape::Diamond, mid, (10.0 * s) as i32, t, s, style),
                    What::Shape(shape) => {
                        // As big as fits: up to twice the map's texel.
                        let (pt, side) = [t * 2, t]
                            .into_iter()
                            .map(|pt| (pt, fit(shape, room, pt)))
                            .find(|(pt, side)| (*side as i32 + 2) * pt <= room)
                            .unwrap_or((t, fit(shape, room, t)));
                        let pat = pattern(shape, side);
                        let ([gw, gh], runs) = paint(&pat, style);
                        let p = pixel(mid, s);
                        draw_runs(draw, &runs, [p[0] - gw * pt / 2, p[1] - gh * pt / 2], pt, s);
                    }
                    What::Mob(i) => {
                        face_at(draw, i, mid, room, t, s, style);
                    }
                    What::Item(key) => {
                        draw.item(key, [cell[0] + 1.0, cell[1] + 1.0], 16.0);
                    }
                }
            }
        }
        if let Some(bar) = self.bar {
            bar.draw(draw, mouse, self.held.is_some(), BAR_PAD);
        }
        // What the cursor is on, else how many there are.
        let line = match self.hovered {
            Some(index) => {
                let choice = &self.choices[self.shown[index]];
                if choice.id.is_empty() { "Default (diamond, a cross for a death)".to_string() } else { choice.name.clone() }
            }
            None if self.loading() => "Loading items...".to_string(),
            None if self.shown.is_empty() => "Nothing found".to_string(),
            None => format!("{} icons", self.shown.len()),
        };
        let line = gfx::fit(draw, &line, w - 10.0);
        let color = if self.hovered.is_some() { 0xFFFF_FFFF } else { 0xFF90_90A0 };
        draw.text(&line, [x + 5.0, y + h - 12.0], color, false);
    }
}

const HOME_7: &[&str] = &[
    "...#...",
    "..###..",
    ".#####.",
    "#######",
    ".##o##.",
    ".##o##.",
    ".##o##.",
];
const HOME_9: &[&str] = &[
    "....#....",
    "...###...",
    "..#####..",
    ".#######.",
    "#########",
    ".###o###.",
    ".###o###.",
    ".###o###.",
    ".###o###.",
];
const HOME_11: &[&str] = &[
    ".....#.....",
    "....###....",
    "...#####...",
    "..#######..",
    ".#########.",
    "###########",
    ".####o####.",
    ".###ooo###.",
    ".###ooo###.",
    ".###ooo###.",
    ".###ooo###.",
];
const STAR_7: &[&str] = &[
    "...#...",
    "...#...",
    "#######",
    ".#####.",
    "..###..",
    ".##.##.",
    ".#...#.",
];
const STAR_9: &[&str] = &[
    "....#....",
    "....#....",
    "...###...",
    "#########",
    ".#######.",
    "..#####..",
    "..##.##..",
    ".##...##.",
    ".#.....#.",
];
const STAR_11: &[&str] = &[
    ".....#.....",
    ".....#.....",
    "....###....",
    "....###....",
    "###########",
    ".#########.",
    "..#######..",
    "...#####...",
    "..###.###..",
    "..##...##..",
    ".##.....##.",
];
const FLAG_7: &[&str] = &[
    "##.....",
    "#####..",
    "#######",
    "#####..",
    "##.....",
    "#......",
    "#......",
];
const FLAG_9: &[&str] = &[
    "###......",
    "######...",
    "#########",
    "######...",
    "###......",
    "#........",
    "#........",
    "#........",
    "#........",
];
const FLAG_11: &[&str] = &[
    "##.........",
    "#####......",
    "########...",
    "###########",
    "########...",
    "#####......",
    "##.........",
    "#..........",
    "#..........",
    "#..........",
    "#..........",
];
const PORTAL_7: &[&str] = &[
    "ooooo",
    "o###o",
    "o#h#o",
    "o###o",
    "oh##o",
    "o##ho",
    "ooooo",
];
const PORTAL_9: &[&str] = &[
    "ooooooo",
    "o#####o",
    "o##h##o",
    "o#h###o",
    "o###h#o",
    "o#####o",
    "o##h##o",
    "o#####o",
    "ooooooo",
];
const PORTAL_11: &[&str] = &[
    "ooooooooo",
    "o#######o",
    "o###h###o",
    "o##h####o",
    "o#####h#o",
    "o####h##o",
    "o#######o",
    "o##h####o",
    "o###h###o",
    "o#######o",
    "ooooooooo",
];
const SKULL_7: &[&str] = &[
    ".#####.",
    "#######",
    "#oo#oo#",
    "#oo#oo#",
    ".##o##.",
    ".#####.",
    "..#.#..",
];
const SKULL_9: &[&str] = &[
    "..#####..",
    ".#######.",
    "#########",
    "#ooo#ooo#",
    "#ooo#ooo#",
    ".###o###.",
    "..#####..",
    "..#####..",
    "..#.#.#..",
];
const SKULL_11: &[&str] = &[
    "...#####...",
    ".#########.",
    "###########",
    "###########",
    "##ooo#ooo##",
    "##ooo#ooo##",
    ".####o####.",
    "..#######..",
    "..#######..",
    "..#.#.#.#..",
    "...........",
];
const PIN_7: &[&str] = &[
    ".#####.",
    "###o###",
    "##ooo##",
    "###o###",
    ".#####.",
    "..###..",
    "...#...",
];
const PIN_9: &[&str] = &[
    "..#####..",
    ".#######.",
    "###ooo###",
    "###ooo###",
    "###ooo###",
    ".#######.",
    "..#####..",
    "...###...",
    "....#....",
];
const PIN_11: &[&str] = &[
    "...#####...",
    ".#########.",
    ".#########.",
    "####ooo####",
    "####ooo####",
    "####ooo####",
    ".#########.",
    ".#########.",
    "...#####...",
    "....###....",
    ".....#.....",
];
const HEART_7: &[&str] = &[
    ".##.##.",
    "#######",
    "#######",
    "#######",
    ".#####.",
    "..###..",
    "...#...",
];
const HEART_9: &[&str] = &[
    ".###.###.",
    "#########",
    "#########",
    "#########",
    ".#######.",
    "..#####..",
    "...###...",
    "....#....",
];
const HEART_11: &[&str] = &[
    "..##...##..",
    ".####.####.",
    "###########",
    "###########",
    "###########",
    ".#########.",
    "..#######..",
    "...#####...",
    "....###....",
    ".....#.....",
];
const PICKAXE_7: &[&str] = &[
    ".####..",
    "...###.",
    "...s##.",
    "..s.##.",
    ".s...#.",
    "s......",
    "s......",
];
const PICKAXE_9: &[&str] = &[
    "..####...",
    ".....###.",
    ".....s##.",
    "....s.##.",
    "...s...#.",
    "..s....#.",
    ".s.......",
    "s........",
    "s........",
];
const PICKAXE_11: &[&str] = &[
    "..#####....",
    "......###..",
    ".......##..",
    "......s###.",
    ".....s..##.",
    "....s....#.",
    "...s.....#.",
    "..s........",
    ".s.........",
    "s..........",
    "...........",
];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn icon_ids_read_and_write_back() {
        assert_eq!(parse(""), Icon::Default);
        assert_eq!(parse("shape:home"), Icon::Shape(Shape::Home));
        assert_eq!(parse("shape:nonsense"), Icon::Default);
        assert_eq!(parse("item:minecraft:ender_pearl"), Icon::Item("minecraft:ender_pearl".into()));
        assert_eq!(parse("item:ender_pearl"), Icon::Item("minecraft:ender_pearl".into()));
        assert_eq!(parse("item:Not An Id"), Icon::Default);
        assert_eq!(parse("item:"), Icon::Default);
        let enderman = faces::index("enderman").unwrap();
        assert_eq!(parse("mob:minecraft:enderman"), Icon::Mob(enderman));
        assert_eq!(parse("mob:enderman"), Icon::Mob(enderman));
        assert_eq!(parse("mob:minecraft:arrow"), Icon::Default);
        assert_eq!(parse("mob:othermod:enderman"), Icon::Default);
        assert_eq!(parse("whatever"), Icon::Default);
        for icon in [Icon::Default, Icon::Shape(Shape::Skull), Icon::Item("minecraft:stone".into()), Icon::Mob(enderman)] {
            assert_eq!(parse(&id_of(&icon)), icon);
        }
        for (name, _, shape) in SHAPES {
            assert_eq!(id_of(&Icon::Shape(shape)), format!("shape:{name}"));
        }
        let mut wp = Waypoint::new("x", [0, 0, 0], 0xFFFFFF, "minecraft:overworld");
        assert_eq!(look(&wp), Icon::Shape(Shape::Diamond));
        wp.kind = Kind::Death;
        assert_eq!(look(&wp), Icon::Shape(Shape::Cross));
        wp.icon = "shape:star".into();
        assert_eq!(look(&wp), Icon::Shape(Shape::Star));
    }

    fn choices() -> Vec<Choice> {
        let c = |id: &str, name: &str| Choice::new(id.into(), name.into(), What::Default);
        vec![
            c("", "Default"),
            c("shape:home", "Home"),
            c("mob:minecraft:enderman", "Enderman"),
            c("mob:minecraft:ender_dragon", "Ender Dragon"),
            c("mob:minecraft:zombie", "Zombie"),
            c("item:minecraft:ender_pearl", "Ender Pearl"),
            c("item:minecraft:ender_eye", "Eye of Ender"),
            c("item:minecraft:end_stone", "End Stone"),
            c("item:minecraft:diamond_pickaxe", "Diamond Pickaxe"),
        ]
    }

    #[test]
    fn search_finds_by_name_and_id_in_any_case() {
        let all = choices();
        let ids = |q: &str| search(&all, q).iter().map(|i| all[*i].id.clone()).collect::<Vec<_>>();
        assert_eq!(ids("pearl"), ["item:minecraft:ender_pearl"]);
        assert_eq!(
            ids("ENDER"),
            ["mob:minecraft:enderman", "mob:minecraft:ender_dragon", "item:minecraft:ender_pearl", "item:minecraft:ender_eye"]
        );
        assert_eq!(ids("ender_eye"), ["item:minecraft:ender_eye"]);
        assert_eq!(ids("diamond pick"), ["item:minecraft:diamond_pickaxe"]);
        assert_eq!(ids("  "), all.iter().map(|c| c.id.clone()).collect::<Vec<_>>());
        assert!(ids("nothing like it").is_empty());
    }

    #[test]
    fn keys_edit_the_search() {
        let mut p = Picker::new("");
        p.choices = choices();
        p.type_char('p');
        p.type_char('e');
        assert_eq!(p.query(), "pe");
        assert_eq!(p.key("key.keyboard.backspace"), Reply::Handled);
        assert_eq!(p.query(), "p");
        assert_eq!(p.key("key.keyboard.backspace"), Reply::Handled);
        assert_eq!(p.key("key.keyboard.backspace"), Reply::Ignored);
        assert_eq!(p.key("key.keyboard.escape"), Reply::Ignored);
        p.set_query("pearl");
        p.shown = search(&p.choices, p.query());
        assert_eq!(p.key("key.keyboard.enter"), Reply::Chose("item:minecraft:ender_pearl".into()));
    }

    fn ascii(pat: &Pattern, style: Style) -> Vec<String> {
        let ([w, h], runs) = paint(pat, style);
        let mut rows = vec![vec!['.'; w as usize]; h as usize];
        for (x, y, n, c) in runs {
            let ch = match c {
                0xFF00_0000 => 'o',
                0xFFFF_FFFF => 'W',
                _ => '#',
            };
            for i in 0..n {
                rows[y as usize][(x + i) as usize] = ch;
            }
        }
        rows.into_iter().map(|r| r.into_iter().collect()).collect()
    }

    #[test]
    fn shapes_are_symmetric_and_fit_their_room() {
        let style = Style { rgb: 0x55FF55, ..Style::default() };
        // Every shape that should be is mirror symmetric, at every size.
        for (name, _, shape) in SHAPES {
            for side in [5, 7, 9, 11, 13] {
                let pat = pattern(shape, side);
                assert_eq!(pat.cells.len(), pat.w * pat.h, "{name}");
                if matches!(shape, Shape::Flag | Shape::Pickaxe | Shape::Portal) {
                    continue;
                }
                for row in ascii(&pat, style) {
                    assert_eq!(row, row.chars().rev().collect::<String>(), "{name} {side}");
                }
            }
        }
        for (room, t) in [(10, 1), (12, 1), (14, 1), (16, 1), (15, 2), (21, 2), (20, 2), (28, 2)] {
            for (_, _, shape) in SHAPES {
                let side = fit(shape, room, t);
                if side > 7 {
                    assert!((side as i32 + 2) * t <= room, "{shape:?} {room} {t}");
                }
            }
        }
        // The diamond at GUI scale 2, half 2.5: eleven across, a rim round it.
        assert_eq!(fit(Shape::Diamond, 14, 1), 11);
        let rows = ascii(&pattern(Shape::Diamond, 7), style);
        assert_eq!(rows[0], "....o....");
        assert_eq!(rows[4], "o#######o");
        // Tracked: a white ring round the rim.
        let rows = ascii(&pattern(Shape::Diamond, 7), Style { tracked: true, ..style });
        assert_eq!(rows.len(), 11);
        assert!(rows[0].contains('W') && rows[5].starts_with('W'));
        // Temporary: hollow.
        let rows = ascii(&pattern(Shape::Diamond, 9), Style { hollow: true, ..style });
        assert_eq!(rows[5].chars().nth(5), Some('.'));
        // Small too: a texel of colour round a dark middle.
        let rows = ascii(&pattern(Shape::Diamond, 7), Style { hollow: true, ..style });
        assert_eq!(rows[4], "o#ooooo#o");
        assert_eq!(texel(1.0), 1);
        assert_eq!(texel(2.0), 1);
        assert_eq!(texel(3.0), 2);
        assert_eq!(texel(4.0), 2);
    }
}
