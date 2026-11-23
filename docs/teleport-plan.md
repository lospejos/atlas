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
4. [ ] Any map point: "Teleport here" on a right click. Y: surface height if the chunk is loaded; else `/tp` to the
   world's top, wait for the chunk, read the height, `/tp` to the surface. Nether and caves: the player's Y.
5. [ ] Guard: setting "teleport", off by default; an optional confirmation for far jumps; errors to chat; README, lang.
6. [ ] Release tag `v0.5.0` (version in `atlas/Cargo.toml` and `atlas/horizon-mod.toml`).

Risk: teleport breaks the "honest explorer" mode, hence the setting is off by default.
