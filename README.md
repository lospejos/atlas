# Atlas

A minimap, world map and waypoints mod for [Horizon](https://github.com/horizon-rust/horizon)
(Minecraft: Java Edition 26.3 in Rust). A WebAssembly `.hmod`: nothing to compile on your PC, and it
runs in the game's sandbox.

- **Minimap** (N): square or round, north-up or rotating, any corner; radar for mobs, players and items.
- **World map** (M): everything you explored, smooth drag and zoom, surface / cave / Nether / Nether-roof layers,
  biome colours, coordinates with the Nether ↔ Overworld equivalent under the cursor.
- **Waypoints** (J): add, rename, recolour, hide, track with a HUD arrow, beams in the world, linked Nether
  waypoints, automatic Death / Spawn / Bed, share to chat and import from chat.
- **Explore**: chunk grid, slime chunks and structures — exact in single player / on a LAN host, and
  "possible" positions from a seed you enter on other servers.
- **Export** the map as a PNG.

Details, settings and limits: [`atlas/README.md`](atlas/README.md).

## Install
Download `atlas-<version>.hmod` from [Releases](../../releases), put it in your game folder's
`horizon-mods/` (launcher → your instance's ⋮ → Open folder), add the line `mod.atlas:true` to
`horizon-mods.txt` next to `options.txt`, and restart. Needs a Horizon Nightly from 2026-11-19 or newer.

## Layout
- `atlas/` — the mod (`cargo horizon-mod build --wasm` inside it).
- `atlas-explore/` — a dependency-free library: Java `Random`, slime chunks, structure placement and
  strongholds, used by the mod's Explore layer. `cargo test` inside it.

## Licence
MIT, see [`LICENSE`](LICENSE).
