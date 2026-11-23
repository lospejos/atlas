# Teleport plan

Goal: teleport to a waypoint or to any point of the world map from World Atlas.

## Facts

- `ClientHost::send_chat(&str)` sends a chat line or, with `/` first, a command (SDK doc: "sent as
  ClientPacketListener.sendChat / sendCommand"). So teleport is `send_chat("/tp @s x y z")`.
  It errors when the player is not in a server's play state (`not connected to a server`, `the player has not joined yet`).
- Needs op (level 2) on a server, cheats in a single-player world. No client-side way to know in advance: send, show the answer.
- The map stores colours, not heights. An unknown Y needs the surface height (`client_surface_heights`, loaded chunks only).
- No local toolchain: CI (`.github/workflows/build.yml`) builds the `.hmod`; the game is driven with the horizon MCP tools.
- SDK pin `0a875da` (`atlas/Cargo.toml`); it is also `SDK_REV` in the workflow.

## Steps

1. [x] CI for atlas (`.github/workflows/build.yml`), branch `teleport`.
2. [x] `/atlas tp <x> <y> <z> [dimension]` through `send_chat`; check it in single player and on a server.
   Verified in single player (cheats world): same-dimension and `nether` jumps both work (CI build of branch `teleport`). A server is not tried yet.
3. [x] Waypoints: a Teleport button in the bubble (the list's right-click opens the same bubble); another dimension through
   `/execute in <dim> run tp @s ...` (`waypoints::tp_command`). Setting `teleport` (off by default) gates the button, the typed
   `/atlas tp` always works. Verified in game: Nether to an Overworld waypoint. Mod settings are plain files in
   `horizon-mods/.horizon-mod-data/world_atlas/<key>`, read at world join (useful for tests).
4. [~] Any map point (code pushed in commit acfbb3b, CI run 45251001699 FAILED in unit tests: `bubble::tests::the_rows_fit_the_bubble` (my `NEW_MAP_H` 86 was 2 px too small, fixed to 88 in the next commit) and `explore::tests::a_left_world_lets_no_seed_through` (passed in the earlier green run a8badfd: check whether it is flaky / shares global state, or whether my change broke it); not yet tested in game): "Teleport here" on a right click. Y: surface height if the chunk is loaded; else `/tp` to the
   world's top, wait for the chunk, read the height, `/tp` to the surface. Nether and caves: the player's Y.
5. [ ] Guard: setting "teleport", off by default; an optional confirmation for far jumps; errors to chat; README, lang.
6. [ ] Release tag `v0.5.0` (version in `atlas/Cargo.toml` and `atlas/horizon-mod.toml`).

Risk: teleport breaks the "honest explorer" mode, hence the setting is off by default.

## Handoff (state at the end of session 1)

Branch `teleport` (pushed to `origin`), based on `main` 4ae6942. Language of the user: Russian.

Done and verified in game:
- Step 1: CI `.github/workflows/build.yml`. Step 2: `/atlas tp <x> <y> <z> [dimension]`. Step 3: Teleport button in the
  waypoint bubble, gated by setting `teleport` (default off).

Step 4, written but unverified (commit acfbb3b):
- A right click on an empty spot of the map opens the new-waypoint bubble; it now has a Teleport button
  (`bubble.rs`: `teleport_here`, `run_pending_tp`, `PendingTp`, `Edit::guess`, `NEW_MAP_H`; `mapscreen.rs` sets `guess`).
- Column explored: plain `/tp` at the stored height. Column unseen (Y is the player's, a guess) in a dimension without a roof:
  `/effect give @s minecraft:slow_falling 20 0 true`, `/tp` to the world top, then, once `client_surface_heights(WorldSurface)`
  has the column, a second `/tp` onto the ground (gives up after 15 s with a chat line).
- Next: check the newest CI run (`gh run list --branch teleport`); if it failed, read `gh run view <id> --log-failed`. Then test in game
  (below): right-click an unexplored spot on the map, press Teleport, check the player lands on the ground, also in the Nether
  (roof: plain `/tp` at the guessed Y).

Still to do: step 5 (a confirmation for far jumps is optional; README and `lang/en_us.json` are done for the button and setting,
add the map-point Teleport to README), step 6 (release tag `v0.5.0`, bump versions in `atlas/Cargo.toml` and
`atlas/horizon-mod.toml`; check whether `[wasm] sha512` in `horizon-mod.toml` needs the new value). A server was never tried.
Open idea: teleport from the radar or the list row without opening the bubble.

How to test without a local toolchain (Windows, Git Bash):
1. `gh run download <run id> -n atlas-hmod -R lospejos/atlas` into a scratch folder; the file is `world_atlas.hmod`.
2. Copy it over `%APPDATA%\horizon\horizon-mods\world_atlas.hmod` (the registry's 0.4.1 copy was backed up in the
   session scratchpad as `world_atlas.orig.hmod`; it is only the unmodified registry build). `horizon-mods.txt` has
   `mod.world_atlas:true`.
3. Enable the setting: write `true` into `%APPDATA%\horizon\horizon-mods\.horizon-mod-data\world_atlas	eleport`
   (settings are plain files, one per key, read at world join: restart the game after writing). Delete the file afterwards.
4. Horizon MCP: `play` (creative, world `atlas_tp_test`), call `game_wait_ready` twice if the first says the game is not under
   control, `game_key` m (tap) opens the map, `game_click_at` takes GUI coordinates (GUI is 1280x720; screenshot pixels / 2 at
   the 2560x1440 window; `right: true` for a right click), `game_messages` shows the `/tp` answer, `game_quit` at the end.
   The test world's player is in the Nether at 25 80 -18; it has waypoint `Home` (Overworld 200 100 -150).
- `horizon-mods/deep_ores.hmod` belongs to another Claude Code session (a different mod): leave it alone.
- Do not install Rust locally (user's rule); read SDK sources with `gh api repos/horizon-rust/horizon/contents/<path>?ref=0a875da...`.
