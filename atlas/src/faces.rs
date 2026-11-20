//! The faces the radar draws: for each entity type, the front of its head as vanilla's own
//! texture has it. No pixels are shipped: `assets/atlas/atlases/radar.json` tells the game
//! which rectangle of which of its entity textures to cut out (an atlas `unstitch` source), and
//! the game stitches those into its GUI atlas under `atlas:gui/atlases/radar/atlas/<type>`. A
//! resource pack that changes a mob's texture changes its face here with it.
//!
//! The rectangles are the texture coordinates of the head cube's front face in the game's own
//! entity models (`entity-models.json` of the 26.3 resources). Where the eyes are on the sides
//! of a long head it is a profile put together from the sides of the model's cubes: a horse is
//! its muzzle, its head and the top of its neck; a camel its head and neck; a goat its head
//! under a horn; a fish its head and body.
//!
//! Generated together with the atlas definition.

/// One face: the icon's size in texels and its parts, each `[x, y, width, height]` in the
/// icon. Part `i`'s sprite is the type's name with nothing, `_b` or `_c` after it.
pub struct Face {
    pub name: &'static str,
    pub w: u8,
    pub h: u8,
    pub parts: &'static [[u8; 4]],
}

pub const FACES: [Face; 92] = [
    Face { name: "allay", w: 5, h: 5, parts: &[[0, 0, 5, 5]] },
    Face { name: "armadillo", w: 8, h: 6, parts: &[[0, 0, 8, 6]] },
    Face { name: "axolotl", w: 8, h: 5, parts: &[[0, 0, 8, 5]] },
    Face { name: "bat", w: 8, h: 4, parts: &[[0, 0, 8, 4]] },
    Face { name: "bee", w: 7, h: 7, parts: &[[0, 0, 7, 7]] },
    Face { name: "blaze", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "bogged", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "breeze", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "camel", w: 10, h: 12, parts: &[[0, 0, 10, 8], [3, 8, 7, 4]] },
    Face { name: "camel_husk", w: 10, h: 12, parts: &[[0, 0, 10, 8], [3, 8, 7, 4]] },
    Face { name: "cat", w: 5, h: 4, parts: &[[0, 0, 5, 4]] },
    Face { name: "cave_spider", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "chicken", w: 4, h: 6, parts: &[[0, 0, 4, 6]] },
    Face { name: "cod", w: 10, h: 4, parts: &[[0, 0, 3, 4], [3, 0, 7, 4]] },
    Face { name: "copper_golem", w: 8, h: 5, parts: &[[0, 0, 8, 5]] },
    Face { name: "cow", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "creaking", w: 6, h: 10, parts: &[[0, 0, 6, 10]] },
    Face { name: "creeper", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "dolphin", w: 8, h: 7, parts: &[[0, 0, 8, 7]] },
    Face { name: "donkey", w: 12, h: 12, parts: &[[0, 0, 5, 5], [5, 0, 7, 5], [5, 5, 7, 7]] },
    Face { name: "drowned", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "elder_guardian", w: 12, h: 12, parts: &[[0, 0, 12, 12]] },
    Face { name: "ender_dragon", w: 16, h: 16, parts: &[[0, 0, 16, 16]] },
    Face { name: "enderman", w: 8, h: 6, parts: &[[0, 0, 8, 6]] },
    Face { name: "endermite", w: 4, h: 3, parts: &[[0, 0, 4, 3]] },
    Face { name: "evoker", w: 8, h: 10, parts: &[[0, 0, 8, 10]] },
    Face { name: "fox", w: 8, h: 6, parts: &[[0, 0, 8, 6]] },
    Face { name: "frog", w: 7, h: 5, parts: &[[0, 0, 3, 2], [4, 0, 3, 2], [0, 2, 7, 3]] },
    Face { name: "ghast", w: 32, h: 32, parts: &[[0, 0, 32, 32]] },
    Face { name: "giant", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "glow_squid", w: 12, h: 16, parts: &[[0, 0, 12, 16]] },
    Face { name: "goat", w: 10, h: 10, parts: &[[6, 0, 2, 3], [0, 3, 10, 7]] },
    Face { name: "guardian", w: 12, h: 12, parts: &[[0, 0, 12, 12]] },
    Face { name: "happy_ghast", w: 32, h: 32, parts: &[[0, 0, 32, 32]] },
    Face { name: "hoglin", w: 14, h: 19, parts: &[[0, 0, 14, 19]] },
    Face { name: "horse", w: 12, h: 12, parts: &[[0, 0, 5, 5], [5, 0, 7, 5], [5, 5, 7, 7]] },
    Face { name: "husk", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "illusioner", w: 8, h: 10, parts: &[[0, 0, 8, 10]] },
    Face { name: "iron_golem", w: 8, h: 10, parts: &[[0, 0, 8, 10]] },
    Face { name: "killer_bunny", w: 5, h: 5, parts: &[[0, 0, 5, 5]] },
    Face { name: "llama", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "magma_cube", w: 8, h: 7, parts: &[[0, 0, 8, 7]] },
    Face { name: "mooshroom", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "mule", w: 12, h: 12, parts: &[[0, 0, 5, 5], [5, 0, 7, 5], [5, 5, 7, 7]] },
    Face { name: "nautilus", w: 16, h: 10, parts: &[[0, 0, 16, 10]] },
    Face { name: "ocelot", w: 5, h: 4, parts: &[[0, 0, 5, 4]] },
    Face { name: "panda", w: 13, h: 10, parts: &[[0, 0, 13, 10]] },
    Face { name: "parched", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "parrot", w: 6, h: 5, parts: &[[0, 0, 6, 5]] },
    Face { name: "phantom", w: 7, h: 3, parts: &[[0, 0, 7, 3]] },
    Face { name: "pig", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "piglin", w: 10, h: 8, parts: &[[0, 0, 10, 8]] },
    Face { name: "piglin_brute", w: 10, h: 8, parts: &[[0, 0, 10, 8]] },
    Face { name: "pillager", w: 8, h: 10, parts: &[[0, 0, 8, 10]] },
    Face { name: "player", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "polar_bear", w: 7, h: 7, parts: &[[0, 0, 7, 7]] },
    Face { name: "pufferfish", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "rabbit", w: 5, h: 5, parts: &[[0, 0, 5, 5]] },
    Face { name: "ravager", w: 16, h: 18, parts: &[[0, 0, 16, 18]] },
    Face { name: "salmon", w: 11, h: 5, parts: &[[0, 1, 3, 4], [3, 0, 8, 5]] },
    Face { name: "sheep", w: 6, h: 6, parts: &[[0, 0, 6, 6]] },
    Face { name: "shulker", w: 6, h: 6, parts: &[[0, 0, 6, 6]] },
    Face { name: "silverfish", w: 3, h: 2, parts: &[[0, 0, 3, 2]] },
    Face { name: "skeleton", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "skeleton_horse", w: 12, h: 12, parts: &[[0, 0, 5, 5], [5, 0, 7, 5], [5, 5, 7, 7]] },
    Face { name: "slime", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "sniffer", w: 12, h: 9, parts: &[[0, 0, 12, 9]] },
    Face { name: "snow_golem", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "spider", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "squid", w: 12, h: 16, parts: &[[0, 0, 12, 16]] },
    Face { name: "stray", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "strider", w: 16, h: 14, parts: &[[0, 0, 16, 14]] },
    Face { name: "sulfur_cube", w: 18, h: 18, parts: &[[0, 0, 18, 18]] },
    Face { name: "tadpole", w: 3, h: 2, parts: &[[0, 0, 3, 2]] },
    Face { name: "trader_llama", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "tropical_fish", w: 6, h: 3, parts: &[[0, 0, 6, 3]] },
    Face { name: "turtle", w: 6, h: 5, parts: &[[0, 0, 6, 5]] },
    Face { name: "vex", w: 5, h: 5, parts: &[[0, 0, 5, 5]] },
    Face { name: "villager", w: 8, h: 10, parts: &[[0, 0, 8, 10]] },
    Face { name: "vindicator", w: 8, h: 10, parts: &[[0, 0, 8, 10]] },
    Face { name: "wandering_trader", w: 8, h: 10, parts: &[[0, 0, 8, 10]] },
    Face { name: "warden", w: 16, h: 16, parts: &[[0, 0, 16, 16]] },
    Face { name: "witch", w: 8, h: 10, parts: &[[0, 0, 8, 10]] },
    Face { name: "wither", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "wither_skeleton", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "wolf", w: 6, h: 6, parts: &[[0, 0, 6, 6]] },
    Face { name: "zoglin", w: 14, h: 19, parts: &[[0, 0, 14, 19]] },
    Face { name: "zombie", w: 8, h: 8, parts: &[[0, 0, 8, 8]] },
    Face { name: "zombie_horse", w: 12, h: 12, parts: &[[0, 0, 5, 5], [5, 0, 7, 5], [5, 5, 7, 7]] },
    Face { name: "zombie_nautilus", w: 16, h: 10, parts: &[[0, 0, 16, 10]] },
    Face { name: "zombie_villager", w: 8, h: 10, parts: &[[0, 0, 8, 10]] },
    Face { name: "zombified_piglin", w: 10, h: 8, parts: &[[0, 0, 10, 8]] },
];

/// The place in [`FACES`] of entity type `path` (`zombie`), if it has a face.
pub fn index(path: &str) -> Option<usize> {
    FACES.binary_search_by(|f| f.name.cmp(path)).ok()
}

/// The GUI sprites of face `index`, one for each of its parts.
pub fn sprites(index: usize) -> Vec<String> {
    let face = &FACES[index];
    ["", "_b", "_c"]
        .iter()
        .take(face.parts.len())
        .map(|suffix| format!("atlas:gui/atlases/radar/atlas/{}{suffix}", face.name))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_face_is_in_the_atlas_definition_and_found() {
        let definition = include_str!("../assets/atlas/atlases/radar.json");
        assert!(FACES.windows(2).all(|w| w[0].name < w[1].name), "sorted for the search");
        for (i, face) in FACES.iter().enumerate() {
            assert!(!face.parts.is_empty() && face.parts.len() <= 3, "{}", face.name);
            for (part, sprite) in face.parts.iter().zip(sprites(i)) {
                assert!(part[0] + part[2] <= face.w && part[1] + part[3] <= face.h, "{}", face.name);
                let short = sprite.rsplit('/').next().unwrap_or("");
                assert!(definition.contains(&format!("\"sprite\": \"atlas:{short}\"")), "{short}");
            }
            assert_eq!(index(face.name), Some(i));
            // Square enough to read: no sliver.
            assert!(face.w.max(face.h) <= face.w.min(face.h) * 3, "{}", face.name);
        }
        assert!(index("arrow").is_none());
        // Nothing but vanilla's own textures is named: no picture is shipped.
        assert_eq!(definition.matches("\"resource\": \"minecraft:entity/").count(), FACES.len());
    }
}
