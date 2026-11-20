//! The scrollbar of Atlas's lists (the waypoint list, the icon picker): a thin track with a
//! thumb that the wheel moves and the mouse drags. A press on the thumb grabs it where it was
//! pressed; a press on the track brings the thumb's middle there and grabs it, so one press can
//! jump and go on dragging. While held, the thumb follows the cursor wherever it goes.
use mod_api::DrawContext;

use crate::gfx::inside;

/// The thumb's width, GUI pixels; the track is the one pixel down its middle.
pub const WIDTH: f32 = 3.0;
/// The shortest the thumb gets.
const THUMB_MIN: f32 = 8.0;

/// A bar as it was drawn: its track (`WIDTH` wide), the first row shown, how many rows show
/// and how many there are.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Bar {
    pub track: [f32; 4],
    pub first: usize,
    pub shown: usize,
    pub total: usize,
}

impl Bar {
    /// The first row at the end.
    pub fn most(&self) -> usize {
        self.total.saturating_sub(self.shown)
    }

    /// The thumb: as long as the share of the rows that shows, on whole pixels.
    pub fn thumb(&self) -> [f32; 4] {
        let [x, y, w, h] = self.track;
        let len = (h * self.shown as f32 / self.total.max(1) as f32).round().clamp(THUMB_MIN.min(h), h);
        let most = self.most();
        let top = if most == 0 { y } else { y + ((h - len) * self.first.min(most) as f32 / most as f32).round() };
        [x, top, w, len]
    }

    /// The strip that takes the mouse: the track and `pad` pixels either side of it.
    pub fn grip(&self, pad: f32) -> [f32; 4] {
        let [x, y, w, h] = self.track;
        [x - pad, y, w + pad * 2.0, h]
    }

    /// A press at height `y` on the strip: the first row now, and where on the thumb it is held
    /// (from the thumb's top).
    pub fn press(&self, y: f32) -> (usize, f32) {
        let thumb = self.thumb();
        if y >= thumb[1] && y < thumb[1] + thumb[3] {
            return (self.first.min(self.most()), y - thumb[1]);
        }
        let grab = (thumb[3] / 2.0).floor();
        (self.first_at(y, grab), grab)
    }

    /// The first row that puts the thumb's top at `y - grab`.
    pub fn first_at(&self, y: f32, grab: f32) -> usize {
        let room = self.track[3] - self.thumb()[3];
        let most = self.most();
        if room <= 0.0 || most == 0 {
            return 0;
        }
        let k = ((y - grab - self.track[1]) / room).clamp(0.0, 1.0);
        ((k * most as f32).round() as usize).min(most)
    }

    /// Draws the track and the thumb: lighter under the cursor (`hot`), white while `held`.
    pub fn draw(&self, draw: &mut dyn DrawContext, mouse: [f32; 2], held: bool, pad: f32) {
        let [x, y, w, h] = self.track;
        draw.fill([x + (w / 2.0).floor(), y, 1.0, h], 0x40FF_FFFF);
        let thumb = self.thumb();
        let hot = inside(self.grip(pad), mouse);
        let color = if held {
            0xFFFF_FFFF
        } else if hot && inside([thumb[0] - pad, thumb[1], thumb[2] + pad * 2.0, thumb[3]], mouse) {
            0xFFE0_E0F0
        } else if hot {
            0xFFB8_B8C8
        } else {
            0xFF90_90A0
        };
        draw.fill(thumb, color);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn bar(first: usize) -> Bar {
        // 100 pixels of track, 10 of 30 rows shown: a 33-pixel thumb with 67 to move in.
        Bar { track: [0.0, 10.0, WIDTH, 100.0], first, shown: 10, total: 30 }
    }

    #[test]
    fn the_thumb_is_the_share_shown_and_moves_with_the_rows() {
        assert_eq!(bar(0).thumb(), [0.0, 10.0, WIDTH, 33.0]);
        assert_eq!(bar(20).thumb(), [0.0, 77.0, WIDTH, 33.0]);
        assert_eq!(bar(10).thumb()[1], 44.0);
        // A long list keeps a thumb to hold.
        let long = Bar { total: 5000, ..bar(0) };
        assert_eq!(long.thumb()[3], THUMB_MIN);
    }

    #[test]
    fn a_held_thumb_follows_the_cursor_and_stops_at_the_ends() {
        let b = bar(0);
        // Grabbed 5 pixels into the thumb: the thumb's top stays 5 above the cursor.
        let (first, grab) = b.press(15.0);
        assert_eq!((first, grab), (0, 5.0));
        assert_eq!(b.first_at(15.0 + 67.0, grab), 20);
        assert_eq!(b.first_at(15.0 + 33.5, grab), 10);
        // Far past either end, outside the bar.
        assert_eq!(b.first_at(-500.0, grab), 0);
        assert_eq!(b.first_at(900.0, grab), 20);
    }

    #[test]
    fn a_press_on_the_track_brings_the_thumb_there() {
        let b = bar(0);
        // At the bottom of the track: the end, held by the thumb's middle.
        let (first, grab) = b.press(109.0);
        assert_eq!((first, grab), (20, 16.0));
        let moved = Bar { first, ..b };
        assert!(inside(moved.thumb(), [1.0, 109.0]));
        // Nothing to scroll: always the top.
        let all = Bar { total: 5, ..b };
        assert_eq!(all.press(80.0).0, 0);
    }
}
