//! The entity radar: the entities the client has around the player, as dots on the maps.
//! Hostile mobs red, other mobs green, players white, dropped items small and yellow. Only
//! what the server shows this client is known: nothing here sees through that.
use std::collections::HashMap;

use mod_api::{ClientHost, DrawContext, EntityTypeKey, LevelRef};

use crate::gfx;
use crate::{Config, State};

/// Blocks either side of the player that are looked at, and up and down.
const REACH: f64 = 96.0;
const REACH_Y: f64 = 24.0;
/// Up and down while a cave layer is shown: about the layer's own height.
const REACH_Y_CAVE: f64 = 14.0;
/// At most this many dots.
const MOST: usize = 400;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    Hostile,
    Passive,
    Player,
    Item,
}

impl Class {
    pub fn rgb(self) -> u32 {
        match self {
            Class::Hostile => 0xFF4040,
            Class::Passive => 0x50E050,
            Class::Player => 0xFFFFFF,
            Class::Item => 0xFFE040,
        }
    }

    fn shown(self, cfg: &Config) -> bool {
        match self {
            Class::Hostile => cfg.hostile,
            Class::Passive => cfg.passive,
            Class::Player => cfg.players,
            Class::Item => cfg.items,
        }
    }
}

/// Mobs that attack, by the path of their id.
const HOSTILE: &[&str] = &[
    "zombie", "zombie_villager", "husk", "drowned", "skeleton", "stray", "bogged", "wither_skeleton", "creeper",
    "spider", "cave_spider", "enderman", "witch", "slime", "magma_cube", "blaze", "ghast", "phantom", "pillager",
    "vindicator", "evoker", "illusioner", "ravager", "vex", "guardian", "elder_guardian", "shulker", "silverfish",
    "endermite", "hoglin", "zoglin", "piglin", "piglin_brute", "zombified_piglin", "warden", "wither",
    "ender_dragon", "breeze", "creaking", "giant", "parched", "zombie_nautilus", "camel_husk",
];
/// Entities that are no creature: not on the radar.
const THINGS: &[&str] = &[
    "arrow", "spectral_arrow", "trident", "experience_orb", "item_frame", "glow_item_frame", "painting",
    "armor_stand", "falling_block", "tnt", "fireball", "small_fireball", "dragon_fireball", "wither_skull",
    "snowball", "egg", "ender_pearl", "eye_of_ender", "potion", "splash_potion", "lingering_potion",
    "experience_bottle", "firework_rocket", "leash_knot", "lightning_bolt", "area_effect_cloud", "marker",
    "block_display", "item_display", "text_display", "interaction", "fishing_bobber", "llama_spit",
    "shulker_bullet", "evoker_fangs", "end_crystal", "wind_charge", "breeze_wind_charge", "ominous_item_spawner",
    "minecart", "chest_minecart", "furnace_minecart", "hopper_minecart", "tnt_minecart", "spawner_minecart",
    "command_block_minecart", "mannequin",
];

/// What the entity type `id` (`minecraft:zombie`) is on the radar, if anything.
pub fn classify(id: &str) -> Option<Class> {
    let path = id.rsplit(':').next().unwrap_or(id);
    if path == "player" {
        Some(Class::Player)
    } else if path == "item" {
        Some(Class::Item)
    } else if HOSTILE.contains(&path) {
        Some(Class::Hostile)
    } else if THINGS.contains(&path) || path.ends_with("_boat") || path.ends_with("_raft") {
        None
    } else {
        Some(Class::Passive)
    }
}

pub struct Blip {
    pub class: Class,
    /// Where it was a tick ago and where it is: a frame draws it in between.
    pub from: [f64; 3],
    pub to: [f64; 3],
    /// A player's name, or the type's id for the rest.
    pub name: String,
}

#[derive(Default)]
pub struct Radar {
    pub blips: Vec<Blip>,
    kinds: HashMap<EntityTypeKey, (Option<Class>, String)>,
    last: HashMap<u32, [f64; 3]>,
    /// Host calls the last tick made.
    pub calls: u32,
    pub calls_total: u64,
    pub ticks: u64,
}

impl Radar {
    pub fn clear(&mut self) {
        self.blips.clear();
        self.last.clear();
    }

    /// Reads the entities around `pos`.
    pub fn tick(
        &mut self,
        host: &dyn ClientHost,
        level: LevelRef,
        pos: [f64; 3],
        own: u32,
        cfg: &Config,
        cave: bool,
    ) {
        self.calls = 0;
        self.ticks += 1;
        self.blips.clear();
        if !(cfg.hostile || cfg.passive || cfg.players || cfg.items) {
            self.last.clear();
            return;
        }
        let up = if cave { REACH_Y_CAVE } else { REACH_Y };
        let min = [pos[0] - REACH, pos[1] - up, pos[2] - REACH];
        let max = [pos[0] + REACH, pos[1] + up, pos[2] + REACH];
        self.calls += 1;
        let Some(found) = level.entities_in(host, min, max) else { return };
        let mut now = HashMap::with_capacity(found.len());
        for entity in found {
            let id = entity.id();
            if id == own || self.blips.len() >= MOST {
                continue;
            }
            self.calls += 1;
            let Some(kind) = entity.kind(host) else { continue };
            let (class, type_id) = self.kinds.entry(kind).or_insert_with(|| {
                let id = kind.id(host);
                (classify(&id), id)
            });
            let Some(class) = *class else { continue };
            if !class.shown(cfg) {
                continue;
            }
            self.calls += 1;
            let Some(at) = entity.position(host) else { continue };
            let name = if class == Class::Player {
                self.calls += 1;
                entity.name(host).unwrap_or_else(|| "Player".into())
            } else {
                type_id.clone()
            };
            now.insert(id, at);
            self.blips.push(Blip { class, from: self.last.get(&id).copied().unwrap_or(at), to: at, name });
        }
        self.last = now;
        self.calls_total += u64::from(self.calls);
    }
}

/// A type id in words: `minecraft:zombie_villager` is "Zombie Villager".
pub fn pretty(id: &str) -> String {
    let path = id.rsplit(':').next().unwrap_or(id);
    let mut out = String::with_capacity(path.len());
    for word in path.split('_').filter(|w| !w.is_empty()) {
        if !out.is_empty() {
            out.push(' ');
        }
        let mut chars = word.chars();
        if let Some(first) = chars.next() {
            out.extend(first.to_uppercase());
            out.push_str(chars.as_str());
        }
    }
    out
}

impl State {
    /// Draws the dots through `place`, which answers where a block position is on the screen
    /// (none: off the map), `size` being a mob's dot's radius. Returns the name of the one
    /// under `mouse`.
    pub(crate) fn draw_blips(
        &self,
        draw: &mut dyn DrawContext,
        partial: f64,
        mouse: Option<[f32; 2]>,
        size: f32,
        place: impl Fn(f64, f64) -> Option<[f32; 2]>,
    ) -> Option<String> {
        let mut hovered = None;
        // Items under mobs under players.
        for class in [Class::Item, Class::Passive, Class::Hostile, Class::Player] {
            for blip in self.radar.blips.iter().filter(|b| b.class == class) {
                let far = (0..3).any(|i| (blip.to[i] - blip.from[i]).abs() > crate::TELEPORT);
                let t = if far { 1.0 } else { partial };
                let x = blip.from[0] + (blip.to[0] - blip.from[0]) * t;
                let z = blip.from[2] + (blip.to[2] - blip.from[2]) * t;
                let Some(at) = place(x, z) else { continue };
                let r = size
                    * match class {
                        Class::Item => 0.7,
                        Class::Player => 1.5,
                        _ => 1.0,
                    };
                gfx::dot(draw, at, class.rgb(), r);
                if let Some(m) = mouse
                    && (m[0] - at[0]).abs() <= 4.0
                    && (m[1] - at[1]).abs() <= 4.0
                {
                    hovered = Some(if class == Class::Player { blip.name.clone() } else { pretty(&blip.name) });
                }
            }
        }
        hovered
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kinds_fall_into_their_classes() {
        assert_eq!(classify("minecraft:zombie"), Some(Class::Hostile));
        assert_eq!(classify("minecraft:cow"), Some(Class::Passive));
        assert_eq!(classify("minecraft:player"), Some(Class::Player));
        assert_eq!(classify("minecraft:item"), Some(Class::Item));
        assert_eq!(classify("minecraft:arrow"), None);
        assert_eq!(classify("minecraft:oak_boat"), None);
        assert_eq!(classify("somemod:deer"), Some(Class::Passive));
        assert_eq!(pretty("minecraft:zombie_villager"), "Zombie Villager");
    }
}
