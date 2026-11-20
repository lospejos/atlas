//! Atlas: a minimap, a full-screen map of everything explored (the surface, cave layers and the
//! Nether under and over its roof), an entity radar and waypoints shown as beams in the world,
//! on both maps and in a list.
//!
//! The map is one image: the game gives client mods a 1024 x 1024 slot in its GUI atlas
//! (`ClientHost::mod_image`). The minimap or the open full map is painted into it when what it
//! shows changes, never every frame, and drawn as one sprite. Explored columns live in
//! `store` (regions of 512 x 512 blocks, one file each, a folder per layer), read by `scan` a
//! few chunks a tick.
mod bubble;
mod explore;
mod export;
mod faces;
mod gfx;
mod mapscreen;
mod mapview;
mod minimap;
mod overlays;
mod png;
mod probe;
mod radar;
mod render;
mod scan;
mod store;
mod waypoints;

use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};

use mod_api::{
    ClientChatReceivedEvent, ClientHost, ClientLevelUnloadEvent, ClientPlayerLoggingOutEvent, ClientStoppingEvent,
    ClientTickPostEvent, Command, CommandEvent, ConfigPath, Flow, Heightmap, Hooks, HostSide, KeyConflictContext,
    KeyRegistration, LevelRef, LightLayer, LivingRef, PlayerTickPostEvent, RegisterClientCommandsEvent,
    RegistrationContext, Registrar, RenderFramePreEvent, RenderGuiPostEvent, RenderLevelStage,
    RenderLevelStageEvent, ScreenCharTypedPreEvent, ScreenClosingEvent, ScreenKeyPressedPreEvent,
    ScreenMouseButtonPressedPreEvent, ScreenMouseButtonReleasedPreEvent, ScreenMouseDraggedPreEvent,
    ScreenMouseScrolledPreEvent, ScreenRenderPostEvent, SettingSpec, Text, WorldDepth,
    WorldQuad,
};

use bubble::Edit;
use mapview::{Drag, Glide, Image, Slide, Trail, View, ZoomAnim};
use render::Palette;
use scan::{Layer, Scanner};
use store::{Legend, Store};
use waypoints::{Kind, Waypoint};

pub const ID: &str = "atlas";
/// The atlas slot `mod_image` paints.
const SPRITE: &str = "horizon/modimage";
/// The slot's side in texels.
const IMAGE_MAX: usize = 1024;
/// Test builds only: letter keys type into the name field (the control socket the hidden tests
/// drive the game through presses keys but sends no text into a mod's screen), and `probe`
/// writes what it measures to `probe.log`.
const TEST: bool = false;
const KEY_MINIMAP: &str = "minimap";
const KEY_MAP: &str = "map";
const KEY_WAYPOINT: &str = "waypoint";
/// Blocks per GUI pixel for each "minimap zoom" choice.
const MINIMAP_ZOOMS: [f32; 5] = [0.25, 0.5, 1.0, 2.0, 4.0];
/// A tick's move longer than this is a teleport: nothing is drawn in between.
const TELEPORT: f64 = 12.0;
const FRAME_COLOR: u32 = 0xFF15_1515;
/// The chat's words when a bed or respawn anchor takes the player's spawn point.
const SPAWN_SET: &str = "Respawn point set";

/// What the mod's half in the integrated server of a single-player world (or a LAN host)
/// reads for the client half: the world spawn and the player's own respawn point. A client
/// has neither. On someone else's server nothing fills this.
#[derive(Default)]
struct ServerSide {
    /// The player this client is, as its own tick names it.
    local: Option<u128>,
    spawn: Option<[i32; 3]>,
    /// The respawn point: where and in which dimension; `Some(None)` when there is none.
    bed: Option<Option<([i32; 3], String)>>,
}
static SERVER: Mutex<ServerSide> = Mutex::new(ServerSide { local: None, spawn: None, bed: None });
/// Counts the server half's reads, so the client half knows a new one came.
static SERVER_BEAT: AtomicU64 = AtomicU64::new(0);
const UNSEEN_COLOR: u32 = 0xFF1B_1D22;

/// Which layer the maps show.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub(crate) enum Mode {
    /// The cave layer while the player is underground (or under the Nether's roof), else
    /// the surface.
    #[default]
    Auto,
    Surface,
    Cave,
}

/// The settings as the last tick read them.
#[derive(Clone, Copy, PartialEq)]
pub(crate) struct Config {
    minimap: bool,
    size: f32,
    /// Blocks per GUI pixel on the minimap.
    scale: f32,
    /// 0 top right, 1 top left, 2 bottom right, 3 bottom left.
    corner: u32,
    round: bool,
    rotate: bool,
    coords: bool,
    other_coords: bool,
    mode: Mode,
    tints: bool,
    hostile: bool,
    passive: bool,
    players: bool,
    items: bool,
    /// Mobs' and players' faces on the radar instead of dots.
    icons: bool,
    /// The world map names the radar marker under the cursor.
    hover: bool,
    beams: bool,
    labels: bool,
    hover_names: bool,
    death: bool,
    spawn: bool,
    bed: bool,
    arrow: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            minimap: true,
            size: 96.0,
            scale: 1.0,
            corner: 0,
            round: false,
            rotate: false,
            coords: true,
            other_coords: false,
            mode: Mode::Auto,
            tints: true,
            hostile: true,
            passive: true,
            players: true,
            items: true,
            icons: true,
            hover: true,
            beams: true,
            labels: true,
            hover_names: false,
            death: true,
            spawn: true,
            bed: true,
            arrow: false,
        }
    }
}

/// The on/off settings: key and default.
const TOGGLES: [(&str, bool); 18] = [
    ("radar_hover", true),
    ("minimap", true),
    ("minimap_round", false),
    ("minimap_rotate", false),
    ("coords", true),
    ("other_coords", false),
    ("biome_colors", true),
    ("radar_hostile", true),
    ("radar_passive", true),
    ("radar_players", true),
    ("radar_items", true),
    ("beams", true),
    ("labels", true),
    ("hover_names", false),
    ("death", true),
    ("spawn", true),
    ("bed", true),
    ("arrow", false),
];

fn settings() -> Vec<SettingSpec> {
    let label = |key: &str| format!("atlas.setting.{key}");
    let toggle = |key: &str| {
        let default = TOGGLES.iter().find(|t| t.0 == key).is_some_and(|t| t.1);
        SettingSpec::toggle(key, label(key), default).tooltip(format!("atlas.setting.{key}.tooltip"))
    };
    let choice = |key: &str, options: &[&str], default: usize| {
        let options: Vec<String> = options.iter().map(|o| format!("atlas.setting.{key}.{o}")).collect();
        SettingSpec::choice(key, label(key), options, default).tooltip(format!("atlas.setting.{key}.tooltip"))
    };
    let mut list = vec![
        toggle("minimap"),
        SettingSpec::range("minimap_size", label("minimap_size"), 48.0, 200.0, 8.0, 96.0)
            .tooltip("atlas.setting.minimap_size.tooltip"),
        choice("minimap_zoom", &["4x", "2x", "1x", "half", "quarter"], 2),
        choice("minimap_corner", &["top_right", "top_left", "bottom_right", "bottom_left"], 0),
        toggle("minimap_round"),
        toggle("minimap_rotate"),
        toggle("coords"),
        toggle("other_coords"),
        choice("layer", &["auto", "surface", "cave"], 0),
        toggle("biome_colors"),
        toggle("radar_hostile"),
        toggle("radar_passive"),
        toggle("radar_players"),
        toggle("radar_items"),
        choice("radar_style", &["icons", "dots"], 0),
        toggle("radar_hover"),
        toggle("beams"),
        toggle("labels"),
        toggle("hover_names"),
        toggle("death"),
        toggle("spawn"),
        toggle("bed"),
        toggle("arrow"),
    ];
    for slot in overlays::registered() {
        let key = format!("overlay_{}", slot.overlay.id());
        list.push(SettingSpec::toggle(key.clone(), label(&key), false));
    }
    list
}

#[derive(Clone, Copy)]
pub(crate) struct Player {
    pos: [f64; 3],
    yaw: f32,
}

#[derive(Default)]
pub(crate) enum Ui {
    #[default]
    Closed,
    Map,
    Edit(Edit),
}

/// What the dimension the player is in is like.
#[derive(Clone, Copy)]
pub(crate) struct DimInfo {
    /// It has a roof (the Nether): under it everything is a cave.
    ceiling: bool,
    /// The first height over the roof.
    roof: i32,
    skylight: bool,
}

impl Default for DimInfo {
    fn default() -> Self {
        Self { ceiling: false, roof: 128, skylight: true }
    }
}

/// What the painting did, for the test builds' log.
#[derive(Default, Clone, Copy)]
pub(crate) struct Stats {
    uploads: u64,
    /// Texels uploaded, all told.
    texels: u64,
    /// Uploads made while the view was being panned: there should be none.
    while_panning: u64,
    /// Mosaic cells drawn in the last frame.
    cells: u32,
}

pub(crate) struct State {
    /// `sp/<save folder>` or `mp/<address>`: the folder this world's files are in.
    world: Option<String>,
    dim: String,
    dim_info: DimInfo,
    /// The top of each column: in the Nether, the top of its roof.
    surface: Store,
    /// The cave layer of `band`, open while it is shown.
    cave: Store,
    /// The cave layers left last, by band, still in memory.
    spare: Vec<(i32, Store)>,
    /// What the biome and tint numbers in the regions stand for.
    legend: Legend,
    /// The legend's tints in the map's brightnesses, as of `tints_revision`.
    tints: Vec<render::Tints>,
    tints_revision: u64,
    scan_surface: Scanner,
    scan_cave: Scanner,
    /// The cave band the player is in, while a cave layer is shown.
    band: Option<i32>,
    /// Whether the player is under ground, and for how many ticks the opposite has been seen.
    underground: bool,
    under_ticks: u32,
    palette: Palette,
    waypoints: Vec<Waypoint>,
    /// The player as this frame draws them: between the last two ticks' positions, with the
    /// view's own yaw.
    player: Option<Player>,
    /// The last two ticks' positions, older first.
    ticks: Option<([f64; 3], [f64; 3])>,
    was_dead: bool,
    config: Config,
    overlays: Vec<overlays::Slot>,
    image: Option<Image>,
    pixels: Vec<u8>,
    /// When the view last moved by a drag, a glide or a slide, on the frame clock.
    panned: f64,
    /// The GUI's size as the last draw saw it.
    gui: [f32; 2],
    ui: Ui,
    view: Option<View>,
    close: bool,
    drag: Option<Drag>,
    glide: Option<Glide>,
    slide: Option<Slide>,
    /// A slide to a picked spot: from, to and when it began.
    goto: Option<([f64; 2], [f64; 2], f64)>,
    trail: Trail,
    /// The cursor as this frame read it, in GUI pixels.
    cursor: [f32; 2],
    /// Blocks a second the view moved over the last frame.
    vel: [f64; 2],
    /// What the last draw of the world map put where, topmost last: what a click lands on.
    hits: Vec<(mapscreen::Hit, [f32; 4])>,
    /// Where the block a bubble opened in the world is about lies on the screen this frame.
    edit_spot: Option<[f32; 2]>,
    /// The status line's explanation of what the cursor is on.
    tip: Option<String>,
    /// A line shown over the world map for a while: what it says and until when.
    notice: Option<(String, u64)>,
    list_scroll: f32,
    next_color: usize,
    now: u64,
    /// Seconds of frames drawn, for animations: finer than `now`.
    clock: f64,
    zoom_anim: Option<ZoomAnim>,
    /// The frame's partial tick, for what moves between ticks.
    partial: f32,
    radar: radar::Radar,
    explore: explore::Explore,
    soft: minimap::Soft,
    export: Option<export::Export>,
    /// Waypoints shared in chat, waiting in the list to be taken.
    inbox: Vec<Waypoint>,
    /// Chat lines to send, and lines to show in this client's own chat, at the next tick.
    outbox: Vec<String>,
    said: Vec<String>,
    /// The server half's read this state has taken in.
    server_beat: u64,
    stats: Stats,
    probe: probe::Probe,
}

impl Default for State {
    fn default() -> Self {
        Self {
            world: None,
            dim: String::new(),
            dim_info: DimInfo::default(),
            surface: Store::keeping(64),
            cave: Store::keeping(16),
            spare: Vec::new(),
            legend: Legend::default(),
            tints: Vec::new(),
            tints_revision: u64::MAX,
            scan_surface: Scanner::new(Layer::Surface),
            scan_cave: Scanner::new(Layer::Cave(0)),
            band: None,
            underground: false,
            under_ticks: 0,
            palette: Palette::default(),
            waypoints: Vec::new(),
            player: None,
            ticks: None,
            was_dead: false,
            config: Config::default(),
            overlays: overlays::registered(),
            image: None,
            pixels: Vec::new(),
            panned: -1.0,
            gui: [0.0; 2],
            ui: Ui::Closed,
            view: None,
            close: false,
            drag: None,
            glide: None,
            slide: None,
            goto: None,
            trail: Trail::new(),
            cursor: [0.0; 2],
            vel: [0.0; 2],
            hits: Vec::new(),
            edit_spot: None,
            tip: None,
            notice: None,
            list_scroll: 0.0,
            next_color: 0,
            now: 0,
            clock: 0.0,
            zoom_anim: None,
            partial: 0.0,
            radar: radar::Radar::default(),
            explore: explore::Explore::default(),
            soft: minimap::Soft::default(),
            export: None,
            inbox: Vec::new(),
            outbox: Vec::new(),
            said: Vec::new(),
            server_beat: 0,
            stats: Stats::default(),
            probe: probe::Probe::default(),
        }
    }
}

fn sanitize(name: &str) -> String {
    let clean: String = name
        .chars()
        .map(|c| if c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '_') { c } else { '_' })
        .take(80)
        .collect();
    // The prefix keeps device names (CON, COM1) and a trailing dot away.
    format!("w_{}_", clean)
}

/// The block the crosshair is on, as far as 256 blocks away: the view's own long raycast, else
/// a ray from the player's eyes (which only the integrated server answers).
fn aimed_block(host: &dyn ClientHost) -> (Option<[i32; 3]>, String) {
    let ray = host.raycast(256.0).map(|hit| [hit.block_x, hit.block_y, hit.block_z]);
    let view = host.view_hit().filter(|(far, _)| *far <= 256.0).map(|(_, hit)| [hit.block_x, hit.block_y, hit.block_z]);
    let pick = host
        .local_player()
        .and_then(|p| p.entity().pick(host, 256.0))
        .filter(|hit| !hit.miss)
        .map(|hit| hit.block);
    let seen = if TEST { format!("aim: raycast {ray:?} view_hit {view:?} pick {pick:?}") } else { String::new() };
    (ray.or(view).or(pick), seen)
}

fn block_of(pos: [f64; 3]) -> [i32; 3] {
    [pos[0].floor() as i32, pos[1].floor() as i32, pos[2].floor() as i32]
}

impl State {
    /// The folder of the open world's current dimension: its surface regions, and a folder
    /// `cave.<band>` in it for each cave layer.
    fn dim_dir(&self) -> Option<String> {
        Some(format!("{}/{}", self.world.as_ref()?, self.dim.replace(|c: char| !c.is_ascii_alphanumeric(), "_")))
    }

    fn save_waypoints(&self, host: &dyn ClientHost) {
        let Some(world) = &self.world else { return };
        if let Ok(path) = ConfigPath::new(&format!("{world}/waypoints.txt")) {
            let _ = host.data_files().write(&path, waypoints::encode(&self.waypoints).as_bytes());
        }
    }

    /// Writes the legend when it grew: before any region that uses its numbers.
    fn save_legend(&mut self, host: &dyn ClientHost) {
        let Some(world) = &self.world else { return };
        if self.legend.dirty
            && let Ok(path) = ConfigPath::new(&format!("{world}/legend.txt"))
            && host.data_files().write(&path, self.legend.encode().as_bytes()).is_ok()
        {
            self.legend.dirty = false;
        }
    }

    /// Writes up to `budget` changed regions of each layer; with no limit, as many as 50 ms
    /// allow. A hook call that runs past 100 ms is cut off by the game and the mod stopped, so
    /// leaving a world never writes for longer; the hooks that follow write what is left.
    fn save(&mut self, host: &dyn ClientHost, budget: usize) {
        self.save_legend(host);
        if self.legend.dirty {
            return;
        }
        if budget != usize::MAX {
            self.surface.save(budget);
            self.cave.save(budget);
            return;
        }
        let start = host.millis();
        while host.millis().saturating_sub(start) < 50 {
            let spare: usize = self.spare.iter_mut().map(|(_, s)| s.save(1)).sum();
            if self.surface.save(1) + self.cave.save(1) + spare == 0 {
                break;
            }
        }
    }

    /// Writes everything and forgets the world.
    fn leave(&mut self, host: &dyn ClientHost) {
        if self.world.is_some() {
            self.save(host, usize::MAX);
            self.save_waypoints(host);
        }
        self.surface.close();
        self.cave.close();
        for (_, mut store) in self.spare.drain(..) {
            store.close();
        }
        self.scan_surface.clear();
        self.scan_cave.clear();
        self.legend = Legend::default();
        self.tints_revision = u64::MAX;
        self.band = None;
        self.underground = false;
        self.world = None;
        self.dim.clear();
        self.waypoints.clear();
        self.player = None;
        self.ticks = None;
        self.zoom_anim = None;
        self.drag = None;
        self.stop_motion();
        self.image = None;
        self.view = None;
        self.ui = Ui::Closed;
        self.was_dead = false;
        self.radar.clear();
        self.explore.leave();
        self.export = None;
        self.inbox.clear();
        self.outbox.clear();
        self.server_beat = 0;
        if let Ok(mut server) = SERVER.try_lock() {
            *server = ServerSide::default();
        }
    }

    /// Follows the world and dimension the client is in: opens their files when they change.
    fn follow_world(&mut self, host: &dyn ClientHost) {
        let dim = host.dimension();
        if self.world.is_some() && dim == self.dim && self.surface.is_open() {
            return;
        }
        if self.world.is_none() {
            let Some(server) = host.client_session().client_server() else { return };
            let world = match (&server.save_folder, &server.address, &server.name) {
                (Some(folder), _, _) if server.integrated_local => format!("sp/{}", sanitize(folder)),
                (_, Some(address), _) => format!("mp/{}", sanitize(address)),
                (_, _, Some(name)) => format!("sp/{}", sanitize(name)),
                _ => "mp/w_unknown_".to_string(),
            };
            let read = |name: &str| {
                ConfigPath::new(&format!("{world}/{name}"))
                    .ok()
                    .and_then(|path| host.data_files().read(&path).ok())
                    .map(|bytes| String::from_utf8_lossy(&bytes).into_owned())
            };
            self.waypoints = read("waypoints.txt").map(|t| waypoints::decode(&t)).unwrap_or_default();
            self.legend = read("legend.txt").map(|t| Legend::decode(&t)).unwrap_or_default();
            self.tints_revision = u64::MAX;
            self.world = Some(world);
        } else {
            self.save(host, usize::MAX);
        }
        self.dim = dim.to_string();
        let kind = host.level(&self.dim).and_then(|level| level.dimension_type(host));
        self.dim_info = match kind {
            Some(t) => DimInfo { ceiling: t.has_ceiling, roof: t.min_y + t.logical_height, skylight: t.has_skylight },
            None if self.dim == waypoints::NETHER => DimInfo { ceiling: true, roof: 128, skylight: false },
            None => DimInfo::default(),
        };
        self.scan_surface.clear();
        self.scan_cave.clear();
        self.cave.close();
        for (_, mut store) in self.spare.drain(..) {
            store.close();
        }
        self.band = None;
        self.underground = false;
        self.under_ticks = 0;
        self.image = None;
        self.view = None;
        self.radar.clear();
        self.export = None;
        if let Some(dir) = self.dim_dir() {
            self.surface.open(host.data_files(), dir);
        }
    }

    fn read_config(&mut self, host: &dyn ClientHost) {
        let d = Config::default();
        let on = |key: &str, default: bool| host.setting_bool(key).unwrap_or(default);
        let zoom = host.setting_choice("minimap_zoom").unwrap_or(2) as usize;
        self.config = Config {
            minimap: on("minimap", d.minimap),
            size: host.setting_f64("minimap_size").map_or(d.size, |s| s as f32),
            scale: MINIMAP_ZOOMS.get(zoom).copied().unwrap_or(1.0),
            corner: host.setting_choice("minimap_corner").unwrap_or(0),
            round: on("minimap_round", d.round),
            rotate: on("minimap_rotate", d.rotate),
            coords: on("coords", d.coords),
            other_coords: on("other_coords", d.other_coords),
            mode: match host.setting_choice("layer") {
                Some(1) => Mode::Surface,
                Some(2) => Mode::Cave,
                _ => Mode::Auto,
            },
            tints: on("biome_colors", d.tints),
            hostile: on("radar_hostile", d.hostile),
            passive: on("radar_passive", d.passive),
            players: on("radar_players", d.players),
            items: on("radar_items", d.items),
            icons: host.setting_choice("radar_style").unwrap_or(0) == 0,
            hover: on("radar_hover", d.hover),
            beams: on("beams", d.beams),
            labels: on("labels", d.labels),
            hover_names: on("hover_names", d.hover_names),
            death: on("death", d.death),
            spawn: on("spawn", d.spawn),
            bed: on("bed", d.bed),
            arrow: on("arrow", d.arrow),
        };
        for slot in &mut self.overlays {
            slot.on = on(&format!("overlay_{}", slot.overlay.id()), false);
        }
    }

    /// Flips an on/off setting and keeps it.
    pub(crate) fn flip(&mut self, host: &dyn ClientHost, key: &str) {
        let default = TOGGLES.iter().find(|t| t.0 == key).is_some_and(|t| t.1);
        let now = !host.setting_bool(key).unwrap_or(default);
        host.set_setting(ID, key, if now { "true" } else { "false" });
        self.read_config(host);
    }

    pub(crate) fn set_mode(&mut self, host: &dyn ClientHost, mode: Mode) {
        let index = match mode {
            Mode::Auto => "0",
            Mode::Surface => "1",
            Mode::Cave => "2",
        };
        host.set_setting(ID, "layer", index);
        self.read_config(host);
        // The setting may read back late: the choice holds from now.
        self.config.mode = mode;
    }

    /// Whether the maps show the cave layer now.
    pub(crate) fn shown_cave(&self) -> bool {
        match self.config.mode {
            Mode::Surface => false,
            Mode::Cave => true,
            Mode::Auto => self.underground,
        }
    }

    /// The layer the maps show.
    pub(crate) fn shown(&self) -> &Store {
        if self.shown_cave() && self.band.is_some() { &self.cave } else { &self.surface }
    }

    /// The waypoints of this dimension that are shown, with their index.
    fn here(&self) -> impl Iterator<Item = (usize, &Waypoint)> {
        self.waypoints.iter().enumerate().filter(|(_, w)| w.dim == self.dim && !w.hidden)
    }

    fn add_waypoint(&mut self, host: &dyn ClientHost, waypoint: Waypoint) {
        if waypoint.kind != Kind::User {
            self.waypoints.retain(|w| w.kind != waypoint.kind);
        }
        self.waypoints.push(waypoint);
        self.save_waypoints(host);
    }

    fn default_name(&self) -> String {
        format!("Waypoint {}", self.waypoints.iter().filter(|w| w.kind == Kind::User).count() + 1)
    }

    /// The render table of the legend's tints, made again when the legend grew.
    fn refresh_tints(&mut self) {
        if self.tints_revision == self.legend.revision && !self.tints.is_empty() {
            return;
        }
        self.tints_revision = self.legend.revision;
        self.tints = self.legend.tints().map(render::tinted).collect();
    }

    /// Keeps the automatic waypoint of `kind` at `at`, or removes it with `at` none. One the
    /// player hid stays hidden, renamed stays renamed.
    fn keep_auto(&mut self, host: &dyn ClientHost, kind: Kind, at: Option<([i32; 3], String)>) {
        let old = self.waypoints.iter().position(|w| w.kind == kind);
        match (old, at) {
            (Some(i), Some((pos, dim))) => {
                let w = &mut self.waypoints[i];
                if w.pos == pos && w.dim == dim {
                    return;
                }
                (w.pos, w.dim) = (pos, dim);
            }
            (None, Some((pos, dim))) => self.waypoints.push(Waypoint::auto(kind, pos, dim)),
            (Some(i), None) => {
                self.waypoints.remove(i);
            }
            (None, None) => return,
        }
        self.save_waypoints(host);
    }

    /// Takes in what the server half read: the world spawn and the respawn point.
    fn sync_auto(&mut self, host: &dyn ClientHost) {
        let beat = SERVER_BEAT.load(Ordering::Relaxed);
        if beat != self.server_beat
            && let Ok(server) = SERVER.try_lock()
        {
            let (spawn, bed) = (server.spawn, server.bed.clone());
            drop(server);
            self.server_beat = beat;
            if let Some(pos) = spawn.filter(|_| self.config.spawn) {
                self.keep_auto(host, Kind::Spawn, Some((pos, waypoints::OVERWORLD.to_string())));
            }
            if let Some(bed) = bed.filter(|_| self.config.bed) {
                self.keep_auto(host, Kind::Bed, bed);
            }
        }
        if !self.config.spawn {
            self.keep_auto(host, Kind::Spawn, None);
        }
        if !self.config.bed {
            self.keep_auto(host, Kind::Bed, None);
        }
    }

    /// Takes every waypoint waiting in the inbox. Returns how many.
    fn import_all(&mut self, host: &dyn ClientHost) -> usize {
        let taken = std::mem::take(&mut self.inbox);
        let count = taken.len();
        for wp in taken {
            self.add_waypoint(host, wp);
        }
        count
    }

    /// Whether the player is under ground: under the roof in a dimension that has one, else
    /// under a roof of blocks with little of the sky's light reaching them. A change must hold
    /// for a second before it counts.
    fn watch_underground(&mut self, host: &dyn ClientHost, level: LevelRef, pos: [f64; 3]) {
        let [x, y, z] = block_of(pos);
        let under = if self.dim_info.ceiling {
            y < self.dim_info.roof - 1
        } else {
            let eye = y + 1;
            let roofed = host
                .client_surface_heights(&[(x, z)], Heightmap::MotionBlockingNoLeaves)
                .ok()
                .and_then(|h| h.first().copied().flatten())
                .is_some_and(|top| top > eye + 2);
            let dark = !self.dim_info.skylight
                || level.brightness(host, LightLayer::Sky, [x, eye, z]).is_none_or(|light| light < 8);
            roofed && dark
        };
        if under == self.underground {
            self.under_ticks = 0;
            return;
        }
        self.under_ticks += 1;
        if self.dim_info.ceiling || self.under_ticks >= 20 {
            self.underground = under;
            self.under_ticks = 0;
        }
    }

    /// Reads the terrain around the player into the layers: the cave layer of the player's
    /// band while it is shown, and the surface, slower then.
    fn scan(&mut self, host: &dyn ClientHost, level: LevelRef, pos: [f64; 3]) {
        let center = ((pos[0].floor() as i32) >> 4, (pos[2].floor() as i32) >> 4);
        let cave = self.shown_cave();
        if cave {
            let band = scan::band(pos[1], self.band);
            if self.band != Some(band) || !self.cave.is_open() {
                // The layer left stays in memory beside the one entered, so coming back to it
                // shows it at once. The image in the slot stays too, until the new one is in.
                let old = std::mem::replace(&mut self.cave, Store::keeping(16));
                if let (Some(was), true) = (self.band, old.is_open()) {
                    self.spare.retain(|(b, _)| *b != was);
                    self.spare.push((was, old));
                    if self.spare.len() > 2 {
                        let (_, mut dropped) = self.spare.remove(0);
                        dropped.close();
                    }
                }
                match self.spare.iter().position(|(b, _)| *b == band) {
                    Some(at) => self.cave = self.spare.remove(at).1,
                    None => {
                        if let Some(dir) = self.dim_dir() {
                            self.cave.open(host.data_files(), format!("{dir}/cave.{band}"));
                        }
                    }
                }
                self.band = Some(band);
                self.scan_cave.clear();
                self.scan_cave.layer = Layer::Cave(band);
            }
            self.scan_cave.run(host, level, &mut self.cave, &mut self.legend, center, 2, 0);
        }
        // The cave layer stays open while the surface is shown: walking back in finds it.
        let (fresh, again) = if cave { (2, 4) } else { (5, 8) };
        self.scan_surface.run(host, level, &mut self.surface, &mut self.legend, center, fresh, again);
    }
}

#[derive(Default)]
struct Atlas {
    // Taken with `try_lock`: a host call can fire another of this mod's hooks, which then
    // steps aside instead of waiting for itself.
    state: Mutex<State>,
}

impl Atlas {
    /// Runs `f` on the state while this mod's own screen is open, else answers `Continue`.
    fn on_screen(&self, host: &dyn ClientHost, f: impl FnOnce(&mut State) -> Flow) -> Flow {
        if host.mod_screen() != Some(ID) {
            return Flow::Continue;
        }
        match self.state.try_lock() {
            Ok(mut state) if !matches!(state.ui, Ui::Closed) => f(&mut state),
            _ => Flow::Continue,
        }
    }

    /// Opens the mod's screen on the full map, or on the new-waypoint dialog at the player.
    /// True when it is open now.
    fn open(&self, host: &mut dyn ClientHost, map: bool) -> bool {
        // J in the world: the block the crosshair is on, as far as 256 blocks off.
        let (aimed, seen) = if map { (None, String::new()) } else { aimed_block(host) };
        {
            let Ok(mut st) = self.state.try_lock() else { return false };
            let (Ui::Closed, Some(player), true) = (&st.ui, st.player, st.world.is_some()) else { return false };
            let pos = player.pos;
            // Set before the call: the screen's hooks may run inside it.
            if map {
                let zoom = st.view.map_or(1.0, |v| v.zoom);
                st.view = Some(View { cx: pos[0], cz: pos[2], zoom });
                st.ui = Ui::Map;
            } else {
                if TEST {
                    st.probe_note(host, seen);
                }
                let mut edit = Edit::new(block_of(pos), st.next_color, false);
                if let Some([x, y, z]) = aimed {
                    edit.pos = [x, y + 1, z];
                    edit.anchor = Some([f64::from(x) + 0.5, f64::from(y) + 1.0, f64::from(z) + 0.5]);
                }
                st.edit_spot = None;
                st.ui = Ui::Edit(edit);
            }
            st.list_scroll = 0.0;
            st.drag = None;
            st.zoom_anim = None;
            st.stop_motion();
            st.close = false;
        }
        let opened = host.mod_screen_open(ID);
        if !opened && let Ok(mut st) = self.state.try_lock() {
            st.ui = Ui::Closed;
        }
        opened
    }
}

#[mod_api::horizon_mod]
impl Hooks for Atlas {
    fn register(&self, r: &mut dyn Registrar, _: &RegistrationContext) {
        for (id, key) in [(KEY_MINIMAP, "key.keyboard.n"), (KEY_MAP, "key.keyboard.m"), (KEY_WAYPOINT, "key.keyboard.j")] {
            r.register_key_mapping(
                KeyRegistration::new(id, key).label(&format!("key.atlas.{id}")).context(KeyConflictContext::InGame),
            );
        }
        for setting in settings() {
            let _ = r.register_setting(setting);
        }
    }

    fn client_tick_post(&self, host: &mut dyn ClientHost, _: &mut ClientTickPostEvent) {
        let Ok(mut guard) = self.state.try_lock() else { return };
        let st = &mut *guard;
        let began = if TEST { host.millis() } else { 0 };
        if !host.on(ID) {
            st.leave(host);
            return;
        }
        let Some(local) = host.local_player() else {
            st.leave(host);
            return;
        };
        let entity = local.entity();
        let (Some(pos), Some(rot)) = (entity.position(host), entity.rotation(host)) else { return };
        st.follow_world(host);
        st.read_config(host);
        st.ticks = Some((st.ticks.map_or(pos, |t| t.1), pos));
        if st.player.is_none() {
            st.player = Some(Player { pos, yaw: rot[0] });
        }

        // Death: the moment health runs out, where the player stood.
        let dead = LivingRef::new(entity).health(host).is_some_and(|h| h <= 0.0);
        if dead && !st.was_dead && st.config.death && st.world.is_some() {
            let dim = st.dim.clone();
            st.add_waypoint(host, Waypoint::auto(Kind::Death, block_of(pos), dim));
        }
        st.was_dead = dead;

        // Keys.
        if host.consume_click(ID, KEY_MINIMAP) {
            st.flip(host, "minimap");
            st.image = None;
        }
        let open_map = host.consume_click(ID, KEY_MAP);
        let open_edit = host.consume_click(ID, KEY_WAYPOINT);
        if open_map || open_edit {
            drop(guard);
            self.open(host, open_map);
            return;
        }

        // The terrain around the player, and the files.
        if let Some(level) = host.level(&st.dim) {
            st.watch_underground(host, level, pos);
            st.scan(host, level, pos);
            let cfg = st.config;
            let cave = st.shown_cave();
            st.radar.tick(host, level, pos, entity.id(), &cfg, cave);
            let query = overlays::OverlayQuery {
                dimension: &st.dim,
                bounds: [pos[0] - 256.0, pos[2] - 256.0, pos[0] + 256.0, pos[2] + 256.0],
                tick: st.scan_surface.tick,
            };
            for slot in st.overlays.iter_mut().filter(|s| s.on) {
                slot.overlay.tick(host, &query);
            }
            if TEST {
                st.probe_tick(host, level, pos);
            }
        }
        // A changed region is written every few seconds, one at a time (about 3 ms), the two
        // layers in turn.
        if st.scan_surface.tick.is_multiple_of(50) {
            let before = if TEST { host.millis() } else { 0 };
            st.save_legend(host);
            if !st.legend.dirty {
                if st.scan_surface.tick.is_multiple_of(100) {
                    st.surface.save(1);
                } else if st.cave.save(1) == 0 {
                    for (_, store) in &mut st.spare {
                        if store.save(1) > 0 {
                            break;
                        }
                    }
                }
            }
            if TEST {
                st.probe.save_ms = st.probe.save_ms.max(host.millis() - before);
            }
        }
        st.sync_auto(host);
        st.explore_tick(host, pos);
        // What the screen's hooks, which cannot send, left to do.
        for line in std::mem::take(&mut st.outbox) {
            if let Err(why) = host.send_chat(&line) {
                st.note(format!("Could not share: {why}"));
            }
        }
        for line in std::mem::take(&mut st.said) {
            let _ = host.chat_line(Text::literal(line));
        }
        if TEST {
            let took = host.millis() - began;
            st.probe.tick_ms = (st.probe.tick_ms.0 + took, st.probe.tick_ms.1.max(took), st.probe.tick_ms.2 + 1);
        }
    }

    /// The world spawn and the player's respawn point, which only a server knows: read by
    /// this mod's half in the integrated server for the player this client is.
    fn player_tick_post(&self, host: HostSide, event: &mut PlayerTickPostEvent) {
        match host {
            HostSide::Client(_) => {
                if let Ok(mut server) = SERVER.try_lock() {
                    server.local = Some(event.player.uuid());
                }
            }
            HostSide::Server(host) => {
                if TEST && let Ok(mut t) = explore::TRACE.try_lock() {
                    *t = "player_tick_post: respawn reads".into();
                }
                let Ok(mut server) = SERVER.try_lock() else { return };
                if server.local != Some(event.player.uuid()) {
                    return;
                }
                // Twice a second is plenty.
                let beat = SERVER_BEAT.load(Ordering::Relaxed);
                static TICKS: AtomicU64 = AtomicU64::new(0);
                if !TICKS.fetch_add(1, Ordering::Relaxed).is_multiple_of(10) {
                    return;
                }
                let spawn = host.level(waypoints::OVERWORLD).and_then(|l| l.respawn_data(&*host)).map(|d| d.pos);
                let bed = event.player.respawn_config(&*host).map(|config| {
                    config.map(|c| {
                        let p = c.position.pos();
                        ([p.x, p.y, p.z], c.position.dimension().to_string())
                    })
                });
                if server.spawn != spawn || server.bed != bed {
                    server.spawn = spawn;
                    server.bed = bed;
                    SERVER_BEAT.store(beat + 1, Ordering::Relaxed);
                }
            }
        }
    }

    /// The radar's faces: an atlas whose definition cuts them out of the game's own entity
    /// textures (`assets/atlas/atlases/radar.json`). The game stitches them into its GUI atlas.
    fn register_texture_atlases(&self, _: &RegistrationContext, event: &mut mod_api::RegisterTextureAtlasesEvent<'_>) {
        if let Ok(id) = mod_api::Identifier::parse("atlas:radar") {
            event.atlases.push(mod_api::TextureAtlasRegistration { atlas: id.clone(), definition: id });
        }
    }

    /// The explore overlays' server half: the seed, and the structure checks.
    fn server_tick(&self, host: &mut dyn mod_api::ServerHost, event: &mut mod_api::ServerTickEvent<'_>) {
        explore::server_tick(host, event);
    }

    /// A waypoint shared in chat (`[Atlas] name @ x y z dimension`) is offered in the world
    /// map's list, and its chat line gets an [Import] to click. And where there is no server
    /// half to ask: the chat's "Respawn point set" marks the bed.
    fn client_chat_received(&self, host: &mut dyn ClientHost, event: &mut ClientChatReceivedEvent<'_>) -> Flow {
        let Ok(mut st) = self.state.try_lock() else { return Flow::Continue };
        if st.world.is_none() || event.overlay {
            return Flow::Continue;
        }
        let Ok(line) = host.text_string(event.message) else { return Flow::Continue };
        if event.system && line.trim() == SPAWN_SET && st.server_beat == 0 && st.config.bed {
            if let Some(p) = st.player {
                let dim = st.dim.clone();
                st.keep_auto(host, Kind::Bed, Some((block_of(p.pos), dim)));
            }
            return Flow::Continue;
        }
        let Some(mut wp) = waypoints::parse_share(&line) else { return Flow::Continue };
        let same = |w: &Waypoint| w.pos == wp.pos && w.dim == wp.dim;
        if st.waypoints.iter().any(same) || st.inbox.iter().any(same) {
            return Flow::Continue;
        }
        wp.color = waypoints::COLORS[st.next_color % waypoints::COLORS.len()];
        st.note(format!("Waypoint \"{}\" shared in chat: press M to add it", wp.name));
        st.inbox.push(wp);
        st.inbox.truncate(20);
        let mut style = mod_api::text::TextStyle::default();
        style.color = Some("aqua".into());
        style.click = Some(mod_api::text::TextClick::RunCommand("/atlas import".into()));
        *event.message = event.message.clone().append(Text::literal(" [Import]").with_style(style));
        Flow::Continue
    }

    fn register_client_commands(&self, _: &mut dyn ClientHost, event: &mut RegisterClientCommandsEvent<'_>) {
        event.commands.push(Command::new("atlas import"));
    }

    /// `/atlas import`: takes the waypoints shared in chat.
    fn command(&self, host: &mut dyn ClientHost, event: &mut CommandEvent<'_>) -> Flow {
        let line = event.command.trim_start_matches('/').trim();
        if line != "atlas import" {
            return Flow::Continue;
        }
        if let Ok(mut st) = self.state.try_lock() {
            let count = st.import_all(host);
            let text = match count {
                0 => "[Atlas] Nothing to import".to_string(),
                1 => "[Atlas] Waypoint added".to_string(),
                n => format!("[Atlas] {n} waypoints added"),
            };
            let _ = host.chat_line(Text::literal(text));
        }
        Flow::Cancel
    }

    fn render_frame_pre(&self, host: &mut dyn ClientHost, event: &mut RenderFramePreEvent) {
        let Ok(mut guard) = self.state.try_lock() else { return };
        let st = &mut *guard;
        if st.world.is_none() {
            return;
        }
        st.now = host.millis();
        let dt = event.elapsed.as_secs_f64().min(0.25);
        st.clock += dt;
        st.partial = event.world_partial_tick.clamp(0.0, 1.0);
        st.frame_player(host, event.world_partial_tick);
        // The cursor, once a frame: a drag follows it, not the mouse's events.
        st.cursor = host.cursor();
        if TEST {
            st.probe_drive(host);
        }
        st.animate_view(dt);
        if TEST {
            st.probe_frame(host, dt);
        }
        let ours = host.mod_screen() == Some(ID);
        if st.close && ours {
            st.close = false;
            st.ui = Ui::Closed;
            drop(guard);
            host.mod_screen_close(ID);
            return;
        }
        if !ours && !matches!(st.ui, Ui::Closed) {
            // Escape, or another screen, closed it.
            st.ui = Ui::Closed;
            st.close = false;
        }
        st.surface.pump(2);
        st.cave.pump(2);
        st.export_step(host);
        st.refresh_image(host);
        if TEST {
            let took = host.millis().saturating_sub(st.now);
            st.probe.frame_ms = (st.probe.frame_ms.0 + took, st.probe.frame_ms.1.max(took), st.probe.frame_ms.2 + 1);
        }
    }

    fn render_gui_post(&self, host: &dyn ClientHost, event: &mut RenderGuiPostEvent<'_>) {
        let Ok(mut guard) = self.state.try_lock() else { return };
        let st = &mut *guard;
        st.gui = event.draw.size();
        if st.world.is_none() || !host.on(ID) || matches!(st.ui, Ui::Map) {
            return;
        }
        if st.config.labels {
            st.draw_labels(host, event.draw);
        }
        if st.config.minimap {
            st.draw_minimap(host, event);
        }
        if st.config.arrow && matches!(st.ui, Ui::Closed) {
            st.draw_direction(event.draw);
        }
        // A waypoint shared in chat is said on the HUD too, for a moment.
        if let Some((text, until)) = &st.notice
            && *until > st.now
            && !st.inbox.is_empty()
        {
            let tw = event.draw.text_width(text);
            let x = ((st.gui[0] - tw) / 2.0).round();
            event.draw.fill([x - 4.0, 40.0, tw + 8.0, 12.0], 0x9000_0000);
            event.draw.text(text, [x, 42.0], 0xFF9F_F0F0, false);
        }
    }

    fn render_level_stage(&self, _: &dyn ClientHost, event: &mut RenderLevelStageEvent<'_>) {
        if event.stage != RenderLevelStage::AfterTranslucentBlocks {
            return;
        }
        let Ok(st) = self.state.try_lock() else { return };
        if !st.config.beams || st.world.is_none() {
            return;
        }
        let cam = event.view.camera;
        let mut quads = Vec::new();
        for (_, wp) in st.here() {
            let (mut dx, mut dz) = (f64::from(wp.pos[0]) + 0.5 - cam[0], f64::from(wp.pos[2]) + 0.5 - cam[2]);
            let dist = (dx * dx + dz * dz).sqrt();
            if dist < 1.0 {
                continue;
            }
            // Past the drawn distance the beam stands in its direction, nearer.
            let shown = dist.min(160.0);
            (dx, dz) = (dx / dist * shown, dz / dist * shown);
            // Across the view, so the beam faces the camera.
            let (px, pz) = (-dz / shown, dx / shown);
            let (bottom, top) = if dist > 160.0 {
                (cam[1] - 96.0, cam[1] + 192.0)
            } else {
                (f64::from(wp.pos[1]).min(cam[1] - 8.0).max(-64.0), f64::from(wp.pos[1]).max(cam[1]) + 256.0)
            };
            for (half, alpha) in [(0.14 + shown * 0.0035, 0xB0u32), (0.34 + shown * 0.008, 0x38)] {
                let (ax, az) = (cam[0] + dx - px * half, cam[2] + dz - pz * half);
                let (bx, bz) = (cam[0] + dx + px * half, cam[2] + dz + pz * half);
                let corners = [[ax, top, az], [ax, bottom, az], [bx, bottom, bz], [bx, top, bz]];
                let color = alpha << 24 | wp.color;
                quads.push(WorldQuad::new(corners, color, WorldDepth::Tested));
                // The same quad seen from behind, in case back faces are culled.
                let back = [corners[3], corners[2], corners[1], corners[0]];
                quads.push(WorldQuad::new(back, color, WorldDepth::Tested));
            }
        }
        if !quads.is_empty() {
            let _ = event.draw.quads(&quads);
        }
    }

    fn screen_render_post(&self, host: &dyn ClientHost, event: &mut ScreenRenderPostEvent<'_>) {
        if host.mod_screen() != Some(ID) {
            return;
        }
        let Ok(mut guard) = self.state.try_lock() else { return };
        let st = &mut *guard;
        st.gui = event.draw.size();
        let mouse = event.screen.mouse;
        if TEST {
            st.probe_draw(mouse);
        }
        match std::mem::take(&mut st.ui) {
            Ui::Closed => {}
            Ui::Map => {
                st.ui = Ui::Map;
                st.draw_map(host, event.draw, mouse, true);
            }
            Ui::Edit(edit) => {
                if edit.from_map {
                    st.draw_map(host, event.draw, mouse, false);
                }
                // The block the bubble is about, where the view has it now.
                st.edit_spot = edit.anchor.and_then(|at| host.project(at)).and_then(|[nx, ny, _]| {
                    let on = nx.abs() <= 1.0 && ny.abs() <= 1.0;
                    on.then_some([(nx + 1.0) / 2.0 * st.gui[0], (1.0 - ny) / 2.0 * st.gui[1]])
                });
                st.draw_edit(event.draw, &edit, mouse);
                st.ui = Ui::Edit(edit);
            }
        }
    }

    fn screen_mouse_button_pressed_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseButtonPressedPreEvent<'_>,
    ) -> Flow {
        let (mouse, button) = (event.screen.mouse, event.button);
        self.on_screen(host, |st| {
            if TEST {
                st.probe.press = mouse;
            }
            if matches!(st.ui, Ui::Edit(_)) {
                st.edit_press(host, mouse, button);
            } else {
                st.map_press(host, mouse, button);
            }
            Flow::Cancel
        })
    }

    fn screen_mouse_button_released_pre(
        &self,
        host: &dyn ClientHost,
        event: &mut ScreenMouseButtonReleasedPreEvent<'_>,
    ) -> Flow {
        let mouse = event.screen.mouse;
        self.on_screen(host, |st| {
            if TEST {
                st.probe_release(host, mouse);
            }
            st.drag_to(mouse);
            st.end_drag();
            Flow::Cancel
        })
    }

    fn screen_mouse_dragged_pre(&self, host: &dyn ClientHost, event: &mut ScreenMouseDraggedPreEvent<'_>) -> Flow {
        let (delta, mouse) = (event.delta, event.screen.mouse);
        // The view follows the cursor itself, where it is: here and again each frame. The
        // event's own delta is not added up.
        self.on_screen(host, |st| {
            if TEST {
                st.probe.drag_event(delta);
            }
            st.drag_to(mouse);
            Flow::Cancel
        })
    }

    fn screen_mouse_scrolled_pre(&self, host: &dyn ClientHost, event: &mut ScreenMouseScrolledPreEvent<'_>) -> Flow {
        let (mouse, steps) = (event.screen.mouse, event.scroll[1] as f32);
        self.on_screen(host, |st| {
            if matches!(st.ui, Ui::Map) {
                st.map_scroll(mouse, steps);
            }
            Flow::Cancel
        })
    }

    fn screen_key_pressed_pre(&self, host: &dyn ClientHost, event: &mut ScreenKeyPressedPreEvent<'_>) -> Flow {
        let key = event.key;
        self.on_screen(host, |st| {
            if matches!(st.ui, Ui::Edit(_)) { st.edit_key(host, key) } else { st.map_key(host, key) }
        })
    }

    fn screen_char_typed_pre(&self, host: &dyn ClientHost, event: &mut ScreenCharTypedPreEvent<'_>) -> Flow {
        let c = event.c;
        self.on_screen(host, |st| {
            if let Ui::Edit(edit) = &mut st.ui {
                edit.type_char(c);
            } else {
                st.explore_char(c);
            }
            Flow::Cancel
        })
    }

    fn screen_closing(&self, _: &dyn ClientHost, event: &mut ScreenClosingEvent<'_>) {
        if event.screen.id != "mod" {
            return;
        }
        if let Ok(mut st) = self.state.try_lock() {
            st.ui = Ui::Closed;
            st.drag = None;
            st.stop_motion();
            st.image = None;
        }
    }

    fn client_level_unload(&self, host: &mut dyn ClientHost, _: &mut ClientLevelUnloadEvent<'_>) {
        if let Ok(mut st) = self.state.try_lock() {
            st.save(host, usize::MAX);
            st.save_waypoints(host);
        }
    }

    fn client_player_logging_out(&self, host: &mut dyn ClientHost, _: &mut ClientPlayerLoggingOutEvent) {
        if let Ok(mut st) = self.state.try_lock() {
            st.leave(host);
        }
    }

    fn client_stopping(&self, host: &mut dyn ClientHost, _: &mut ClientStoppingEvent) {
        if let Ok(mut st) = self.state.try_lock() {
            st.leave(host);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_setting_is_one_the_game_takes() {
        let list = settings();
        for setting in &list {
            assert_eq!(setting.check(), Ok(()), "{}", setting.key);
        }
        for (key, _) in TOGGLES {
            assert!(list.iter().any(|s| s.key == key), "{key}");
        }
    }

    #[test]
    fn every_setting_has_its_words() {
        let lang = include_str!("../assets/atlas/lang/en_us.json");
        for setting in settings() {
            assert!(lang.contains(&format!("\"atlas.setting.{}\"", setting.key)), "{}", setting.key);
        }
        for key in [KEY_MINIMAP, KEY_MAP, KEY_WAYPOINT] {
            assert!(lang.contains(&format!("\"key.atlas.{key}\"")));
        }
    }

    #[test]
    fn world_folders_are_safe_names() {
        for name in ["Test", "New World (2)", "CON", "play.example.net:25565", "..", "a/b\\c"] {
            let folder = sanitize(name);
            assert!(ConfigPath::new(&format!("sp/{folder}/x_y/r.0.0.bin")).is_ok(), "{folder}");
            assert!(ConfigPath::new(&format!("sp/{folder}/x_y/cave.-3/r.0.-1.bin")).is_ok(), "{folder}");
        }
    }
}
