# World Atlas (`world_atlas`) 0.4.1

A client-side wasm mod for Horizon: a minimap, a full-screen world map of everything you have
explored (surface, cave layers, the Nether under and over its roof), an entity radar, waypoints
you can edit, share and link through portals, a PNG export, and overlays worked out from the seed
(chunk grid, slime chunks, structures). API only: no `[[inject]]`, no native code.

It was `my_map` ("Map & Waypoints") up to 0.2.0. Its files are the same: copy
`horizon-mod-data/my_map/` to `horizon-mod-data/world_atlas/` and the explored map and the waypoints are
there (see "Moving from my_map").

## What it does

- **Minimap.** Square, north up, top right by default (unchanged from 0.2). Options: round, turning
  with you, any of the four corners, size, zoom, a second line with the Nether / Overworld
  coordinates. Terrain in map colours with height and water-depth shading, your arrow, waypoints
  (on the edge when they are off the map, wholly inside it), entity faces, structure markers.
  **[ / ]** zoom it a step in or out (a short "2x" hint shows, the step is kept as the setting);
  **hold Z** to enlarge it (2.2x, as far as the screen has room) while held. It hides itself
  under F3 when F3's text is in its corner, and in menus and chat (settings).
- **Compass bar** (setting, off by default): a slim strip at the top centre, as wide as the
  hotbar, with N / E / S / W and ticks sliding past as you turn and this dimension's waypoints
  at their bearings (each as its icon; the nearest wins where two would overlap). The tracked
  waypoint is a little larger with its name and distance under it in the labels' small type; out
  of view it becomes an arrow at the bar's end to turn towards. Boss bars move down under it. The
  enlarged minimap fades it out when it would reach over it.
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
- **Bar on the world map** (top left): Auto / Surface (Roof) / Cave, four radar switches (a zombie's
  face for hostile mobs, a cow's for passive ones, the player's for players, an item for items;
  dimmed while off), Biomes, Grid, Slime, Structures, Filter, PNG. What the cursor is on is
  explained bottom right. The same switches are in Options > Mods > Atlas.
- **Radar.** Each mob is its **face**: the front of its head, flat, as the game's own texture has
  it (92 vanilla mobs and the player's default face), on a one-pixel dark edge. A dropped item is
  its own icon. The world map names whatever is under the cursor, face or dot, beside it (setting
  "Radar names on hover"). "Radar style: Dots" brings the plain dots back. Only what the server
  shows your client. **No texture is shipped**: `assets/atlas/atlases/radar.json` is an atlas
  definition that tells the game which rectangle of which of its entity textures to cut out
  (`unstitch`), registered through `register_texture_atlases`; a resource pack's mobs get their
  own faces.
- **Waypoints.** Right-click the map or press J: the bubble asks for a name, the exact **X Y Z**
  (filled in; Tab moves between the fields, a field that is no number turns red and is not
  saved), a colour, an **icon**, and two switches: **Beam** (on) and **Temporary** (off). **J in
  the world puts the waypoint on the block the crosshair is on** (up to 256 blocks away) and the
  bubble points at that block; with nothing aimed at it is a plain box that says "Where you stand".
  **Shift+J** opens it with Temporary on. Click a marker, or right-click a row of the list: the
  same bubble with **Move here, Hide / Show, Track / Untrack, Link, Share, Delete**; typing
  renames, a swatch, a switch or an icon applies at once (Enter keeps the name and place).
  The list is this dimension's nearest first, then the other dimensions' greyed with their
  dimension; a **search box** over it filters by name. Click a row to slide there, its beam glyph
  switches that waypoint's beam, its `x` deletes.
  - **Icons.** The square at the end of the colours shows the waypoint's icon; a click opens the
    **icon picker** beside the bubble: a search box over a grid of pixel shapes (home, star,
    flag, portal, skull, pin, heart, pickaxe, cross; drawn in the waypoint's colour), the 92 mob
    faces and every item of the game. Type to search ("ender" finds the enderman, the dragon,
    pearls, eyes...), wheel or arrows to scroll, click (or Enter for the first found) to choose,
    Esc closes the picker. The default is a diamond, a cross for a death. Shapes are this mod's
    own patterns, items and faces are the game's own pictures drawn at runtime. A face whose look
    is partly in a glowing layer (the enderman's purple eyes, the copper golem's) gets that layer
    drawn over it. Every view (map, minimap, list, labels, compass) draws a waypoint the same way:
    a tracked one has a white ring, a temporary one is an outline round a dark middle (a face's
    rim and an item's dotted edge in its colour), a hidden one or one in another dimension dimmed.
  - **Beams** stand at every waypoint (switch per waypoint, and all at once on the map's bar). They
    fade out near you ("Hide beam within", 16 blocks) and past "Max beam distance".
  - **Temporary** waypoints delete themselves when you get within 3 blocks.
  - **Deaths**: the last 1, 3 or 5 deaths are kept ("Death", "Death 2", ...), also with the
    `immediate_respawn` game rule (the client never sees its health at 0 then: the death is
    added on the respawn, where you last stood). A bubble or a click holds its waypoint by an id
    the game session gives it, not by its place in the list: a death or a reached temporary
    waypoint while a bubble is open never makes it save into, or delete, another one.
  - **`/atlas add <name> <x> <y> <z> [dimension]`** adds one from chat (`~` for your own
    coordinate, `~5` relative; a name of several words, or in double quotes; dimension
    `overworld`, `nether`, `end` or an id).
  - **Link** creates the matching waypoint in the Nether (÷8) or the Overworld (×8): portal planning.
  - **Share** sends `[Atlas] name @ x y z dimension` to chat. Such a line from anyone gets an
    `[Import]` to click (it adds that line's waypoint alone: `/atlas import <number>`), a line on
    the HUD, and a row in the list (Add / x). Typed, `/atlas import` adds all that wait. At most 20
    wait, 5 from one sender: a newer one pushes the oldest out. A `§` (formatting code) or a
    control character never goes into a name, so a shared name cannot get you kicked; a shared
    place past the world border (30,000,000 across, 20,000 up or down) is not taken.
  - **Track** points the **direction arrow** at the top of the screen to it, with its distance
    (setting "Direction arrow", off until you track something).
  - **Spawn** and **Bed** waypoints are kept automatically (settings; single player and LAN host,
    see limits). **Death** as before.
- **Biome colours** (on by default): grass, leaves and water in the colour the game tints them
  with where they are, instead of a map item's one green and one blue.
- **PNG**: writes everything explored on the shown layer of this dimension to
  `horizon-mod-data/world_atlas/export/<world>-<dimension>-<layer>.png` in the game folder; the path is
  shown on the map and in chat.
- **Explore overlays** (from `mods/atlas-explore`): chunk and region **grid**; **slime chunks**
  (exact from the seed); **structures**. In single player / as LAN host the seed is read by the
  mod's half in the integrated server and every candidate in a loaded chunk is checked against
  the world (one a server tick, a read that loads and builds nothing): solid markers are real.
  Strongholds are not checked anywhere: they are *about here* from the ring maths (the game's
  stronghold search places all 128 at once, which in a real world ran past a hook's 100 ms and
  stopped the mod). On someone else's server type the seed in **Filter**: slime chunks are exact,
  structures are *possible* (hollow markers: the biome is not checked), strongholds *about here*
  (ring of 128 blocks). Filter also has a switch per structure type.

## Keys (rebindable in Options > Controls)

| Key | Does |
|---|---|
| **N** | minimap on / off |
| **M** | world map |
| **J** | new waypoint (on the aimed block, else where you stand) |
| **Shift+J** | new temporary waypoint |
| **[** / **]** | minimap zoom in / out |
| **Z** (hold) | enlarge the minimap |

On the world map: drag, wheel, right-click (new waypoint), click a marker (edit), Space (to you),
Esc or M (close). In the bubble: Enter saves, Esc cancels, **Tab / Shift+Tab move between the name
and X, Y, Z**, Delete (in the name) deletes. With the icon picker open the typing goes to its
search, Esc closes the picker first.

## Settings (Options > Mods > World Atlas)

Show minimap, size (48-200), zoom (4x-1/4x), corner, round, turns with you, coordinates,
Nether / Overworld coordinates (off), hide under F3 (on), hide in menus and chat (on), map layer
(Auto), biome colours (on), radar hostile / animals / players / items (on), radar style (faces /
dots), radar names on hover (on), waypoint beams (on), hide beam within (off, 8, 16, 32, 64
blocks; 16), max beam distance (500 to 10000 blocks, unlimited), waypoint labels (on), names only
on hover (off), death waypoint (on), deaths kept (1, 3, 5; 3), spawn / bed waypoints (on),
direction arrow (off), compass bar (off). The explore overlays and the structure filter are kept in
`horizon-mod-data/world_atlas/explore.txt`, a typed seed in `<world>/seed.txt`.

## Files (`horizon-mod-data/world_atlas/` in the game folder)

- `sp/w_<save folder>_/` or `mp/w_<server address>_/` per world (every character but letters,
  digits, `.`, `-` and `_` becomes `_`; a name with characters past ASCII, `Haus ä`, gets a hash of
  the whole name after it, `w_Haus___1a2b3c4d`, so `Haus ä` and `Haus ö` are two folders, unless
  the folder 0.4 and before used, `w_Haus___`, is already there: that one is kept), in it:
  - `waypoints.txt`: one a line, tab separated: x, y, z, colour, flags (1 death, 2 hidden, 4 spawn,
    8 bed, 16 tracked, 32 beam off, 64 temporary), dimension, name, and an optional eighth column,
    the icon: `shape:<name>` (`shape:home`), `item:<item id>` (`item:minecraft:ender_pearl`) or
    `mob:<entity id>` (`mob:minecraft:enderman`); written only when set. 0.2's and 0.3's files
    read as they are. A line Atlas cannot read is written back as it was, after the waypoints.
    **Don't go back to 0.3.x after using 0.4**: 0.3 reads the icon column into the name and
    drops the flags 32 (beam off) and 64 (temporary).
  - A file that is there but cannot be read (`waypoints.txt`, `legend.txt`, a region) is never
    written over: chat says so, and for that session the waypoints (or the map, or that region)
    are not saved.
  - `legend.txt`: what the biome and tint numbers in the regions mean.
  - `<dimension>/r.<x>.<z>.bin`: the surface, 512 x 512 blocks a file; `<dimension>/cave.<n>/…`:
    the cave layer of heights 16n-16 to 16n+24.
  - Region format `MMR2` (0.3): colour, depth, height, then a tint plane and a biome per 4 x 4
    cell. `MMR1` files (0.1, 0.2) are read and written back as `MMR2` when they change; 0.2 cannot
    read `MMR2`, so going back to 0.2 means going back to a copy.
- `export/*.png`, `explore.txt`.

### Moving from Atlas 0.4.0 (`atlas`)

0.4.1 is the same mod under a new id, `world_atlas`: the registry's id `atlas` belongs to another
author's earlier mod. A mod can only reach its own folders, so World Atlas can't take the old
ones over by itself. Once, with the game closed, in the game folder:

1. Copy `horizon-mod-data/atlas/` to `horizon-mod-data/world_atlas/` (the explored map and the
   waypoints) and `horizon-mods/.horizon-mod-data/atlas/` to
   `horizon-mods/.horizon-mod-data/world_atlas/` (the settings).
2. In `horizon-mods/`, replace `atlas.hmod` (or `atlas-0.4.0.hmod`) with the new file, and in
   `horizon-mods.txt` write `mod.world_atlas:true` instead of `mod.atlas:true`.

Keys you rebound in Options > Controls go back to their defaults once. The `/atlas` command keeps
its name.

### Moving from my_map

Copy `horizon-mod-data/my_map` to `horizon-mod-data/world_atlas`, put `mod.atlas:true` in
`horizon-mods.txt` instead of `mod.my_map:true`. Two things to know:

- **The first time a world opens the game says "Missing mods: 1 (my_map)"** and warns about removed
  blocks. The mod never added a block; "Back Up and Open" is the safe choice. Seen in the hidden
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

## Performance (hidden, this PC, 1280 x 720, vsync off; test build's own timers)

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
- **Radar faces**: players get the default face, not their skin (no API gives a client the skin's
  pixels). A mod's mob, or one without an entry in the table, is a dot. Faces were picked by eye
  from 26.3's textures: a few are a profile (horse, goat) or a body part that reads better than
  the head (sniffer's nose, nautilus's shell), and camel is a guess. A dropped item's icon is found
  from its English name (the client is told the name, not the id); an item not found that way is a
  small grey tile.
- **Labels in the world** ("Base 125m") are the game's own font at a fixed small size: two
  screen pixels a font pixel on a window 900 pixels high or more, one below that, and always less
  than the GUI scale, at any distance. They are placed through the camera's own matrices, the
  ones the world and the beams are drawn with (view bobbing included; the game's `project` call
  leaves the bobbing out, which is what made 0.3.2's labels wander against their beams while
  walking), snapped to whole screen pixels so the glyphs stay sharp. Labels no longer push each
  other aside, so two close together overlap.
- **Radar faces** come from the game's entity models: the texture rectangle of the head cube's
  front face. Where the eyes are on the sides of a long head it is a profile put together from
  up to three rectangles into a squarish icon: horses, donkeys and mules (muzzle, head, top of
  the neck), camels (head and neck), goats (head under a horn), frogs (eyes over the mouth), fish.
  Markers are left out under the world map's panels, bar and status lines, and cut off at the
  minimap's edge. Slimes, magma
  cubes, the wither, wolves, bats, parrots, sniffers and nautiluses keep a rectangle chosen by
  eye because their model's head has no usable front face. Markers move by whole screen pixels,
  between the positions of the last two ticks (measured on a walking cow: steps of 1 screen
  pixel at most).
- **A cave layer never visited** still fills in as it is read (two chunks a tick near you,
  about a second for the minimap's area): only what is stored can be shown at once.

- **A hook call that runs past about 100 ms (wall clock) is cut off by the game and the whole mod
  is stopped until restart.** Everything here is sliced to stay far below (exports, saves on
  leaving, lookups). It still happened once in about 17 hidden game runs, on the render thread during a
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
  when chat says "Respawn point set" (its translation key `block.minecraft.set_spawn`, or those
  English words when a server sends plain text).
- **[Import] in chat** runs `/atlas import <number>`; the command was tested, the click itself
  could not be driven by the test tool. The number holds for this session: an older line's
  [Import] says the waypoint is not waiting any more.
- **PNG**: at most 40 regions (20,480 blocks) across and 8192 px a side (a larger area at 2 or 4
  blocks a pixel), under 64 MiB. Its edges are whole regions, so there is a transparent margin.
- **Turning minimap** uploads a small image every frame the view turns (see the table).
- **Structures on other servers** are candidates from the seed alone: most rarer ones are not
  real (wrong biome). Solid markers there are only the sets that need no biome (ruined portals).
- **Biome colours** are stored as one of 255 tints per world (rounded to 4 a channel, nearest
  when full). Columns explored with 0.2 have no tint until they are read again.
- Typing in the bubble's picker was tested on the real path (`horizon-ctl type` reaches a mod's
  screen on this Nightly); the name and X Y Z fields through the test build's key path.
- **Icons**: an item in an in-world label (and so in the compass's caption) is drawn only where a
  label's font pixel is two screen pixels (a window 900 pixels high or more); smaller, an item
  would blur, so it shows as the diamond there. Shapes at their smallest (7 texels) lose detail:
  the skull's eyes and the star's points are simplified. A face bigger than its room is drawn at
  a whole number of screen pixels a texel and may reach a pixel past a shape of the same size.
- **Compass**: the enlarged minimap fades the bar out only while it would reach over it (narrow
  windows, GUI scale 3 or more); the markers on the bar can't fade, so they go at half way.

## Building and testing

```sh
cargo horizon-mod build --wasm
cargo horizon-mod check target/horizon-mod/world_atlas.hmod
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
