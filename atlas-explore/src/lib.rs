//! # atlas-explore
//!
//! The "built-in Chunkbase" logic for the Atlas map, with no mod-API dependency:
//!
//! * [`slime`]: slime chunks, exact from the seed.
//! * [`structures`]: where each vanilla 26.3 structure set may start (random-spread placement,
//!   frequency reduction, exclusion zones), exact positions, biome not checked.
//! * [`stronghold`]: the 128 stronghold ring guesses (± a few chunks without biomes).
//! * [`verify`]: the singleplayer / LAN-host plan that turns candidates into exact structures with
//!   the server's `level_find_nearest_structure` (radius 0, one candidate per call).
//! * [`grid`]: chunk and region lines for the map view.
//!
//! Reference implementation: Horizon's worldgen (`horizon-world/src/worldgen/structure/start.rs`,
//! `stronghold.rs`, `horizon-core/src/random*`), itself a port of vanilla Java. `no_std` + `alloc`.
#![no_std]

extern crate alloc;
#[cfg(test)]
extern crate std;

pub mod grid;
pub mod java_random;
pub mod slime;
pub mod stronghold;
pub mod structures;
pub mod verify;

pub use slime::is_slime_chunk;
pub use structures::{Candidate, Certainty, Dimension, SetId};
pub use verify::{Budget, Lookup, Marker, Status, Verifier, seed_markers};
