//! The faces the radar draws: for each entity type, the front of its head as vanilla's own
//! texture has it. No pixels are shipped: `assets/atlas/atlases/radar.json` tells the game
//! which rectangle of which of its entity textures to cut out (an atlas `unstitch` source), and
//! the game stitches those into its GUI atlas under `atlas:gui/atlases/radar/atlas/<type>`. A
//! resource pack that changes a mob's texture changes its face here with it.
//!
//! The rectangles are the texture coordinates of the head cube's front face in the game's own
//! entity models (`entity-models.json` of the 26.3 resources). Where the eyes are on the sides
//! of a long head (horses, camels, goats, fish) it is the head's side instead, and for horses
//! and fish two rectangles side by side: the muzzle's side, then the head's.
//!
//! Generated together with the atlas definition: the type's path, the first part's width and
//! height in texels, and the second part's (0 when there is none; its sprite is `<type>_b`).
pub const FACES: [(&str, u8, u8, u8, u8); 92] = [
    ("allay", 5, 5, 0, 0),
    ("armadillo", 8, 6, 0, 0),
    ("axolotl", 8, 5, 0, 0),
    ("bat", 8, 4, 0, 0),
    ("bee", 7, 7, 0, 0),
    ("blaze", 8, 8, 0, 0),
    ("bogged", 8, 8, 0, 0),
    ("breeze", 8, 8, 0, 0),
    ("camel", 19, 8, 0, 0),
    ("camel_husk", 19, 8, 0, 0),
    ("cat", 5, 4, 0, 0),
    ("cave_spider", 8, 8, 0, 0),
    ("chicken", 4, 6, 0, 0),
    ("cod", 3, 4, 7, 4),
    ("copper_golem", 8, 5, 0, 0),
    ("cow", 8, 8, 0, 0),
    ("creaking", 6, 10, 0, 0),
    ("creeper", 8, 8, 0, 0),
    ("dolphin", 8, 7, 0, 0),
    ("donkey", 5, 5, 7, 5),
    ("drowned", 8, 8, 0, 0),
    ("elder_guardian", 12, 12, 0, 0),
    ("ender_dragon", 16, 16, 0, 0),
    ("enderman", 8, 6, 0, 0),
    ("endermite", 4, 3, 0, 0),
    ("evoker", 8, 10, 0, 0),
    ("fox", 8, 6, 0, 0),
    ("frog", 7, 2, 0, 0),
    ("ghast", 32, 32, 0, 0),
    ("giant", 8, 8, 0, 0),
    ("glow_squid", 12, 16, 0, 0),
    ("goat", 10, 7, 0, 0),
    ("guardian", 12, 12, 0, 0),
    ("happy_ghast", 32, 32, 0, 0),
    ("hoglin", 14, 19, 0, 0),
    ("horse", 5, 5, 7, 5),
    ("husk", 8, 8, 0, 0),
    ("illusioner", 8, 10, 0, 0),
    ("iron_golem", 8, 10, 0, 0),
    ("killer_bunny", 5, 5, 0, 0),
    ("llama", 8, 8, 0, 0),
    ("magma_cube", 8, 7, 0, 0),
    ("mooshroom", 8, 8, 0, 0),
    ("mule", 5, 5, 7, 5),
    ("nautilus", 16, 10, 0, 0),
    ("ocelot", 5, 4, 0, 0),
    ("panda", 13, 10, 0, 0),
    ("parched", 8, 8, 0, 0),
    ("parrot", 6, 5, 0, 0),
    ("phantom", 7, 3, 0, 0),
    ("pig", 8, 8, 0, 0),
    ("piglin", 10, 8, 0, 0),
    ("piglin_brute", 10, 8, 0, 0),
    ("pillager", 8, 10, 0, 0),
    ("player", 8, 8, 0, 0),
    ("polar_bear", 7, 7, 0, 0),
    ("pufferfish", 8, 8, 0, 0),
    ("rabbit", 5, 5, 0, 0),
    ("ravager", 16, 18, 0, 0),
    ("salmon", 3, 4, 8, 5),
    ("sheep", 6, 6, 0, 0),
    ("shulker", 6, 6, 0, 0),
    ("silverfish", 3, 2, 0, 0),
    ("skeleton", 8, 8, 0, 0),
    ("skeleton_horse", 5, 5, 7, 5),
    ("slime", 8, 8, 0, 0),
    ("sniffer", 12, 9, 0, 0),
    ("snow_golem", 8, 8, 0, 0),
    ("spider", 8, 8, 0, 0),
    ("squid", 12, 16, 0, 0),
    ("stray", 8, 8, 0, 0),
    ("strider", 16, 14, 0, 0),
    ("sulfur_cube", 18, 18, 0, 0),
    ("tadpole", 3, 2, 0, 0),
    ("trader_llama", 8, 8, 0, 0),
    ("tropical_fish", 6, 3, 0, 0),
    ("turtle", 6, 5, 0, 0),
    ("vex", 5, 5, 0, 0),
    ("villager", 8, 10, 0, 0),
    ("vindicator", 8, 10, 0, 0),
    ("wandering_trader", 8, 10, 0, 0),
    ("warden", 16, 16, 0, 0),
    ("witch", 8, 10, 0, 0),
    ("wither", 8, 8, 0, 0),
    ("wither_skeleton", 8, 8, 0, 0),
    ("wolf", 6, 6, 0, 0),
    ("zoglin", 14, 19, 0, 0),
    ("zombie", 8, 8, 0, 0),
    ("zombie_horse", 5, 5, 7, 5),
    ("zombie_nautilus", 16, 10, 0, 0),
    ("zombie_villager", 8, 10, 0, 0),
    ("zombified_piglin", 10, 8, 0, 0),
];

/// The place in [`FACES`] of entity type `path` (`zombie`), if it has a face.
pub fn index(path: &str) -> Option<usize> {
    FACES.binary_search_by(|f| f.0.cmp(path)).ok()
}

/// The GUI sprites of face `index`: its first part and its second.
pub fn sprites(index: usize) -> [String; 2] {
    let name = FACES[index].0;
    [format!("atlas:gui/atlases/radar/atlas/{name}"), format!("atlas:gui/atlases/radar/atlas/{name}_b")]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_face_is_in_the_atlas_definition_and_found() {
        let definition = include_str!("../assets/atlas/atlases/radar.json");
        assert!(FACES.windows(2).all(|w| w[0].0 < w[1].0), "sorted for the search");
        for (name, w, h, w2, _) in FACES {
            assert!(w > 0 && h > 0);
            assert!(definition.contains(&format!("\"sprite\": \"atlas:{name}\"")), "{name}");
            assert_eq!(definition.contains(&format!("\"sprite\": \"atlas:{name}_b\"")), w2 > 0, "{name}");
            assert!(index(name).is_some_and(|i| sprites(i)[0].ends_with(name)));
        }
        assert!(index("arrow").is_none());
        // Nothing but vanilla's own textures is named: no picture is shipped.
        assert_eq!(definition.matches("\"resource\": \"minecraft:entity/").count(), FACES.len());
    }
}
