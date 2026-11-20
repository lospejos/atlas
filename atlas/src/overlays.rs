//! The hook point for extra marker sets on the maps (structures, slime chunks, anything a
//! separate module works out): a set gives its markers for an area, the maps draw them between
//! the terrain and the entity dots, and the world map's bar gets a switch per set.
//!
//! To add one: implement [`Overlay`] and push it in [`registered`]. Nothing else changes.

/// One thing an overlay puts on the map.
#[derive(Clone, Debug, PartialEq)]
pub struct OverlayMarker {
    /// Block `x`, `z`.
    pub at: [f64; 2],
    pub rgb: u32,
    /// Its name, shown on the world map under the cursor.
    pub label: String,
    /// A filled square of this many blocks a side instead of a point marker (a chunk: 16).
    pub area: Option<f64>,
}

/// What an overlay is asked about.
#[allow(dead_code)] // read by the overlays, of which this build has none
pub struct OverlayQuery<'a> {
    pub dimension: &'a str,
    /// The blocks the map shows: `[min x, min z, max x, max z]`.
    pub bounds: [f64; 4],
    /// Game ticks since the world was opened, for an overlay that works in the background.
    pub tick: u64,
}

pub trait Overlay: Send {
    /// The setting key that keeps its switch (`overlay_<id>`), and the bar button's label.
    fn id(&self) -> &'static str;
    fn label(&self) -> &'static str;
    /// One line for the status bar when the cursor is on the button.
    fn tip(&self) -> &'static str;
    /// Once a client tick while the overlay is on, with the game to read from.
    fn tick(&mut self, _host: &dyn mod_api::ClientHost, _query: &OverlayQuery<'_>) {}
    /// The markers in `query.bounds`. Called once a frame for each map that is drawn while
    /// the overlay is on: keep what is expensive and answer from that.
    fn markers(&mut self, query: &OverlayQuery<'_>, out: &mut Vec<OverlayMarker>);
}

/// An overlay with its switch.
pub struct Slot {
    pub overlay: Box<dyn Overlay>,
    pub on: bool,
}

/// The overlays this build has. Empty: the structure module plugs in here.
pub fn registered() -> Vec<Slot> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use super::*;

    struct Grid;
    impl Overlay for Grid {
        fn id(&self) -> &'static str {
            "grid"
        }
        fn label(&self) -> &'static str {
            "Grid"
        }
        fn tip(&self) -> &'static str {
            "Every 64 blocks"
        }
        fn markers(&mut self, query: &OverlayQuery<'_>, out: &mut Vec<OverlayMarker>) {
            let [x0, z0, x1, z1] = query.bounds;
            let mut x = (x0 / 64.0).ceil() * 64.0;
            while x <= x1 {
                let mut z = (z0 / 64.0).ceil() * 64.0;
                while z <= z1 {
                    out.push(OverlayMarker { at: [x, z], rgb: 0xFFFFFF, label: format!("{x} {z}"), area: None });
                    z += 64.0;
                }
                x += 64.0;
            }
        }
    }

    #[test]
    fn an_overlay_answers_for_an_area() {
        let mut slot = Slot { overlay: Box::new(Grid), on: true };
        let mut out = Vec::new();
        let query = OverlayQuery { dimension: "minecraft:overworld", bounds: [-10.0, -10.0, 100.0, 70.0], tick: 0 };
        slot.overlay.markers(&query, &mut out);
        assert_eq!(out.len(), 4);
        assert_eq!(slot.overlay.id(), "grid");
    }
}
