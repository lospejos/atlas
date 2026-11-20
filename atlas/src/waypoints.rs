//! Waypoints and their file: one line each, tab separated, in `<world>/waypoints.txt`.

/// The colours a waypoint can have, 0xRRGGBB.
pub const COLORS: [u32; 8] = [
    0xFF5555, 0xFFAA00, 0xFFFF55, 0x55FF55, 0x55FFFF, 0x5599FF, 0xFF55FF, 0xFFFFFF,
];
pub const DEATH_COLOR: u32 = 0xE02020;
pub const SPAWN_COLOR: u32 = 0x7FE0FF;
pub const BED_COLOR: u32 = 0xFF9FC0;
pub const NAME_MAX: usize = 32;
pub const OVERWORLD: &str = "minecraft:overworld";
pub const NETHER: &str = "minecraft:the_nether";
/// What starts a shared waypoint's chat line.
pub const SHARE_TAG: &str = "[Atlas]";

/// Who made a waypoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    User,
    /// The automatic one where the player last died: the next death replaces it.
    Death,
    /// The automatic one at the world spawn.
    Spawn,
    /// The automatic one at the player's bed or respawn anchor.
    Bed,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Waypoint {
    pub name: String,
    pub pos: [i32; 3],
    pub color: u32,
    /// The dimension it is in (`minecraft:overworld`).
    pub dim: String,
    pub kind: Kind,
    /// Kept, but not shown on the maps or in the world.
    pub hidden: bool,
    /// The one the direction arrow points to.
    pub tracked: bool,
}

impl Waypoint {
    pub fn new(name: impl Into<String>, pos: [i32; 3], color: u32, dim: impl Into<String>) -> Self {
        Self { name: name.into(), pos, color, dim: dim.into(), kind: Kind::User, hidden: false, tracked: false }
    }

    pub fn auto(kind: Kind, pos: [i32; 3], dim: impl Into<String>) -> Self {
        let (name, color) = match kind {
            Kind::Death => ("Death", DEATH_COLOR),
            Kind::Spawn => ("Spawn", SPAWN_COLOR),
            Kind::Bed => ("Bed", BED_COLOR),
            Kind::User => ("Waypoint", COLORS[0]),
        };
        Self { kind, ..Self::new(name, pos, color, dim) }
    }

    /// The fifth field of a line. 0.2 wrote 0 or 1 (death) there, which reads the same.
    fn flags(&self) -> u8 {
        let kind = match self.kind {
            Kind::User => 0,
            Kind::Death => 1,
            Kind::Spawn => 4,
            Kind::Bed => 8,
        };
        kind | u8::from(self.hidden) << 1 | u8::from(self.tracked) << 4
    }
}

pub fn encode(list: &[Waypoint]) -> String {
    let mut out = String::from(
        "# atlas waypoints: x y z colour flags dimension name (tab separated; flags: 1 death, 2 hidden, 4 spawn, 8 bed, 16 tracked)\n",
    );
    for w in list {
        let name: String = w.name.chars().filter(|c| !c.is_control()).collect();
        out.push_str(&format!(
            "{}\t{}\t{}\t{:06X}\t{}\t{}\t{}\n",
            w.pos[0],
            w.pos[1],
            w.pos[2],
            w.color,
            w.flags(),
            w.dim,
            name
        ));
    }
    out
}

pub fn decode(text: &str) -> Vec<Waypoint> {
    text.lines().filter_map(line).collect()
}

fn line(line: &str) -> Option<Waypoint> {
    if line.starts_with('#') {
        return None;
    }
    let mut f = line.splitn(7, '\t');
    let pos = [f.next()?.parse().ok()?, f.next()?.parse().ok()?, f.next()?.parse().ok()?];
    let color = u32::from_str_radix(f.next()?, 16).ok()? & 0xFFFFFF;
    let flags: u8 = f.next()?.parse().unwrap_or(0);
    let dim = f.next()?.to_string();
    let name = f.next()?.to_string();
    let kind = match flags & 13 {
        1 => Kind::Death,
        4 => Kind::Spawn,
        8 => Kind::Bed,
        _ => Kind::User,
    };
    Some(Waypoint { name, pos, color, dim, kind, hidden: flags & 2 != 0, tracked: flags & 16 != 0 })
}

/// "87m", "1.4km".
pub fn distance_text(blocks: f64) -> String {
    if blocks < 1000.0 {
        format!("{blocks:.0}m")
    } else {
        format!("{:.1}km", blocks / 1000.0)
    }
}

/// A dimension as a word: "Overworld", "Nether", "End", else its id's path.
pub fn dim_label(dim: &str) -> String {
    match dim {
        OVERWORLD => "Overworld".into(),
        NETHER => "Nether".into(),
        "minecraft:the_end" => "End".into(),
        other => other.rsplit(':').next().unwrap_or(other).to_string(),
    }
}

/// The place in the other dimension a portal at `pos` in `dim` leads to: Overworld and Nether
/// only, the Nether an eighth the size. The height is kept, inside the Nether's.
pub fn linked(pos: [i32; 3], dim: &str) -> Option<([i32; 3], &'static str)> {
    match dim {
        OVERWORLD => Some(([pos[0].div_euclid(8), pos[1].clamp(5, 120), pos[2].div_euclid(8)], NETHER)),
        NETHER => Some(([pos[0] * 8, pos[1], pos[2] * 8], OVERWORLD)),
        _ => None,
    }
}

/// The chat line that shares `w`: `[Atlas] name @ x y z dimension`.
pub fn share_line(w: &Waypoint) -> String {
    let name: String = w.name.chars().filter(|c| !c.is_control()).collect();
    let dim = w.dim.strip_prefix("minecraft:").unwrap_or(&w.dim);
    format!("{SHARE_TAG} {name} @ {} {} {} {dim}", w.pos[0], w.pos[1], w.pos[2])
}

/// The waypoint a chat line shares, whatever stands before the tag (the sender's name). The
/// name may itself hold " @ ": the last one counts.
pub fn parse_share(line: &str) -> Option<Waypoint> {
    let rest = &line[line.find(SHARE_TAG)? + SHARE_TAG.len()..];
    let (name, place) = rest.rsplit_once(" @ ")?;
    let mut f = place.split_whitespace();
    let pos = [f.next()?.parse().ok()?, f.next()?.parse().ok()?, f.next()?.parse().ok()?];
    let dim = f.next()?;
    let ok = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '.' | '-' | '/' | ':');
    if !dim.chars().all(ok) || dim.len() > 80 {
        return None;
    }
    let dim = if dim.contains(':') { dim.to_string() } else { format!("minecraft:{dim}") };
    let name: String = name.trim().chars().filter(|c| !c.is_control()).take(NAME_MAX).collect();
    if name.is_empty() {
        return None;
    }
    Some(Waypoint::new(name, pos, COLORS[4], dim))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_file_keeps_every_field() {
        let mut hidden = Waypoint::new("tab\there", [1, 2, 3], 0xFFFFFF, "a:b");
        hidden.hidden = true;
        hidden.tracked = true;
        let list = vec![
            Waypoint::new("Home base", [-12, 64, 300], 0x55FF55, OVERWORLD),
            Waypoint::auto(Kind::Death, [0, -60, 0], NETHER),
            Waypoint::auto(Kind::Bed, [5, 70, 5], OVERWORLD),
            Waypoint::auto(Kind::Spawn, [0, 64, 0], OVERWORLD),
            hidden,
        ];
        let back = decode(&encode(&list));
        assert_eq!(back[..4], list[..4]);
        assert_eq!(back[4].name, "tabhere");
        assert!(back[4].hidden && back[4].tracked && back[4].kind == Kind::User);
        assert_eq!(decode("garbage\n1\t2\n"), vec![]);
    }

    #[test]
    fn a_0_2_file_reads_the_same() {
        let old = "# my_map waypoints: x y z colour death dimension name (tab separated)\n\
                   -12\t64\t300\t55FF55\t0\tminecraft:overworld\tHome base\n\
                   0\t-60\t0\tE02020\t1\tminecraft:the_nether\tDeath\n";
        let list = decode(old);
        assert_eq!(list[0], Waypoint::new("Home base", [-12, 64, 300], 0x55FF55, OVERWORLD));
        assert_eq!(list[1], Waypoint::auto(Kind::Death, [0, -60, 0], NETHER));
    }

    #[test]
    fn distances_read_short() {
        assert_eq!(distance_text(87.4), "87m");
        assert_eq!(distance_text(1449.0), "1.4km");
    }

    #[test]
    fn portals_link_both_ways() {
        assert_eq!(linked([-17, 70, 800], OVERWORLD), Some(([-3, 70, 100], NETHER)));
        assert_eq!(linked([-3, 70, 100], NETHER), Some(([-24, 70, 800], OVERWORLD)));
        assert_eq!(linked([0, 250, 0], OVERWORLD).unwrap().0[1], 120);
        assert_eq!(linked([0, 0, 0], "minecraft:the_end"), None);
    }

    #[test]
    fn a_shared_line_comes_back() {
        let w = Waypoint::new("Base @ river", [-120, 64, 77], 0xFF5555, NETHER);
        let line = share_line(&w);
        assert_eq!(line, "[Atlas] Base @ river @ -120 64 77 the_nether");
        let back = parse_share(&format!("<Steve> {line}")).unwrap();
        assert_eq!((back.name.as_str(), back.pos, back.dim.as_str()), ("Base @ river", [-120, 64, 77], NETHER));
        assert_eq!(parse_share("[Atlas] x @ 1 2 3 mymod:moon").unwrap().dim, "mymod:moon");
        assert!(parse_share("hello there").is_none());
        assert!(parse_share("[Atlas] x @ 1 2 overworld").is_none());
        assert!(parse_share("[Atlas] x @ 1 2 3 Bad").is_none(), "a dimension id is lower case");
    }
}
