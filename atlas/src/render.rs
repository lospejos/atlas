//! Turns stored columns into pixels the way a vanilla map item does: the block's map colour in
//! one of three brightnesses, brighter where the ground rises towards the south, darker where
//! it falls, water darker with depth. With biome colours on, grass, plants and water take the
//! colour the game tints them with where they stand.
use std::ops::Range;

use crate::store::{REGION, Store, UNSEEN, VOID, WATER};

/// `MapColor`'s colours by id, 0xRRGGBB.
const MAP_COLORS: [u32; 62] = [
    0, 8368696, 16247203, 13092807, 16711680, 10526975, 10987431, 31744, 16777215, 10791096, 9923917, 7368816,
    4210943, 9402184, 16776437, 14188339, 11685080, 6724056, 15066419, 8375321, 15892389, 5000268, 10066329,
    5013401, 8339378, 3361970, 6704179, 6717235, 10040115, 1644825, 16445005, 6085589, 4882687, 55610, 8476209,
    7340544, 13742497, 10441252, 9787244, 7367818, 12223780, 6780213, 10505550, 3746083, 8874850, 5725276,
    8014168, 4996700, 4993571, 5001770, 9321518, 2430480, 12398641, 9715553, 6035741, 1474182, 3837580, 5647422,
    1356933, 6579300, 14200723, 8365974,
];
/// `MapColor.Brightness`: low, normal, high.
const BRIGHTNESS: [u32; 3] = [180, 220, 255];
/// A seen column with no coloured block.
const VOID_RGBA: [u8; 4] = [8, 8, 12, 255];
/// `MapColor.GRASS` and `MapColor.PLANT`.
pub const GRASS: u8 = 1;
pub const PLANT: u8 = 7;

/// Every colour in every brightness, as the bytes of an RGBA pixel.
pub struct Palette([[[u8; 4]; 3]; 64]);

impl Default for Palette {
    fn default() -> Self {
        let mut table = [[[0u8; 4]; 3]; 64];
        for (id, rgb) in MAP_COLORS.iter().enumerate().skip(1) {
            for (b, m) in BRIGHTNESS.iter().enumerate() {
                let ch = |shift: u32| ((rgb >> shift & 255) * m / 255) as u8;
                table[id][b] = [ch(16), ch(8), ch(0), 255];
            }
        }
        table[VOID as usize] = [VOID_RGBA; 3];
        Self(table)
    }
}

/// One biome tint as the map shows it: for grass, for plants and for water, each in the three
/// brightnesses.
pub type Tints = [[[u8; 4]; 3]; 3];

/// Which row of [`Tints`] a column of map colour `color` takes.
#[inline]
fn tint_row(color: u8) -> usize {
    match color {
        GRASS => 0,
        PLANT => 1,
        _ => 2,
    }
}

/// A biome tint as the map shows it. The game multiplies a grey texture by the tint; the
/// factors bring the result to the brightness the map's own grass, plant and water colours
/// have, so tinted and untinted columns sit side by side.
pub fn tinted(tint: [u8; 3]) -> Tints {
    let mut out = [[[0u8; 4]; 3]; 3];
    for (row, k) in [230u32, 178, 255].into_iter().enumerate() {
        for (b, m) in BRIGHTNESS.iter().enumerate() {
            let ch = |c: u8| (u32::from(c) * k / 255 * m / 255) as u8;
            out[row][b] = [ch(tint[0]), ch(tint[1]), ch(tint[2]), 255];
        }
    }
    out
}

/// `MapItem.update`'s brightness for one column: 0 low, 1 normal, 2 high.
#[inline]
fn brightness(color: u8, height: i16, north: i16, depth: u8, parity: i32, step: i32) -> usize {
    if color == WATER {
        let diff = i32::from(depth) + parity * 2;
        return if diff < 5 {
            2
        } else if diff > 9 {
            0
        } else {
            1
        };
    }
    // (height - north) * 4 / (scale + 4) + (parity - 0.5) * 0.4, in tenths.
    let diff = i32::from(height - north) * 40 / (step + 4) + parity * 4 - 2;
    if diff > 6 {
        2
    } else if diff < -6 {
        0
    } else {
        1
    }
}

/// What an image shows: the block at its top-left texel and how many blocks a texel spans.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Frame {
    pub x0: i32,
    pub z0: i32,
    /// Blocks per texel, 1 or more.
    pub step: i32,
    pub w: usize,
    pub h: usize,
}

/// Marks the regions `frame` covers as in use and queues the ones still on disk.
pub fn prepare(store: &mut Store, frame: Frame) {
    let Frame { x0, z0, step, w, h } = frame;
    let last_x = x0 + (w as i32 - 1) * step;
    let last_z = z0 + (h as i32 - 1) * step;
    for rz in (z0 - step).div_euclid(REGION)..=last_z.div_euclid(REGION) {
        for rx in x0.div_euclid(REGION)..=last_x.div_euclid(REGION) {
            store.want((rx, rz));
            store.touch((rx, rz));
        }
    }
}

/// Paints rows `rows` of `frame` into `out` (RGBA, rows top first, `frame.w * frame.h` pixels):
/// unseen columns transparent. `tints` is the store's tint table in map brightnesses, empty
/// with biome colours off.
pub fn compose_rows(
    store: &Store,
    palette: &Palette,
    tints: &[Tints],
    frame: Frame,
    rows: Range<usize>,
    out: &mut [u8],
) {
    let Frame { x0, z0, step, w, .. } = frame;
    let r = REGION;
    for pz in rows {
        let z = z0 + pz as i32 * step;
        let (rz, iz) = (z.div_euclid(r), z.rem_euclid(r));
        let row = &mut out[pz * w * 4..(pz + 1) * w * 4];
        row.fill(0);
        let parity0 = pz as i32 + x0 / step + z0 / step;
        let mut px = 0usize;
        while px < w {
            let x = x0 + px as i32 * step;
            let (rx, ix) = (x.div_euclid(r), x.rem_euclid(r));
            // The texels of this row that fall in one region.
            let run = (((r - ix + step - 1) / step) as usize).min(w - px);
            let Some(region) = store.region((rx, rz)) else {
                px += run;
                continue;
            };
            // The row to the north: in this region, or the last rows of the one above.
            let (north, niz) =
                if iz >= step { (Some(region), iz - step) } else { (store.region((rx, rz - 1)), iz - step + r) };
            let base = (iz * r + ix) as usize;
            let nbase = (niz * r + ix) as usize;
            let tint = region.tint.as_deref().filter(|_| !tints.is_empty());
            for k in 0..run {
                let i = base + k * step as usize;
                let color = region.color[i];
                if color == UNSEEN {
                    continue;
                }
                let height = region.height[i];
                let north = match north {
                    Some(n) if n.color[nbase + k * step as usize] != UNSEEN => n.height[nbase + k * step as usize],
                    _ => height,
                };
                let parity = (px as i32 + k as i32 + parity0) & 1;
                let b = brightness(color, height, north, region.depth[i], parity, step);
                let rgba = match tint.map(|t| t[i]) {
                    Some(t) if t != 0 && (t as usize) < tints.len() => &tints[t as usize][tint_row(color)][b],
                    _ => &palette.0[(color & 63) as usize][b],
                };
                row[(px + k) * 4..(px + k) * 4 + 4].copy_from_slice(rgba);
            }
            px += run;
        }
    }
}

/// The colour of column `x`, `z` in its normal brightness, 0xAARRGGBB: none when it wasn't
/// seen or its region isn't in memory.
pub fn flat(store: &Store, palette: &Palette, tints: &[Tints], x: i32, z: i32) -> Option<u32> {
    let (color, _, _) = store.column(x, z)?;
    if color == UNSEEN {
        return None;
    }
    let tint = store.tint(x, z) as usize;
    let [r, g, b, _] = if tint != 0 && tint < tints.len() {
        tints[tint][tint_row(color)][1]
    } else {
        palette.0[(color & 63) as usize][1]
    };
    Some(0xFF00_0000 | u32::from(r) << 16 | u32::from(g) << 8 | u32::from(b))
}

/// Paints all of `frame` into `out`, sized to it.
pub fn compose(store: &mut Store, palette: &Palette, tints: &[Tints], frame: Frame, out: &mut Vec<u8>) {
    out.clear();
    out.resize(frame.w * frame.h * 4, 0);
    prepare(store, frame);
    compose_rows(store, palette, tints, frame, 0..frame.h, out);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn shades_as_a_map_item_does() {
        // Flat ground dithers nothing at scale 1: both parities are normal.
        assert_eq!(brightness(1, 64, 64, 0, 0, 1), 1);
        assert_eq!(brightness(1, 64, 64, 0, 1, 1), 1);
        // One block up is high, one down is low on one parity at least.
        assert_eq!(brightness(1, 65, 64, 0, 1, 1), 2);
        assert_eq!(brightness(1, 63, 64, 0, 0, 1), 0);
        // Water: shallow bright, deep dark.
        assert_eq!(brightness(WATER, 62, 62, 1, 0, 1), 2);
        assert_eq!(brightness(WATER, 62, 62, 12, 0, 1), 0);
        assert_eq!(brightness(WATER, 62, 62, 6, 0, 1), 1);
    }

    #[test]
    fn grass_is_grass_green() {
        let p = Palette::default();
        assert_eq!(p.0[1][2], [127, 178, 56, 255]);
        assert_eq!(p.0[12][2], [64, 64, 255, 255]);
        // Plains grass (0x91BD59) lands near the map's own grass colour.
        let t = tinted([0x91, 0xBD, 0x59]);
        assert!((i32::from(t[0][2][1]) - 178).abs() < 12, "{t:?}");
    }

    #[test]
    fn rows_match_across_region_borders() {
        let mut store = Store::default();
        store.open_memory();
        // A chunk either side of the region border at x = 0, z = 0, with a step up northwards.
        for chunk in [(-1, -1), (0, -1), (-1, 0), (0, 0)] {
            let color = [1u8; 256];
            let mut height = [64i16; 256];
            if chunk.1 == 0 {
                height = [66; 256];
            }
            store.put_chunk(chunk, &color, &height, &[0; 256], None);
        }
        let palette = Palette::default();
        let frame = Frame { x0: -8, z0: -8, step: 1, w: 16, h: 16 };
        let mut out = Vec::new();
        compose(&mut store, &palette, &[], frame, &mut out);
        let pixel = |px: usize, pz: usize| out[(pz * 16 + px) * 4..(pz * 16 + px) * 4 + 4].to_vec();
        // Row z = 0 is two blocks above its northern neighbour in both regions: high.
        assert_eq!(pixel(0, 8), palette.0[1][2].to_vec());
        assert_eq!(pixel(15, 8), palette.0[1][2].to_vec());
        // Flat rows are normal, on both sides of the border.
        assert_eq!(pixel(3, 4), palette.0[1][1].to_vec());
        assert_eq!(pixel(12, 12), palette.0[1][1].to_vec());
        // The very first row has no northern neighbour stored: flat.
        assert_eq!(pixel(5, 0), palette.0[1][1].to_vec());
    }
}
