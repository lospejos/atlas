//! The entity radar: the entities the client has around the player, as dots on the maps.
//! Hostile mobs red, other mobs green, players white, dropped items small and yellow. Only
//! what the server shows this client is known: nothing here sees through that.
use std::collections::HashMap;

use mod_api::{ClientHost, DrawContext, EntityTypeKey, Identifier, ItemKey, LevelRef};

use crate::gfx;
use crate::{Config, State};

/// Blocks either side of the player that are looked at, and up and down.
const REACH: f64 = 96.0;
const REACH_Y: f64 = 24.0;
/// Up and down while a cave layer is shown: about the layer's own height.
const REACH_Y_CAVE: f64 = 14.0;
/// At most this many dots.
const MOST: usize = 400;
/// At most this many of them drawn as faces and icons, the rest as dots.
const ICONS: usize = 96;

/// The item id a dropped item's English name most likely has: "Leaf Litter" is
/// `minecraft:leaf_litter`. The client is told the name, not the id.
pub fn item_guess(name: &str) -> Option<String> {
    let path: String = name
        .trim()
        .chars()
        .map(|c| if c == ' ' { '_' } else { c.to_ascii_lowercase() })
        .filter(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || *c == '_')
        .collect();
    (!path.is_empty()).then(|| format!("minecraft:{path}"))
}

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
    /// What the cursor is told: a player's or an item's name, the type's name for the rest.
    pub name: String,
    /// A dropped item's own icon, when its name gave its id away.
    pub icon: Option<ItemKey>,
    /// The mob's or player's face: its place in `faces::FACES`.
    pub face: Option<usize>,
}

#[derive(Default)]
pub struct Radar {
    pub blips: Vec<Blip>,
    kinds: HashMap<EntityTypeKey, (Option<Class>, String, Option<usize>)>,
    /// The faces' sprite names, by their place in `faces::FACES`.
    pub sprites: Vec<[String; 2]>,
    /// Dropped items' icons by their names.
    items: HashMap<String, Option<ItemKey>>,
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
        if self.sprites.is_empty() {
            self.sprites = (0..crate::faces::FACES.len()).map(crate::faces::sprites).collect();
        }
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
            let (class, type_name, face) = self.kinds.entry(kind).or_insert_with(|| {
                let id = kind.id(host);
                // Vanilla's mobs have a face; a killer rabbit is a rabbit to the game.
                let face = id.strip_prefix("minecraft:").and_then(crate::faces::index);
                (classify(&id), pretty(&id), face)
            });
            let (Some(class), face) = (*class, *face) else { continue };
            let mut icon = None;
            if !class.shown(cfg) {
                continue;
            }
            self.calls += 1;
            let Some(at) = entity.position(host) else { continue };
            let name = if matches!(class, Class::Player | Class::Item) {
                self.calls += 1;
                entity.name(host).unwrap_or_else(|| type_name.clone())
            } else {
                type_name.clone()
            };
            if class == Class::Item {
                icon = match self.items.get(&name) {
                    Some(known) => *known,
                    None => {
                        let found = item_guess(&name)
                            .and_then(|id| Identifier::parse(&id).ok())
                            .and_then(|item| host.item_key(&item));
                        if self.items.len() > 2000 {
                            self.items.clear();
                        }
                        self.items.insert(name.clone(), found);
                        found
                    }
                };
            }
            now.insert(id, at);
            self.blips.push(Blip { class, from: self.last.get(&id).copied().unwrap_or(at), to: at, name, icon, face });
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
    /// Draws face `index` centred on `at`, its longer side `side` GUI pixels, on a one-pixel
    /// dark edge, every texel multiplied by `tint`. A face of two parts (a muzzle beside a head)
    /// has them side by side, their lower edges level. Answers its size, none when the game has
    /// no such sprite (the atlas did not load).
    pub(crate) fn draw_face(
        &self,
        draw: &mut dyn DrawContext,
        index: usize,
        at: [f32; 2],
        side: f32,
        tint: u32,
    ) -> Option<[f32; 2]> {
        let (_, w, h, w2, h2) = crate::faces::FACES[index];
        let (w, h, w2, h2) = (f32::from(w), f32::from(h), f32::from(w2), f32::from(h2));
        let (all_w, all_h) = (w + w2, h.max(h2));
        // The longer side is `side`; a long, low profile is let grow until it is six pixels high.
        let k = (side / all_w.max(all_h)).max((6.0 / all_w.min(all_h)).min(side * 1.5 / all_w.max(all_h)));
        let (fw, fh) = ((all_w * k).round().max(2.0), (all_h * k).round().max(2.0));
        // On whole screen pixels, not whole GUI pixels: a mob walking across the map moves a
        // screen pixel at a time, as the player's own marker does.
        let snap = |v: f32| (v * self.scale).round() / self.scale;
        let corner = [snap(at[0] - fw / 2.0), snap(at[1] - fh / 2.0)];
        let names = self.radar.sprites.get(index)?;
        // The first part's width on the screen; the second takes the rest.
        let first = if w2 > 0.0 { (fw * w / all_w).round() } else { fw };
        let high = |part: f32| (fh * part / all_h).round().max(1.0);
        draw.fill([corner[0] - 1.0, corner[1] - 1.0, fw + 2.0, fh + 2.0], 0xE000_0000);
        let tint = (tint != 0xFFFF_FFFF).then_some(tint);
        let top = corner[1] + fh - high(h);
        if !draw.sprite(&names[0], [0.0, 0.0, w, h], [corner[0], top, first, high(h)], tint) {
            return None;
        }
        if w2 > 0.0 {
            let top = corner[1] + fh - high(h2);
            draw.sprite(&names[1], [0.0, 0.0, w2, h2], [corner[0] + first, top, fw - first, high(h2)], tint);
        }
        Some([fw, fh])
    }

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
        let icons = self.config.icons;
        let mut drawn = 0;
        // Items under mobs under players.
        for class in [Class::Item, Class::Passive, Class::Hostile, Class::Player] {
            for blip in self.radar.blips.iter().filter(|b| b.class == class) {
                let far = (0..3).any(|i| (blip.to[i] - blip.from[i]).abs() > crate::TELEPORT);
                let t = if far { 1.0 } else { partial };
                let x = blip.from[0] + (blip.to[0] - blip.from[0]) * t;
                let z = blip.from[2] + (blip.to[2] - blip.from[2]) * t;
                let Some(at) = place(x, z) else { continue };
                let mut reach = 4.0;
                let face = blip.face.filter(|_| icons && drawn < ICONS);
                let item = blip.icon.filter(|_| drawn < ICONS);
                if let Some(index) = face {
                    let side = (5.0 + size * 3.0).round();
                    let [fw, fh] = self.draw_face(draw, index, at, side, 0xFFFF_FFFF).unwrap_or_else(|| {
                        gfx::dot(draw, at, class.rgb(), size);
                        [4.0, 4.0]
                    });
                    drawn += 1;
                    reach = fw.max(fh) / 2.0 + 1.0;
                } else if class == Class::Item {
                    // The item's own icon when its id is known, else a plain small tile.
                    let side = (size * 4.0 + 2.0).round();
                    let snap = |v: f32| (v * self.scale).round() / self.scale;
                    let corner = [snap(at[0] - side / 2.0), snap(at[1] - side / 2.0)];
                    if item.is_some_and(|key| draw.item(key, corner, side)) {
                        drawn += 1;
                    } else {
                        draw.fill([at[0] - 2.0, at[1] - 2.0, 4.0, 4.0], 0xD000_0000);
                        draw.fill([at[0] - 1.0, at[1] - 1.0, 2.0, 2.0], 0xFFC8_C8C8);
                    }
                } else {
                    let r = size * if class == Class::Player { 1.5 } else { 1.0 };
                    gfx::dot(draw, at, class.rgb(), r);
                }
                if let Some(m) = mouse.filter(|_| self.config.hover)
                    && (m[0] - at[0]).abs() <= reach
                    && (m[1] - at[1]).abs() <= reach
                {
                    hovered = Some(blip.name.clone());
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
        assert_eq!(item_guess("Leaf Litter").as_deref(), Some("minecraft:leaf_litter"));
        assert_eq!(item_guess("Bottle o' Enchanting").as_deref(), Some("minecraft:bottle_o_enchanting"));
    }
}
