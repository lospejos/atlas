//! The faces the radar draws: for each entity type, the front of its head as vanilla's own
//! texture has it. No pixels are shipped: `assets/atlas/atlases/radar.json` tells the game
//! which rectangle of which of its entity textures to cut out (an atlas `unstitch` source), and
//! the game stitches those into its GUI atlas under `atlas:gui/atlases/radar/atlas/<type>`. A
//! resource pack that changes a mob's texture changes its face here with it.
//!
//! Generated together with the atlas definition: the type's path, the face's width and height
//! in texels.
pub const FACES: [(&str, u8, u8); 92] = [
    ("allay", 5, 5),
    ("armadillo", 8, 6),
    ("axolotl", 8, 5),
    ("bat", 8, 4),
    ("bee", 7, 7),
    ("blaze", 8, 8),
    ("bogged", 8, 8),
    ("breeze", 8, 8),
    ("camel", 9, 9),
    ("camel_husk", 9, 9),
    ("cat", 5, 4),
    ("cave_spider", 8, 8),
    ("chicken", 4, 6),
    ("cod", 7, 4),
    ("copper_golem", 8, 7),
    ("cow", 8, 8),
    ("creaking", 8, 8),
    ("creeper", 8, 8),
    ("dolphin", 8, 7),
    ("donkey", 7, 5),
    ("drowned", 8, 8),
    ("elder_guardian", 12, 12),
    ("ender_dragon", 16, 16),
    ("enderman", 8, 6),
    ("endermite", 4, 3),
    ("evoker", 8, 10),
    ("fox", 8, 6),
    ("frog", 8, 4),
    ("ghast", 32, 32),
    ("giant", 8, 8),
    ("glow_squid", 12, 16),
    ("goat", 10, 7),
    ("guardian", 12, 12),
    ("happy_ghast", 32, 32),
    ("hoglin", 14, 14),
    ("horse", 7, 5),
    ("husk", 8, 8),
    ("illusioner", 8, 10),
    ("iron_golem", 8, 10),
    ("killer_bunny", 7, 5),
    ("llama", 8, 8),
    ("magma_cube", 8, 7),
    ("mooshroom", 8, 8),
    ("mule", 7, 5),
    ("nautilus", 14, 14),
    ("ocelot", 5, 4),
    ("panda", 13, 10),
    ("parched", 8, 8),
    ("parrot", 6, 5),
    ("phantom", 7, 3),
    ("pig", 8, 8),
    ("piglin", 10, 8),
    ("piglin_brute", 10, 8),
    ("pillager", 8, 10),
    ("player", 8, 8),
    ("polar_bear", 7, 7),
    ("pufferfish", 8, 8),
    ("rabbit", 7, 5),
    ("ravager", 16, 18),
    ("salmon", 8, 5),
    ("sheep", 6, 6),
    ("shulker", 6, 6),
    ("silverfish", 3, 2),
    ("skeleton", 8, 8),
    ("skeleton_horse", 7, 5),
    ("slime", 8, 8),
    ("sniffer", 14, 9),
    ("snow_golem", 8, 8),
    ("spider", 8, 8),
    ("squid", 12, 16),
    ("stray", 8, 8),
    ("strider", 16, 14),
    ("sulfur_cube", 20, 19),
    ("tadpole", 3, 2),
    ("trader_llama", 8, 8),
    ("tropical_fish", 8, 2),
    ("turtle", 6, 5),
    ("vex", 5, 5),
    ("villager", 8, 10),
    ("vindicator", 8, 10),
    ("wandering_trader", 8, 10),
    ("warden", 16, 16),
    ("witch", 8, 10),
    ("wither", 8, 8),
    ("wither_skeleton", 8, 8),
    ("wolf", 6, 6),
    ("zoglin", 14, 14),
    ("zombie", 8, 8),
    ("zombie_horse", 7, 5),
    ("zombie_nautilus", 14, 14),
    ("zombie_villager", 8, 10),
    ("zombified_piglin", 10, 8),
];

/// The place in [`FACES`] of entity type `path` (`zombie`), if it has a face.
pub fn index(path: &str) -> Option<usize> {
    FACES.binary_search_by(|f| f.0.cmp(path)).ok()
}

/// The GUI sprite of face `index`.
pub fn sprite(index: usize) -> String {
    format!("atlas:gui/atlases/radar/atlas/{}", FACES[index].0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_face_is_in_the_atlas_definition_and_found() {
        let definition = include_str!("../assets/atlas/atlases/radar.json");
        assert!(FACES.windows(2).all(|w| w[0].0 < w[1].0), "sorted for the search");
        for (name, w, h) in FACES {
            assert!(definition.contains(&format!("\"sprite\": \"atlas:{name}\"")), "{name}");
            assert!(index(name).is_some_and(|i| FACES[i] == (name, w, h)));
            assert!(sprite(index(name).unwrap()).ends_with(name));
        }
        assert!(index("arrow").is_none());
        // Nothing but vanilla's own textures is named: no picture is shipped.
        assert_eq!(definition.matches("\"resource\": \"minecraft:entity/").count(), FACES.len());
    }
}
