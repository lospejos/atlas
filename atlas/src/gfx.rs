//! Small drawing helpers shared by the maps, the bubble and the HUD.
use mod_api::DrawContext;

// Vanilla's tooltip colours, which the bubbles wear.
pub const BUBBLE_BG: u32 = 0xF010_0010;
pub const BUBBLE_EDGE_TOP: u32 = 0xFF50_28C8;
pub const BUBBLE_EDGE_BOTTOM: u32 = 0xFF28_1464;
/// The height of a bubble's pointer.
pub const POINTER: f32 = 6.0;
pub const PANEL_BG: u32 = 0xC010_1014;

pub fn argb(rgb: u32) -> u32 {
    0xFF00_0000 | rgb
}

pub fn inside(rect: [f32; 4], at: [f32; 2]) -> bool {
    at[0] >= rect[0] && at[0] < rect[0] + rect[2] && at[1] >= rect[1] && at[1] < rect[1] + rect[3]
}

/// A filled marker with a dark rim, centred on `at`.
pub fn marker(draw: &mut dyn DrawContext, at: [f32; 2], rgb: u32, half: f32) {
    draw.fill([at[0] - half - 1.0, at[1] - half - 1.0, half * 2.0 + 2.0, half * 2.0 + 2.0], 0xFF00_0000);
    draw.fill([at[0] - half, at[1] - half, half * 2.0, half * 2.0], argb(rgb));
}

/// A round dot with a dark rim, for an entity; a small one is a square.
pub fn dot(draw: &mut dyn DrawContext, at: [f32; 2], rgb: u32, r: f32) {
    let (x, y) = (at[0], at[1]);
    if r < 1.2 {
        draw.fill([x - r - 0.5, y - r - 0.5, r * 2.0 + 1.0, r * 2.0 + 1.0], 0xD000_0000);
        draw.fill([x - r, y - r, r * 2.0, r * 2.0], argb(rgb));
        return;
    }
    draw.fill([x - r, y - r - 0.5, r * 2.0, r * 2.0 + 1.0], 0xD000_0000);
    draw.fill([x - r - 0.5, y - r, r * 2.0 + 1.0, r * 2.0], 0xD000_0000);
    draw.fill([x - r + 0.5, y - r, r * 2.0 - 1.0, r * 2.0], argb(rgb));
    draw.fill([x - r, y - r + 0.5, r * 2.0, r * 2.0 - 1.0], argb(rgb));
}

/// An arrowhead at `at` pointing along `angle` (degrees clockwise from up on the screen).
pub fn arrow(draw: &mut dyn DrawContext, at: [f32; 2], angle: f32, size: f32, argb_fill: u32) {
    let (s, c) = angle.to_radians().sin_cos();
    // Forward and right on the screen (x right, y down).
    let (f, r) = ([s, -c], [c, s]);
    let point = |fwd: f32, right: f32, k: f32| {
        [at[0] + (f[0] * fwd + r[0] * right) * k, at[1] + (f[1] * fwd + r[1] * right) * k]
    };
    for (k, color) in [(size + 1.4, 0xFF00_0000u32), (size, argb_fill)] {
        let (tip, left, notch, right) =
            (point(1.0, 0.0, k), point(-0.8, -0.7, k), point(-0.35, 0.0, k), point(-0.8, 0.7, k));
        // Two triangles, each given in both windings: whichever way the game culls, one shows.
        draw.quad([tip, left, notch, notch], color);
        draw.quad([tip, notch, left, left], color);
        draw.quad([tip, notch, right, right], color);
        draw.quad([tip, right, notch, notch], color);
    }
}

pub fn centered(draw: &mut dyn DrawContext, text: &str, cx: f32, y: f32, color: u32) {
    let w = draw.text_width(text);
    draw.text(text, [(cx - w / 2.0).round(), y], color, true);
}

/// `text` cut to fit `width`, with an ellipsis.
pub fn fit(draw: &dyn DrawContext, text: &str, width: f32) -> String {
    if draw.text_width(text) <= width {
        return text.to_string();
    }
    let mut out: String = text.to_string();
    while !out.is_empty() && draw.text_width(&format!("{out}...")) > width {
        out.pop();
    }
    format!("{out}...")
}

/// A bubble: a tooltip-coloured panel `rect` with its corners rounded off and a pointer to
/// `tip`, below the panel or, with `tip_above`, above it.
pub fn bubble(draw: &mut dyn DrawContext, rect: [f32; 4], tip: Option<([f32; 2], bool)>) {
    let [x, y, w, h] = rect;
    draw.fill([x + 2.0, y, w - 4.0, h], BUBBLE_BG);
    draw.fill([x + 1.0, y + 1.0, 1.0, h - 2.0], BUBBLE_BG);
    draw.fill([x + w - 2.0, y + 1.0, 1.0, h - 2.0], BUBBLE_BG);
    draw.fill([x, y + 2.0, 1.0, h - 4.0], BUBBLE_BG);
    draw.fill([x + w - 1.0, y + 2.0, 1.0, h - 4.0], BUBBLE_BG);
    // Its rim, one pixel in.
    draw.fill([x + 3.0, y + 1.0, w - 6.0, 1.0], BUBBLE_EDGE_TOP);
    draw.fill([x + 3.0, y + h - 2.0, w - 6.0, 1.0], BUBBLE_EDGE_BOTTOM);
    draw.fill_gradient([x + 1.0, y + 3.0, 1.0, h - 6.0], BUBBLE_EDGE_TOP, BUBBLE_EDGE_BOTTOM);
    draw.fill_gradient([x + w - 2.0, y + 3.0, 1.0, h - 6.0], BUBBLE_EDGE_TOP, BUBBLE_EDGE_BOTTOM);
    draw.fill([x + 2.0, y + 2.0, 1.0, 1.0], BUBBLE_EDGE_TOP);
    draw.fill([x + w - 3.0, y + 2.0, 1.0, 1.0], BUBBLE_EDGE_TOP);
    draw.fill([x + 2.0, y + h - 3.0, 1.0, 1.0], BUBBLE_EDGE_BOTTOM);
    draw.fill([x + w - 3.0, y + h - 3.0, 1.0, 1.0], BUBBLE_EDGE_BOTTOM);
    // The pointer: rows narrowing to the tip, rimmed like the panel, joined to it.
    let Some((tip, above)) = tip else { return };
    let edge = if above { BUBBLE_EDGE_TOP } else { BUBBLE_EDGE_BOTTOM };
    let px = tip[0].clamp(x + POINTER + 3.0, x + w - POINTER - 3.0);
    for i in 0..POINTER as i32 {
        let half = (POINTER as i32 - i) as f32;
        let row = if above { y - 1.0 - i as f32 } else { y + h + i as f32 };
        draw.fill([px - half, row, half * 2.0 + 1.0, 1.0], BUBBLE_BG);
        draw.fill([px - half, row, 1.0, 1.0], edge);
        draw.fill([px + half, row, 1.0, 1.0], edge);
    }
    let join = if above { y + 1.0 } else { y + h - 2.0 };
    draw.fill([px - POINTER + 1.0, join, POINTER * 2.0 - 1.0, 1.0], BUBBLE_BG);
}

/// A small button: `label` in a box, lit when `active`, lighter under the cursor. Returns its
/// rectangle, `h` 12.
pub fn button(
    draw: &mut dyn DrawContext,
    at: [f32; 2],
    label: &str,
    active: bool,
    mouse: [f32; 2],
    accent: u32,
) -> [f32; 4] {
    let w = draw.text_width(label) + 8.0;
    let rect = [at[0], at[1], w, 12.0];
    let hot = inside(rect, mouse);
    draw.fill(rect, if active { 0xE034_3A4C } else if hot { 0xD02A_2C36 } else { PANEL_BG });
    if active {
        draw.fill([rect[0], rect[1] + 11.0, w, 1.0], argb(accent));
    }
    let color = if active { 0xFFFF_FFFF } else if hot { 0xFFE0_E0E0 } else { 0xFFA8_A8A8 };
    draw.text(label, [at[0] + 4.0, at[1] + 2.0], color, false);
    rect
}
