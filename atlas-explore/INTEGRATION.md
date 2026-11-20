# atlas-explore: wiring it into Atlas

`atlas-explore` is a plain `no_std` + `alloc` library with **no dependencies** and no mod-API code. Atlas
depends on it by path and draws what it returns:

```toml
# mods/atlas/Cargo.toml
[dependencies]
atlas-explore = { path = "../atlas-explore" }
```

(Proven: a scratch `side = "client"` wasm mod with exactly this kind of path dependency built with
`cargo horizon-mod build --wasm` against SDK f00125f and ran in the game.)

## What is exact, what is not

| Feature | Seed only (any server) | Singleplayer / LAN host (server check) |
|---|---|---|
| Slime chunks | **exact** (`is_slime_chunk`) | same (no need for the server) |
| Chunk / region grid | geometry only (`grid::grid_lines`) | same |
| Random-spread structures (villages, outposts, pyramids, temples, huts, igloos, monuments, mansions, ocean ruins, shipwrecks, ruined portals, ancient cities, trail ruins, trial chambers, abandoned camps, buried treasure, mineshafts, fortresses/bastions, nether fossils, end cities) | **exact positions of every possible start**, biome not checked → draw as "possible" | **exact**: each candidate is checked by the server's own generator (`Verifier`) |
| Nether fortress vs bastion | position always real; type exact when the pick is fortress, "bastion unless basalt deltas" otherwise | exact |
| Ruined portals | treated as exact (every biome has a variant; 7/7 real in tests) | exact |
| Strongholds | ring guess, real start within 8 chunks (measured worst 7) → draw a ~128-block circle | **exact** (one lookup per stronghold) |

Measured share of seed candidates that are real (seed 42, verified by the server, 48-chunk radius
around 0,0 in the Overworld, 32 in the Nether, 64 around chunk (100, 0) in the End): trial chambers
8/8, ruined portals 4/4 + 3/3 Nether, nether complexes 5/5, mineshafts 39/40, ocean ruins 10/23,
shipwrecks 4/17, trail ruins 1/6, swamp huts 1/7, villages 1/8, camps 1/7, buried treasure 3/106,
end cities 2/40, nether fossils 39/1089; ancient cities, pyramids, jungle temples, igloos,
monuments, mansions and outposts 0 of 2-16 each (wrong biome there). So on a remote server, without
biomes, most "possible" markers for rarer structures are not real: show them faded, behind a
toggle, and say why in the legend.

## API (everything Atlas needs)

```rust
use atlas_explore::{Dimension, SetId, Verifier, Budget, Lookup, Marker, Status, seed_markers, is_slime_chunk};
use atlas_explore::{grid, slime, stronghold, structures};
```

- `is_slime_chunk(seed, cx, cz) -> bool`; `slime::slime_region_bitmap(seed, rx, rz) -> [u32; 32]`
  (one 32×32-chunk region, cache it per region). Overworld only (draw nothing elsewhere).
- `grid::grid_lines(min_block[x,z], max_block[x,z], px_per_block, |GridLine| ..)`: chunk lines
  every 16 blocks while a chunk is ≥ 6 px, region lines (512) while a region is ≥ 8 px; region
  lines come as `LineKind::Region` (draw them stronger). `grid::chunk_at(x, z)` and
  `grid::region_of_chunk` for a hover readout.
- `SetId` (21 vanilla sets) and `SetId::def()` → `StructureSet { id, label, dimensions, placement,
  salt, frequency, exclusion, locate_offset, structures, lookup, certainty }`: constants from the
  26.3 jar's `worldgen/structure_set/*.json`. Use `label` in the legend and as the icon key.
- `structures::candidates_in(seed, set, min_chunk, max_chunk, |Candidate| ..)`: every possible
  start in a chunk rectangle. `Candidate { set, chunk, pos /* /locate's block */, first_pick,
  certainty }`. Cheap per spread cell; mineshafts / buried treasure (spacing 1) and nether
  fossils (spacing 2) cost one check per chunk/cell, so keep those to the visible area.
- `structures::nearest_candidate(seed, set, from_block, max_rings)`: `/locate`'s ring search over
  seed candidates (for a "nearest possible X" button on remote servers).
- `stronghold::vanilla_rings(seed) -> Vec<RingGuess { index, ring, chunk }>`,
  `stronghold::RING_ERROR_CHUNKS` (8).
- `seed_markers(seed, dim, min_chunk, max_chunk, sets) -> Vec<Marker>`: the remote-server view.
- `Verifier` (singleplayer / LAN host): see below. `verifier.markers(dim, min_chunk, max_chunk,
  sets) -> Vec<Marker>` is the same view with server verdicts applied: confirmed ones get
  `Status::Confirmed` and their exact id (`minecraft:village_taiga`), rejected candidates are
  dropped, unchecked ones stay `Possible`, unchecked strongholds `Approximate`.
- `Marker { set, chunk, pos, structure: Option<String>, status }`. Draw at `pos` (x/z); `structure`
  is the exact id when known, else use `set.def().label`.
- `verifier.revision()` changes after every server answer: rebuild cached markers then.

Recommended drawing: `Confirmed` solid icon; `Possible` faded/outlined icon; `Approximate` icon plus
a circle of `RING_ERROR_CHUNKS * 16` blocks. Cache marker lists per 32×32-chunk region and per
`(seed, revision, enabled sets)`; don't recompute every frame.

## Where the seed comes from

- Singleplayer / LAN host: the server half reads `host.level("minecraft:overworld")?.seed(host)`
  (`level_seed`, side server). Works in the integrated server of a `side = "client"` mod (tested).
- Remote server: no seed API on the client. A setting where the user pastes the seed (`/seed` output,
  a number; for a text seed use Java's `String.hashCode`, not implemented here). Store it per server
  address.

## Singleplayer: the exact path (server half)

A client mod's server hooks (`server_tick`, …) run in the integrated server, and the same wasm
instance serves both halves, so a `Mutex` shared between the hooks carries data (as in
my-healthbars). The key trick:

> `level.find_nearest_structure(host, s, StructureSearch::new(candidate.pos, 0))` with **radius 0**
> only searches the placement cell holding `candidate.pos`. So it answers "is this one candidate a
> real structure, and which?" with the game's full biome + terrain check, without loading or
> generating chunks. One call per candidate (two for the nether complex: fortress, then bastion).

`Verifier` plans these calls. Shared state and the hook:

```rust
struct Shared {
    verifier: Option<Verifier>,          // built from the server's seed
    want: Option<(Dimension, [i32; 2])>, // client half: player's dimension + chunk, on region change
    sets: Vec<SetId>,                    // enabled layers
}

fn server_tick(&self, host: &mut dyn ServerHost, event: &mut ServerTickEvent<'_>) {
    if !matches!(event.phase, ServerTickPhase::ServerTickEnd { .. }) { return; }
    let tick = event.context.tick.get();
    let mut sh = self.shared.lock().unwrap();
    if sh.verifier.is_none() {
        let Some(seed) = host.level("minecraft:overworld").and_then(|l| l.seed(host)) else { return };
        sh.verifier = Some(Verifier::new(seed, Budget::default()));
    }
    if let Some((dim, center)) = sh.want.take() {
        let sets = sh.sets.clone();
        let v = sh.verifier.as_mut().unwrap();
        v.retain_near(dim, center, 96);           // forget queued work far away
        v.request_area(dim, center, 64, &sets);   // nearest first
        if dim == Dimension::Overworld { v.request_strongholds(center, 1200); }
    }
    let v = sh.verifier.as_mut().unwrap();
    while let Some(lookup) = v.take(tick) {      // the Budget limits this
        let Some(level) = host.level(lookup.dimension.key()) else { break };
        let answer = lookup.structures.iter().find_map(|s| {
            level.find_nearest_structure(host, s, StructureSearch::new(lookup.origin, lookup.radius))
        });
        v.record(&lookup, answer.as_ref().map(|r| (r.structure.as_str(), r.position)));
    }
}
```

The client half sets `want` when the player crosses into another 32×32-chunk region or changes
dimension, and reads `verifier.markers(..)` (under the same mutex, briefly; cache the result by
`revision()`). On a remote server `server_tick` never runs, `verifier` stays `None`, and the client
uses `seed_markers` with the user's seed instead.

Rules learned the hard way:

- **Every wasm hook call has a ~100 ms budget.** Past it the game traps the call and *stops the
  mod's whole group* ("epoch deadline … stopped its group"). The first probe did 6,561
  `chunk_is_slime_chunk` host calls in one tick and was killed. Keep each tick's work small: the
  `Budget` (default 20 lookups/s, at most 2 per tick) does this for lookups. Slime chunks never need
  host calls (pure math).
- A level only exists once the server has it loaded: `host.level("minecraft:the_nether")` was
  `None` until a player had been in the Nether. Lookups for such a level wait (the loop above
  `break`s and retries next tick).
- Chat to the player is a fine debug channel; `eprintln!` from a wasm mod did not reach the log.

Cost (seed 42, `/tick query` during the probe): 6 radius-0 lookups every tick (120/s, 1,418
lookups over 237 ticks) left the server at 4.4-5.5 ms average per tick (P95 6-12 ms) against a
50 ms budget; idle afterwards 2.2-2.4 ms. So the default 20/s is very safe; 40-60/s is fine too. The
48-chunk-radius Overworld area around 0,0 had 281 candidates, 146 of them mineshafts + buried
treasure (make those two layers opt-in): ~7-14 s at 20/s. Strongholds: 128 lookups total (cheap: the server placed all rings
at world start).

## Evidence (all hidden runs, 2026-11-19, nightly f00125f, seed 42)

- `cargo test`: 26 unit tests + 6 ground-truth tests (`tests/seed42_ingame.rs`).
- Java RNG matches JVM reference values and Horizon's oracle test values.
- Slime: the scratch mod compared `is_slime_chunk` with the server's `chunk_is_slime_chunk` on
  6,561 chunks (-40..40 square) plus 4 far chunks (±1.87 M): **0 mismatches**, 651 slime chunks.
- 28 Overworld positions from `level_find_nearest_structure` (radius 100) and typed
  `/locate structure …` (villages, outposts, desert/jungle pyramids, huts, igloos, monuments, a
  mansion 8,300 blocks out, ocean ruins, shipwrecks, ruined portals, ancient city, trail ruins,
  trial chambers, camps, buried treasure, mineshaft) and 4 Nether/End ones (fortress, bastion,
  fossil, end city): **every one is a seed candidate of its set**, and the nether picks
  (fortress/bastion) match `first_pick`. A one-chunk shift of the same positions hits almost never
  (control test).
- 1,418 radius-0 verifications ran in-game without error; 128 strongholds: every exact start
  within 7 chunks of its ring guess.
- Blocks on the ground (teleport + `horizon-ctl find-block`): confirmed swamp hut → cauldron at
  (-266, 66, 356); confirmed village → bell at (653, 81, -311); confirmed ruined portal → 14
  obsidian; confirmed trial chambers → 8 trial spawners; a rejected hut candidate (96, 368) and a
  rejected village candidate (144, 176) → no cauldron / no bell within 64 blocks.
- Side note on Horizon: typed `/locate` right after world start twice reported a farther swamp hut /
  village / shipwreck / trial chambers than after other lookups had run (both real, checked on the
  ground). `/locate` stops at the first ring of cells with a hit, so its answer depends on details
  of the search; the radius-0 check does not have this problem (one cell, one answer).

## Could Horizon's own worldgen run in wasm for exact biomes on remote servers?

Not as a dependency: `horizon-world` is ~176k lines (70k in `worldgen/`), depends on
`horizon-threads` (mimalloc, native threads), `horizon-assets`, flate2/lz4, and loads the noise
settings, density functions and biome parameter lists from the vanilla jar at runtime — a wasm mod
has none of that. A focused port is feasible (what cubiomes does in C): Xoroshiro + normal noise
(`horizon-core/src/noise`, small), the Overworld noise router's six climate density functions,
and the multi-noise parameter list (~7,000 entries, ~100-200 KB embedded) with its R-tree search
(`worldgen/climate/index.rs`). Estimate 4-6k lines and +0.5-1 MB wasm; a biome sample would cost
tens of µs, fine for a few thousand candidates. It would make villages/temples/monuments exact for
remote servers in most cases, but not all: several structures also check terrain height (mansions,
igloos, pyramids, ancient cities via jigsaw/heightmaps), which needs the full terrain noise. Not
done here; the seed + server-check paths cover singleplayer exactly and remote servers as
"possible".
