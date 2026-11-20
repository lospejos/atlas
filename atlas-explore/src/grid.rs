//! Chunk / region grid: which lines to draw for a map view. Pure geometry, no seed.

/// What a grid line separates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum LineKind {
    /// A chunk border (every 16 blocks).
    Chunk,
    /// A region (`.mca`, 32×32 chunks) border, every 512 blocks: draw thicker / brighter.
    Region,
}

/// One line of the grid, in world block coordinates.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GridLine {
    /// The line x = `x` (runs north–south over the whole view).
    X { x: i32, kind: LineKind },
    /// The line z = `z` (runs east–west).
    Z { z: i32, kind: LineKind },
}

/// Below this many screen pixels per chunk, chunk lines are skipped (they would just grey the
/// map); region lines are kept down to [`MIN_PX_PER_REGION`].
pub const MIN_PX_PER_CHUNK: f32 = 6.0;
/// Below this many pixels per region, no grid at all.
pub const MIN_PX_PER_REGION: f32 = 8.0;

/// Calls `out` for each grid line crossing the view `[min, max]` (blocks, inclusive, x/z), given
/// the map scale in screen pixels per block. Lines are on block borders: x = 16·k is the west
/// edge of chunk k. A line that is both a chunk and region border is reported once, as `Region`.
pub fn grid_lines(min: [i32; 2], max: [i32; 2], px_per_block: f32, mut out: impl FnMut(GridLine)) {
    let chunks = px_per_block * 16.0 >= MIN_PX_PER_CHUNK;
    let regions = px_per_block * 512.0 >= MIN_PX_PER_REGION;
    if !regions {
        return;
    }
    let step = if chunks { 16 } else { 512 };
    let first = |v: i32| v.div_euclid(step) * step + if v.rem_euclid(step) == 0 { 0 } else { step };
    let kind = |v: i32| {
        if v.rem_euclid(512) == 0 {
            LineKind::Region
        } else {
            LineKind::Chunk
        }
    };
    let mut x = first(min[0]);
    while x <= max[0] {
        out(GridLine::X { x, kind: kind(x) });
        x += step;
    }
    let mut z = first(min[1]);
    while z <= max[1] {
        out(GridLine::Z { z, kind: kind(z) });
        z += step;
    }
}

/// The chunk holding block (`x`, `z`) and its block bounds `[min_x, min_z, max_x, max_z]`
/// (inclusive) — for a hover readout "Chunk 3, -7 (48..63, -112..-97)".
pub fn chunk_at(x: i32, z: i32) -> ([i32; 2], [i32; 4]) {
    let (cx, cz) = (x >> 4, z >> 4);
    ([cx, cz], [cx * 16, cz * 16, cx * 16 + 15, cz * 16 + 15])
}

/// The region (`r.<x>.<z>.mca`) holding chunk (`cx`, `cz`).
pub fn region_of_chunk(cx: i32, cz: i32) -> [i32; 2] {
    [cx >> 5, cz >> 5]
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec::Vec;

    #[test]
    fn lines_on_chunk_borders() {
        let mut lines = Vec::new();
        grid_lines([-20, 500], [40, 530], 2.0, |l| lines.push(l));
        assert_eq!(
            lines,
            [
                GridLine::X {
                    x: -16,
                    kind: LineKind::Chunk
                },
                GridLine::X {
                    x: 0,
                    kind: LineKind::Region
                },
                GridLine::X {
                    x: 16,
                    kind: LineKind::Chunk
                },
                GridLine::X {
                    x: 32,
                    kind: LineKind::Chunk
                },
                GridLine::Z {
                    z: 512,
                    kind: LineKind::Region
                },
                GridLine::Z {
                    z: 528,
                    kind: LineKind::Chunk
                },
            ]
        );
    }

    #[test]
    fn zoomed_out_keeps_regions_only_then_nothing() {
        let mut lines = Vec::new();
        grid_lines([-1000, -1000], [1000, 1000], 0.1, |l| lines.push(l));
        assert!(lines.iter().all(|l| matches!(
            l,
            GridLine::X {
                kind: LineKind::Region,
                ..
            } | GridLine::Z {
                kind: LineKind::Region,
                ..
            }
        )));
        assert_eq!(lines.len(), 6); // x and z: -512, 0, 512
        let mut n = 0;
        grid_lines([-1000, -1000], [1000, 1000], 0.001, |_| n += 1);
        assert_eq!(n, 0);
    }

    #[test]
    fn chunk_and_region_lookup() {
        assert_eq!(chunk_at(-1, 17), ([-1, 1], [-16, 16, -1, 31]));
        assert_eq!(region_of_chunk(-1, 32), [-1, 1]);
    }
}
