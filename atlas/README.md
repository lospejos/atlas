# Atlas (`atlas`) 0.3.1

A client-side wasm mod for Horizon: a minimap, a full-screen world map of everything you have
explored (surface, cave layers, the Nether under and over its roof), an entity radar, waypoints
you can edit, share and link through portals, a PNG export, and overlays worked out from the seed
(chunk grid, slime chunks, structures). API only: no `[[inject]]`, no native code.

It was `my_map` ("Map & Waypoints") up to 0.2.0. Its files are the same: copy
`horizon-mod-data/my_map/` to `horizon-mod-data/atlas/` and the explored map and the waypoints are
there (see "Moving from my_map").

## What it does

- **Minimap.** Square, north up, top right by default (unchanged from 0.2). Options: round, turning
  with you, any of the four corners, size, zoom, a second line with the Nether / Overworld
  coordinates. Terrain in map colours with height and water-depth shading, your arrow, waypoints
  (on the edge when they are off the map), entity dots, structure markers.
- **World map (M).** Drag to move, wheel to zoom about the cursor (unchanged: 1.15x a notch, eased),
  **Space** slides back to you in a quarter of a second. The bottom lines show your position and
  what is under the cursor: X Z Y, the **biome**, and the matching **Nether / Overworld**
  position (÷8 / ×8).
- **Dragging** keeps the block you grabbed exactly under the cursor on every frame, and glides on
  for about a quarter of a second when you let go in motion (a click stops it).
- **You** are vanilla's map marker (the white pointer of a map item), turned as you look.
- **Layers.** Switching is immediate: the image of the layer left stays up until the new one is
  painted whole from what is stored (measured: in the slot one frame after the change). *Auto* shows the cave layer at your height while you are under ground (a roof of
  blocks over you and little sky light) or under the Nether's roof, else the surface. *Surface* and
  *Cave* force one. In the Nether the surface is the **top of the roof**, with whatever is built on
  it. Each layer is stored on its own, per dimension.
- **Bar on the world map** (top left): Auto / Surface (Roof) / Cave, four radar dots (hostile,
  animals, players, items), Biomes, Grid, Slime, Structures, Filter, PNG. What the cursor is on is
  explained bottom right. The same switches are in Options > Mods > Atlas.
- **Radar.** Icons by default: a mob's **head** where the game has a head item (zombie, creeper,
  skeleton, wither skeleton, piglin, dragon, player), else its **spawn egg**, on a thin ring (red
  hostile, green other, white player); a dropped item is its own icon. The world map names the one
  under the cursor, beside it. "Radar style: Dots" brings the plain dots back. Only what the
  server shows your client.
- **Waypoints.** Right-click the map or press J: the bubble asks for a name and a colour. **J in
  the world puts the waypoint on the block the crosshair is on** (up to 256 blocks away) and the
  bubble points at that block; with nothing aimed at it is a plain box that says "Where you stand". Click a
  marker, or right-click a row of the list: the same bubble with **Move here, Hide / Show,
  Track / Untrack, Link, Share, Delete**; typing renames, a swatch recolours (Enter keeps the name).
  The list is this dimension's nearest first, then the other dimensions' greyed with their
  dimension. Click a row to slide there, its `x` deletes.
  - **Link** creates the matching waypoint in the Nether (÷8) or the Overworld (×8): portal planning.
  - **Share** sends `[Atlas] name @ x y z dimension` to chat. Such a line from anyone gets an
    `[Import]` to click (it runs `/atlas import`), a line on the HUD, and a row in the list (Add / x).
  - **Track** points the **direction arrow** at the top of the screen to it, with its distance
    (setting "Direction arrow", off until you track something).
  - **Spawn** and **Bed** waypoints are kept automatically (settings; single player and LAN host,
    see limits). **Death** as before.
- **Biome colours** (on by default): grass, leaves and water in the colour the game tints them
  with where they are, instead of a map item's one green and one blue.
- **PNG**: writes everything explored on the shown layer of this dimension to
  `horizon-mod-data/atlas/export/<world>-<dimension>-<layer>.png` in the game folder; the path is
  shown on the map and in chat.
- **Explore overlays** (from `mods/atlas-explore`): chunk and region **grid**; **slime chunks**
  (exact from the seed); **structures**. In single player / as LAN host the seed is read by the
  mod's half in the integrated server and every candidate is checked against the world: solid
  markers are real. On someone else's server type the seed in **Filter**: slime chunks are exact,
  structures are *possible* (hollow markers: the biome is not checked), strongholds *about here*
  (ring of 128 blocks). Filter also has a switch per structure type.

## Keys (rebindable in Options > Controls)

| Key | Does |
|---|---|
| **N** | minimap on / off |
| **M** | world map |
| **J** | new waypoint where you stand |

On the world map: drag, wheel, right-click (new waypoint), click a marker (edit), Space (to you),
Esc or M (close). In the bubble: Enter saves, Esc cancels, Tab next colour, Delete deletes.

## Settings (Options > Mods > Atlas)

Show minimap, size (48-200), zoom (4x-1/4x), corner, round, turns with you, coordinates,
Nether / Overworld coordinates (off), map layer (Auto), biome colours (on), radar hostile /
animals / players / items (on), radar style (heads and icons / dots), beams, labels, names only on hover, death / spawn / bed waypoints
(on), direction arrow (off). The explore overlays and the structure filter are kept in
`horizon-mod-data/atlas/explore.txt`, a typed seed in `<world>/seed.txt`.

## Files (`horizon-mod-data/atlas/` in the game folder)

- `sp/w_<save folder>_/` or `mp/w_<server address>_/` per world, in it:
  - `waypoints.txt`: one a line, tab separated: x, y, z, colour, flags (1 death, 2 hidden, 4 spawn,
    8 bed, 16 tracked), dimension, name. 0.2's file reads as it is.
  - `legend.txt`: what the biome and tint numbers in the regions mean.
  - `<dimension>/r.<x>.<z>.bin`: the surface, 512 x 512 blocks a file; `<dimension>/cave.<n>/…`:
    the cave layer of heights 16n-16 to 16n+24.
  - Region format `MMR2` (0.3): colour, depth, height, then a tint plane and a biome per 4 x 4
    cell. `MMR1` files (0.1, 0.2) are read and written back as `MMR2` when they change; 0.2 cannot
    read `MMR2`, so going back to 0.2 means going back to a copy.
- `export/*.png`, `explore.txt`.

### Moving from my_map

Copy `horizon-mod-data/my_map` to `horizon-mod-data/atlas`, put `mod.atlas:true` in
`horizon-mods.txt` instead of `mod.my_map:true`. Two things to know:

- **The first time a world opens the game says "Missing mods: 1 (my_map)"** and warns about removed
  blocks. The mod never added a block; "Back Up and Open" is the safe choice. Seen in a
  test.
- Settings are kept per mod id: the minimap size, zoom and switches start from their defaults.

## How it works

- **Scanning** (`src/scan.rs`). Polls loaded chunks, nearest first. Surface: the heightmap, then
  down each column to the first block with a map colour. Cave layer: from the top of the band the
  player is in (bands of 16, held for 3 blocks past a border), down through rock two blocks at a
  time to the first opening (air or a fluid), then to its floor. The biome is read per 4 x 4 cell,
  the tint the game gives grass, plants and water once per cell and block state.
- **The image** (`src/mapview.rs`). One 1024 x 1024 slot in the GUI atlas. Uploading into it costs
  the game about 18 ns a texel (measured: 256² 1.1 ms, 512² 4.5 ms, 1024x512 9.5 ms, 1024² 19.5 ms),
  painting it here 2 ms for 1024². **So the image is never uploaded while the view is being panned.**
  Every frame draws the image it has exactly where the view is; what a long pan uncovers is filled
  with a coarse mosaic straight from the store, and the image is painted with a wide margin (160
  texels where the slot has room) the moment the view rests.
- **What was wrong with dragging in 0.2.** It repainted and uploaded the whole image every time
  the view left a 48-texel margin, in the frame that needed it. Replayed in a test build with the
  same 1.6 s drag: 0.2's painting made 73 uploads of 338,000 texels (about 6 ms each) and 81 frames
  over 3 ms at a 1280 x 720 GUI; 0.3 makes none. 0.2 also moved the view by adding up the drag
  events' deltas; those add up exactly (checked), so that was not the fault, but 0.3 places the
  grabbed block under the cursor directly, at each event and each frame.
- **Round / turning minimap** (`src/minimap.rs`). Round and north-up is the sprite drawn in rows
  as wide as the circle. Turning, nothing in the HUD can rotate a sprite, so the view is rotated
  here and uploaded each frame it changes (144 x 144 texels for a 96 px map).
- **Server half.** A client mod's server hooks run in the integrated server of the same process:
  that is where the world spawn, the respawn point, the seed and the structure checks come from.

## Performance (off-screen test machine, 1280 x 720, vsync off; test build's own timers)

| | |
|---|---|
| Frame hook (minimap, map, radar drawing aside) | 0.006 ms average, 1 ms worst |
| Tick hook while a freshly entered area is read (surface + cave) | 1.2 to 1.9 ms average over 200 ticks, 7 to 9 ms worst; about 0.45 µs a host call, 3,500 to 6,000 calls a tick |
| Tick hook once the area is read | 0.5 ms average |
| A cave chunk | about 5,000 calls (up to 12,000): one a tick; the 9 around you again every 5 s |
| Radar | about 160 calls a tick for 80 entities (0.03 ms) |
| Saving | one region at a time, 1 to 6 ms, every 2.5 s while something changed |
| CPU a frame, minimap on / off (`horizon-ctl bench`) | 0.343 / 0.321 ms |
| CPU a frame, world map open | 0.41 ms |
| Turning minimap while turning | +0.32 ms a frame (north-up: +0) |
| Structure checks | one lookup a server tick at most, none in the first 10 s or while ticks run over 12 ms |

## Known limits

- **J at a distance** uses the client's own raycast first, then a ray the integrated server
  answers. In the test the client's raycast found ground 8 blocks away but not a one-block pillar
  30 blocks away; the server's did. So on someone else's server a far or thin target may not be
  found, and J falls back to the plain "Where you stand" box.
- **Radar icons**: a dropped item's icon is found from its English name (the client is told the
  name, not the id): other languages and odd names get the yellow dot. Player heads are the
  default head, not the player's skin. Mobs without a spawn egg get a dot. At most 64 icons are
  drawn, the rest as dots. Icons cost about 0.05 ms a frame more than dots with 11 entities.
- **A cave layer never visited** still fills in as it is read (two chunks a tick near you,
  about a second for the minimap's area): only what is stored can be shown at once.

- **A hook call that runs past about 100 ms (wall clock) is cut off by the game and the whole mod
  is stopped until restart.** Everything here is sliced to stay far below (exports, saves on
  leaving, lookups). It still happened once in about 17 test game runs, on the render thread during a
  560 ms stall of the whole game on a loaded machine: not reproduced, cause outside the mod not
  proven. If the map ever goes dead, the log line is "wasm mod libatlas.so stopped its group".
- **The image slot is shared by all mods** (one image for all): two mods painting it flicker.
- **Cave layer.** One layer is shown: the band you are in. Rock is black; a column whose first
  opening is a flooded cave shows water. Reading is limited to 5 chunks around you. In a cave the
  entities shown are those within 14 blocks of your height.
- **Auto** needs a second to switch (so a doorway doesn't flip it); in a house with windows it
  stays on the surface.
- **Radar**: players' names only on the world map (the minimap has no cursor); other players were
  not available to test in single player, their dots use the same path as mobs.
- **Spawn / Bed waypoints**: the client is not told either. In single player and as LAN host the
  server half reads them. On a server there is no Spawn waypoint, and Bed is set where you stand
  when chat says "Respawn point set" (English only).
- **[Import] in chat** runs `/atlas import`; the command was tested, the click itself could not be
  driven by the test tool.
- **PNG**: at most 40 regions (20,480 blocks) across and 8192 px a side (a larger area at 2 or 4
  blocks a pixel), under 64 MiB. Its edges are whole regions, so there is a transparent margin.
- **Turning minimap** uploads a small image every frame the view turns (see the table).
- **Structures on other servers** are candidates from the seed alone: most rarer ones are not
  real (wrong biome). Solid markers there are only the sets that need no biome (ruined portals).
- **Biome colours** are stored as one of 255 tints per world (rounded to 4 a channel, nearest
  when full). Columns explored with 0.2 have no tint until they are read again.
- Typing in the bubble and the seed field was checked through the test build's key path
  (`horizon-ctl` sends no text to a mod's screen); the real path is `screen_char_typed_pre`.

## Building and testing

```sh
cargo horizon-mod build --wasm
cargo horizon-mod check target/horizon-mod/atlas.hmod
cargo test     # 32 tests: formats, 0.2 files, drag, glide, slide, PNG, …
```

`TEST` in `src/lib.rs` (off in the shipped build) turns on `src/probe.rs`: it writes `probe.log`
(what the reads answer, upload and paint times, hook times, a drag followed frame by frame) and
adds keys on the world map for what the test tool cannot reach (K: a drag made a cursor position
a frame, L: the same with 0.2's painting; R/T/C/O/A: minimap round / turning / corner / second
line / arrow; G/S/X/F/U: grid, slime, structures, filter, "as on a remote server").

## Adding an overlay

`src/overlays.rs`: implement `Overlay` (id, label, tip, `tick` with the host, `markers` for an
area) and push it in `registered()`. It gets a switch in the bar, a setting `overlay_<id>`, and
is drawn between the terrain and the entity dots on both maps. The explore overlays are drawn at
the same place (`src/explore.rs`).
