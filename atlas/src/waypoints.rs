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
/// Shared waypoints waiting in the list at most, and from one sender at most.
pub const INBOX_MAX: usize = 20;
pub const INBOX_PER_SENDER: usize = 5;

/// Whether `c` may stand in a name: what vanilla lets into a chat line
/// (`StringUtil.isAllowedChatCharacter`: no `§`, no control character, no DEL). A `§` in a line
/// the client sends gets it kicked.
pub fn allowed_char(c: char) -> bool {
    c != '\u{a7}' && c >= ' ' && c != '\u{7f}' && !c.is_control()
}

/// `text` with every character a chat line may not hold left out.
pub fn clean(text: &str) -> String {
    text.chars().filter(|c| allowed_char(*c)).collect()
}

/// The next waypoint's id: unique for the life of the process, never saved.
fn fresh_id() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(1);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

/// Who made a waypoint.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    User,
    /// An automatic one where the player died: the last few are kept (`add_death`).
    Death,
    /// The automatic one at the world spawn.
    Spawn,
    /// The automatic one at the player's bed or respawn anchor.
    Bed,
}

/// A waypoint. Equal waypoints are equal in every field but `id`.
#[derive(Clone, Debug)]
pub struct Waypoint {
    /// Which one it is while the game runs: a bubble or a click holds it by this, so a death or
    /// a deletion that moves the others in the list never redirects it. Not in the file.
    pub id: u64,
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
    /// Its beam is drawn in the world (flag 32 in the file means off).
    pub beam: bool,
    /// Deletes itself when the player reaches it (flag 64).
    pub temporary: bool,
    /// What it is drawn as: "" the default (a diamond; a cross for a death), `shape:<name>`,
    /// `item:<item id>` or `mob:<entity id>`. The file's eighth field, written only when set.
    pub icon: String,
}

impl PartialEq for Waypoint {
    fn eq(&self, o: &Self) -> bool {
        (&self.name, self.pos, self.color, &self.dim, self.kind, self.hidden, self.tracked, self.beam, self.temporary, &self.icon)
            == (&o.name, o.pos, o.color, &o.dim, o.kind, o.hidden, o.tracked, o.beam, o.temporary, &o.icon)
    }
}

impl Eq for Waypoint {}

impl Waypoint {
    pub fn new(name: impl Into<String>, pos: [i32; 3], color: u32, dim: impl Into<String>) -> Self {
        Self {
            id: fresh_id(),
            name: name.into(),
            pos,
            color,
            dim: dim.into(),
            kind: Kind::User,
            hidden: false,
            tracked: false,
            beam: true,
            temporary: false,
            icon: String::new(),
        }
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
        kind | u8::from(self.hidden) << 1
            | u8::from(self.tracked) << 4
            | u8::from(!self.beam) << 5
            | u8::from(self.temporary) << 6
    }
}

#[cfg(test)]
pub fn encode(list: &[Waypoint]) -> String {
    encode_keeping(list, &[])
}

/// The file, and after the waypoints the lines of the file read that were no waypoint (`kept`,
/// from [`decode_keeping`]), as they were: a line this version cannot read is never lost.
pub fn encode_keeping(list: &[Waypoint], kept: &[String]) -> String {
    let mut out = String::from(
        "# atlas waypoints: x y z colour flags dimension name [icon] (tab separated; flags: 1 death, 2 hidden, 4 spawn, 8 bed, 16 tracked, 32 no beam, 64 temporary)\n",
    );
    for w in list {
        let name = clean(&w.name);
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
        if !w.icon.is_empty() {
            // Before the line's end: a name holds no tab or line break (filtered above).
            out.pop();
            out.push_str(&format!("\t{}\n", w.icon));
        }
    }
    for line in kept {
        out.push_str(line);
        out.push('\n');
    }
    out
}

#[cfg(test)]
pub fn decode(text: &str) -> Vec<Waypoint> {
    decode_keeping(text).0
}

/// The waypoints of a file, and its other lines (but the header this mod writes, and empty
/// ones) to be written back as they are.
pub fn decode_keeping(text: &str) -> (Vec<Waypoint>, Vec<String>) {
    let (mut list, mut kept) = (Vec::new(), Vec::new());
    for l in text.lines() {
        match line(l) {
            Some(w) => list.push(w),
            None if l.trim().is_empty() || l.starts_with("# atlas waypoints:") || l.starts_with("# my_map waypoints:") => {}
            None => kept.push(l.trim_end_matches('\r').to_string()),
        }
    }
    (list, kept)
}

/// `pos` inside the limits a typed place has: a file's or a chat line's farther one is brought
/// in, so it can still be edited and saved.
pub fn clamp_pos(pos: [i32; 3]) -> [i32; 3] {
    [pos[0].clamp(-COORD_MAX, COORD_MAX), pos[1].clamp(-HEIGHT_MAX, HEIGHT_MAX), pos[2].clamp(-COORD_MAX, COORD_MAX)]
}

fn line(line: &str) -> Option<Waypoint> {
    if line.starts_with('#') {
        return None;
    }
    let mut f = line.splitn(8, '\t');
    let pos = clamp_pos([f.next()?.parse().ok()?, f.next()?.parse().ok()?, f.next()?.parse().ok()?]);
    let color = u32::from_str_radix(f.next()?, 16).ok()? & 0xFFFFFF;
    let flags: u8 = f.next()?.parse().unwrap_or(0);
    let dim = clean(f.next()?);
    let name = clean(f.next()?);
    let icon = f.next().unwrap_or("").trim().to_string();
    let kind = match flags & 13 {
        1 => Kind::Death,
        4 => Kind::Spawn,
        8 => Kind::Bed,
        _ => Kind::User,
    };
    Some(Waypoint {
        id: fresh_id(),
        name,
        pos,
        color,
        dim,
        kind,
        hidden: flags & 2 != 0,
        tracked: flags & 16 != 0,
        beam: flags & 32 == 0,
        temporary: flags & 64 != 0,
        icon,
    })
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
        // A Nether waypoint past an eighth of the border would lead past it: brought in.
        NETHER => {
            let out = |v: i32| v.saturating_mul(8).clamp(-COORD_MAX, COORD_MAX);
            Some(([out(pos[0]), pos[1].clamp(-HEIGHT_MAX, HEIGHT_MAX), out(pos[2])], OVERWORLD))
        }
        _ => None,
    }
}

/// The chat line that shares `w`: `[Atlas] name @ x y z dimension`.
pub fn share_line(w: &Waypoint) -> String {
    let name = clean(&w.name);
    let dim = clean(&w.dim);
    let dim = dim.strip_prefix("minecraft:").unwrap_or(&dim);
    format!("{SHARE_TAG} {name} @ {} {} {} {dim}", w.pos[0], w.pos[1], w.pos[2])
}

/// The waypoint a chat line shares, whatever stands before the tag (the sender's name). The
/// name may itself hold " @ ": the last one counts.
pub fn parse_share(line: &str) -> Option<Waypoint> {
    let rest = &line[line.find(SHARE_TAG)? + SHARE_TAG.len()..];
    let (name, place) = rest.rsplit_once(" @ ")?;
    let mut f = place.split_whitespace();
    let pos: [i32; 3] = [f.next()?.parse().ok()?, f.next()?.parse().ok()?, f.next()?.parse().ok()?];
    // As far as a typed place may be, no farther (a Link of it would wrap round).
    // (`unsigned_abs`: overflow wraps in the wasm build, and `i32::MIN.abs()` is negative.)
    if pos[0].unsigned_abs() > COORD_MAX as u32 || pos[2].unsigned_abs() > COORD_MAX as u32 || pos[1].unsigned_abs() > HEIGHT_MAX as u32 {
        return None;
    }
    let dim = f.next()?;
    let ok = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '.' | '-' | '/' | ':');
    if !dim.chars().all(ok) || dim.len() > 80 {
        return None;
    }
    let dim = if dim.contains(':') { dim.to_string() } else { format!("minecraft:{dim}") };
    let name: String = clean(name).trim().chars().take(NAME_MAX).collect();
    if name.is_empty() {
        return None;
    }
    Some(Waypoint::new(name, pos, COLORS[4], dim))
}

/// A waypoint shared in chat, waiting in the list, and who sent it (0: the server).
#[derive(Clone, Debug)]
pub struct Inbound {
    pub wp: Waypoint,
    pub sender: u128,
}

/// Puts `wp` from `sender` in the inbox: when the sender has `INBOX_PER_SENDER` waiting, or the
/// inbox is full, the oldest (of that sender's first) make room. The new one is always kept.
pub fn offer(inbox: &mut Vec<Inbound>, wp: Waypoint, sender: u128) {
    while inbox.iter().filter(|e| e.sender == sender).count() >= INBOX_PER_SENDER {
        let Some(i) = inbox.iter().position(|e| e.sender == sender) else { break };
        inbox.remove(i);
    }
    while inbox.len() >= INBOX_MAX {
        inbox.remove(0);
    }
    inbox.push(Inbound { wp, sender });
}

/// The highest a typed X or Z may be: the world border's reach.
pub const COORD_MAX: i32 = 30_000_000;
/// The highest a typed Y may be, either way.
pub const HEIGHT_MAX: i32 = 20_000;

/// A coordinate typed into the bubble's X, Y or Z field: a whole number, a minus sign at most
/// in front. None for anything else (empty, "-", letters, too big): it is never saved.
pub fn parse_field(text: &str, axis: usize) -> Option<i32> {
    let digits = text.strip_prefix('-').unwrap_or(text);
    if digits.is_empty() || digits.len() > 9 || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    let v: i32 = text.parse().ok()?;
    let max = if axis == 1 { HEIGHT_MAX } else { COORD_MAX };
    (v.abs() <= max).then_some(v)
}

/// Whether `c` may go into a coordinate field that holds `text` now.
pub fn field_takes(text: &str, c: char) -> bool {
    match c {
        '0'..='9' => text.trim_start_matches('-').len() < 9,
        '-' => text.is_empty(),
        _ => false,
    }
}

/// A coordinate of `/atlas add`: a number (a fraction is floored), `~` for the player's own,
/// `~5` / `~-3` relative to it.
fn command_coord(token: &str, base: f64, axis: usize) -> Option<i32> {
    let value = match token.strip_prefix('~') {
        Some("") => base.floor(),
        Some(off) => (base + off.parse::<f64>().ok().filter(|v| v.is_finite())?).floor(),
        None => token.parse::<f64>().ok().filter(|v| v.is_finite())?.floor(),
    };
    let max = f64::from(if axis == 1 { HEIGHT_MAX } else { COORD_MAX });
    (value.abs() <= max).then_some(value as i32)
}

/// A dimension as `/atlas add` takes it: `overworld`, `nether`, `end` (or their ids'
/// paths), or a full `namespace:path` id.
fn command_dim(token: &str) -> Option<String> {
    let token = token.to_ascii_lowercase();
    let full = match token.as_str() {
        "overworld" => OVERWORLD.to_string(),
        "nether" | "the_nether" => NETHER.to_string(),
        "end" | "the_end" => "minecraft:the_end".to_string(),
        id if id.contains(':') => id.to_string(),
        _ => return None,
    };
    let ok = |c: char| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '_' | '.' | '-' | '/' | ':');
    let (ns, path) = full.split_once(':')?;
    (!ns.is_empty() && !path.is_empty() && full.chars().all(ok) && full.len() <= 80).then_some(full)
}

pub const ADD_USAGE: &str = "/atlas add <name> <x> <y> <z> [dimension]";

/// `/atlas add`'s arguments: a name (several words, or in double quotes), X Y Z (numbers or
/// `~` relative to `at`, where the player is) and an optional dimension (else `dim`, the one
/// the player is in). The name, place and dimension, or what is wrong.
pub fn parse_add(args: &str, at: [f64; 3], dim: &str) -> Result<(String, [i32; 3], String), String> {
    let args = args.trim();
    let (name, rest): (String, Vec<&str>) = if let Some(quoted) = args.strip_prefix('"') {
        let (name, rest) = quoted.split_once('"').ok_or("The name's closing \" is missing")?;
        (name.to_string(), rest.split_whitespace().collect())
    } else {
        let words: Vec<&str> = args.split_whitespace().collect();
        // A last word that is no coordinate is the dimension.
        let n = words.len();
        let dimmed = n >= 5 && command_coord(words[n - 1], 0.0, 0).is_none();
        let coords = if dimmed { 4 } else { 3 };
        if n <= coords {
            return Err(format!("Usage: {ADD_USAGE}"));
        }
        (words[..n - coords].join(" "), words[n - coords..].to_vec())
    };
    let name: String = clean(&name).trim().chars().take(NAME_MAX).collect();
    if name.is_empty() {
        return Err(format!("A name is needed. Usage: {ADD_USAGE}"));
    }
    if rest.len() != 3 && rest.len() != 4 {
        return Err(format!("Usage: {ADD_USAGE}"));
    }
    let mut pos = [0; 3];
    for axis in 0..3 {
        pos[axis] =
            command_coord(rest[axis], at[axis], axis).ok_or_else(|| format!("Not a coordinate: {}", rest[axis]))?;
    }
    let dim = match rest.get(3) {
        Some(word) => {
            command_dim(word).ok_or_else(|| format!("Not a dimension: {word} (overworld, nether, end or an id)"))?
        }
        None => dim.to_string(),
    };
    Ok((name, pos, dim))
}

pub const TP_USAGE: &str = "/atlas tp <x> <y> <z> [dimension]";

/// The chat command that takes the player to `pos` in `dim`: a plain `/tp` in the dimension the
/// player is in, else one run `in` the other (the server moves the player there).
pub fn tp_command(pos: [i32; 3], dim: &str, here: &str) -> String {
    let at = format!("tp @s {}.5 {} {}.5", pos[0], pos[1], pos[2]);
    if dim == here { format!("/{at}") } else { format!("/execute in {dim} run {at}") }
}

/// `/atlas tp`'s arguments: X Y Z (numbers or `~`) and an optional dimension, as `/atlas add`
/// takes them, minus the name. The command to send, or what is wrong.
pub fn parse_tp(args: &str, at: [f64; 3], here: &str) -> Result<String, String> {
    let words: Vec<&str> = args.split_whitespace().collect();
    if words.len() != 3 && words.len() != 4 {
        return Err(format!("Usage: {TP_USAGE}"));
    }
    let mut pos = [0; 3];
    for axis in 0..3 {
        pos[axis] =
            command_coord(words[axis], at[axis], axis).ok_or_else(|| format!("Not a coordinate: {}", words[axis]))?;
    }
    let dim = match words.get(3) {
        Some(word) => {
            command_dim(word).ok_or_else(|| format!("Not a dimension: {word} (overworld, nether, end or an id)"))?
        }
        None => here.to_string(),
    };
    Ok(tp_command(pos, &dim, here))
}

/// Whether `name` is one `add_death` gave: "Death", "Death 2", ... (a renamed one keeps its name).
fn death_name(name: &str) -> bool {
    name == "Death" || name.strip_prefix("Death ").is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
}

/// Adds a death and keeps the newest `keep` of them, newest first in the list: "Death" the last
/// one, "Death 2" the one before, and so on. Returns how many old ones were dropped.
pub fn add_death(list: &mut Vec<Waypoint>, death: Waypoint, keep: usize) -> usize {
    let at = list.iter().position(|w| w.kind == Kind::Death).unwrap_or(list.len());
    list.insert(at, death);
    let before = list.len();
    let mut seen = 0;
    list.retain(|w| {
        if w.kind != Kind::Death {
            return true;
        }
        seen += 1;
        seen <= keep.max(1)
    });
    let mut n = 0;
    for w in list.iter_mut().filter(|w| w.kind == Kind::Death) {
        n += 1;
        if death_name(&w.name) {
            w.name = if n == 1 { "Death".into() } else { format!("Death {n}") };
        }
    }
    before - list.len()
}

/// How much of a beam shows `dist` blocks off (horizontally): none within `near` (0: always
/// shown), fading in over a quarter of that past it (two blocks at least); fading out over the
/// last quarter before `far`, none past it.
pub fn beam_alpha(dist: f64, near: f64, far: f64) -> f64 {
    let mut k = 1.0;
    if near > 0.0 {
        let band = (near * 0.25).max(2.0);
        k *= ((dist - near) / band).clamp(0.0, 1.0);
    }
    if far.is_finite() {
        k *= ((far - dist) / (far * 0.25)).clamp(0.0, 1.0);
    }
    // Smooth at both ends of the ramp.
    k * k * (3.0 - 2.0 * k)
}

/// Whether the player at `at` has reached `w`: within 3 blocks across and 6 up or down.
pub fn reached(w: &Waypoint, at: [f64; 3]) -> bool {
    let (dx, dz) = (f64::from(w.pos[0]) + 0.5 - at[0], f64::from(w.pos[2]) + 0.5 - at[2]);
    dx * dx + dz * dz <= 9.0 && (f64::from(w.pos[1]) - at[1]).abs() <= 6.0
}

/// Whether the player at `at` is well away from `w` (past 5 blocks): a temporary waypoint is
/// taken away on an arrival, never right where it was made.
pub fn away(w: &Waypoint, at: [f64; 3]) -> bool {
    let (dx, dz) = (f64::from(w.pos[0]) + 0.5 - at[0], f64::from(w.pos[2]) + 0.5 - at[2]);
    dx * dx + dz * dz > 25.0 || (f64::from(w.pos[1]) - at[1]).abs() > 10.0
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tp_commands() {
        let at = [10.7, 64.0, -3.2];
        assert_eq!(parse_tp("1 70 -2", at, OVERWORLD).unwrap(), "/tp @s 1.5 70 -2.5");
        assert_eq!(parse_tp("~ ~5 ~", at, OVERWORLD).unwrap(), "/tp @s 10.5 69 -4.5");
        assert_eq!(parse_tp("8 70 8 nether", at, OVERWORLD).unwrap(), format!("/execute in {NETHER} run tp @s 8.5 70 8.5"));
        assert_eq!(parse_tp("8 70 8 nether", at, NETHER).unwrap(), "/tp @s 8.5 70 8.5");
        assert!(parse_tp("1 2", at, OVERWORLD).is_err());
        assert!(parse_tp("1 2 x", at, OVERWORLD).is_err());
        assert!(parse_tp("1 2 3 mars", at, OVERWORLD).is_err());
    }

    #[test]
    fn the_file_keeps_every_field() {
        let mut hidden = Waypoint::new("tab\there", [1, 2, 3], 0xFFFFFF, "a:b");
        hidden.hidden = true;
        hidden.tracked = true;
        let mut quick = Waypoint::new("Monument", [400, 50, -900], 0x55FFFF, OVERWORLD);
        quick.beam = false;
        quick.temporary = true;
        quick.icon = "item:minecraft:prismarine_shard".into();
        let list = vec![
            quick,
            Waypoint::new("Home base", [-12, 64, 300], 0x55FF55, OVERWORLD),
            Waypoint::auto(Kind::Death, [0, -60, 0], NETHER),
            Waypoint::auto(Kind::Bed, [5, 70, 5], OVERWORLD),
            Waypoint::auto(Kind::Spawn, [0, 64, 0], OVERWORLD),
            hidden,
        ];
        let back = decode(&encode(&list));
        assert_eq!(back[..5], list[..5]);
        assert_eq!(back[5].name, "tabhere");
        assert!(back[5].hidden && back[5].tracked && back[5].kind == Kind::User && back[5].beam);
        assert!(back[5].icon.is_empty());
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

    #[test]
    fn typed_coordinates_are_numbers_or_nothing() {
        assert_eq!(parse_field("123", 0), Some(123));
        assert_eq!(parse_field("-64", 1), Some(-64));
        assert_eq!(parse_field("0", 2), Some(0));
        for bad in ["", "-", "--1", "1-", "12a", " 1", "1.5", "+5", "9999999999"] {
            assert_eq!(parse_field(bad, 0), None, "{bad:?}");
        }
        assert_eq!(parse_field("30000001", 0), None);
        assert_eq!(parse_field("20001", 1), None);
        assert!(field_takes("", '-') && !field_takes("1", '-') && !field_takes("-", '-'));
        assert!(field_takes("-12", '3') && !field_takes("12", 'x') && !field_takes("123456789", '0'));
    }

    #[test]
    fn the_add_command_reads_names_coordinates_and_dimensions() {
        let at = [100.7, 64.2, -20.5];
        let ok = |s: &str| parse_add(s, at, OVERWORLD).unwrap();
        assert_eq!(ok("Base 1 2 3"), ("Base".into(), [1, 2, 3], OVERWORLD.into()));
        assert_eq!(ok("My old base -100 70 250"), ("My old base".into(), [-100, 70, 250], OVERWORLD.into()));
        assert_eq!(ok("Hub 10 64 10 nether"), ("Hub".into(), [10, 64, 10], NETHER.into()));
        assert_eq!(ok("Far Lands 10 64 10 the_end").2, "minecraft:the_end");
        assert_eq!(ok("x 1 2 3 mymod:moon").2, "mymod:moon");
        assert_eq!(ok("\"Base 7\" ~ ~ ~"), ("Base 7".into(), [100, 64, -21], OVERWORLD.into()));
        assert_eq!(ok("Here ~10 ~-4 ~0.6"), ("Here".into(), [110, 60, -20], OVERWORLD.into()));
        assert_eq!(ok("\"1 2 3\" 4 5 6 overworld").0, "1 2 3");
        assert_eq!(ok("Spot 1.9 -0.5 3").1, [1, -1, 3]);
        assert!(parse_add("1 2 3", at, OVERWORLD).is_err(), "no name");
        assert!(parse_add("Base 1 2", at, OVERWORLD).is_err());
        assert!(parse_add("Base 1 2 x", at, OVERWORLD).is_err());
        assert!(parse_add("Base 1 2 3 Mars", at, OVERWORLD).is_err(), "not a dimension");
        assert!(parse_add("\"Base 1 2 3", at, OVERWORLD).is_err());
        assert!(parse_add("\"\" 1 2 3", at, OVERWORLD).is_err());
        assert!(parse_add("Base 99999999 2 3", at, OVERWORLD).is_err());
        assert_eq!(ok(&format!("{} 1 2 3", "n".repeat(50))).0.len(), NAME_MAX);
    }

    #[test]
    fn deaths_are_kept_newest_first() {
        let home = Waypoint::new("Home", [0, 64, 0], 0x55FF55, OVERWORLD);
        let mut list = vec![home.clone()];
        let death = |x| Waypoint::auto(Kind::Death, [x, 64, 0], OVERWORLD);
        assert_eq!(add_death(&mut list, death(1), 3), 0);
        add_death(&mut list, death(2), 3);
        // A renamed one keeps its name.
        list[1].name = "Lava pit".into();
        add_death(&mut list, death(3), 3);
        let deaths: Vec<(&str, i32)> =
            list.iter().filter(|w| w.kind == Kind::Death).map(|w| (w.name.as_str(), w.pos[0])).collect();
        assert_eq!(deaths, [("Death", 3), ("Lava pit", 2), ("Death 3", 1)]);
        assert_eq!(add_death(&mut list, death(4), 3), 1);
        let xs: Vec<i32> = list.iter().filter(|w| w.kind == Kind::Death).map(|w| w.pos[0]).collect();
        assert_eq!(xs, [4, 3, 2]);
        assert_eq!(list[0], home);
        assert_eq!(add_death(&mut list, death(5), 1), 3);
        assert_eq!(list.len(), 2);
        assert_eq!((list[1].name.as_str(), list[1].pos[0]), ("Death", 5));
        // A file of 0.3 with its one death reads and rotates on.
        let mut old = decode("0\t-60\t0\tE02020\t1\tminecraft:the_nether\tDeath\n");
        add_death(&mut old, death(9), 3);
        assert_eq!(old.iter().map(|w| w.name.as_str()).collect::<Vec<_>>(), ["Death", "Death 2"]);
        assert_eq!(old[1].dim, NETHER);
    }

    #[test]
    fn ids_tell_waypoints_apart_but_not_equal_ones() {
        let a = Waypoint::new("A", [1, 2, 3], 0, OVERWORLD);
        let b = Waypoint::new("A", [1, 2, 3], 0, OVERWORLD);
        assert_ne!(a.id, b.id);
        assert_eq!(a, b);
        let back = decode(&encode(std::slice::from_ref(&a)));
        assert_ne!(back[0].id, a.id, "every read gives new ids");
    }

    #[test]
    fn lines_this_version_cannot_read_are_kept() {
        let text = "# atlas waypoints: header\n1\t2\t3\tFFFFFF\t0\tminecraft:overworld\tHome\n\
                    # my own note\nfuture\tformat\tline\n\n";
        let (list, kept) = decode_keeping(text);
        assert_eq!(list.len(), 1);
        assert_eq!(kept, ["# my own note", "future\tformat\tline"]);
        let again = encode_keeping(&list, &kept);
        assert!(again.ends_with("# my own note\nfuture\tformat\tline\n"), "{again}");
        assert_eq!(decode_keeping(&again), (list, kept));
    }

    #[test]
    fn no_section_sign_reaches_a_shared_line() {
        let w = parse_share("<Eve> [Atlas] \u{a7}kBad\u{a7}r\u{7f} name @ 1 2 3 overworld").unwrap();
        assert_eq!(w.name, "kBadr name");
        let mut f = decode("1\t2\t3\tFFFFFF\t0\tminecraft:over\u{a7}world\tA\u{a7}cB\n");
        assert_eq!(f[0].name, "AcB");
        f[0].name = "x\u{a7}y".into();
        let line = share_line(&f[0]);
        assert!(line.chars().all(allowed_char), "{line}");
        assert!(parse_add("\u{a7}4Red 1 2 3", [0.0; 3], OVERWORLD).unwrap().0 == "4Red");
        assert!(encode(&f).lines().all(|l| l.chars().all(|c| c == '\t' || allowed_char(c))));
    }

    #[test]
    fn shared_coordinates_stay_inside_the_world() {
        assert!(parse_share("[Atlas] x @ 30000000 64 -30000000 overworld").is_some());
        for bad in ["2147483647 64 0", "-2147483648 64 0", "0 20001 0", "30000001 0 0", "0 0 -2147483648"] {
            assert!(parse_share(&format!("[Atlas] x @ {bad} overworld")).is_none(), "{bad}");
        }
        assert_eq!(linked([i32::MAX, 0, i32::MIN], NETHER).unwrap().0, [COORD_MAX, 0, -COORD_MAX]);
        assert_eq!(linked([4_000_000, 64, -3], NETHER).unwrap().0, [COORD_MAX, 64, -24]);
        // A file's farther place is brought in: the bubble can save it again.
        let far = decode("2147483647\t-99999\t-2147483648\tFFFFFF\t0\tminecraft:overworld\tFar\n");
        assert_eq!(far[0].pos, [COORD_MAX, -HEIGHT_MAX, -COORD_MAX]);
        assert_eq!(parse_field(&far[0].pos[0].to_string(), 0), Some(COORD_MAX));
    }

    #[test]
    fn the_inbox_drops_the_oldest_and_caps_each_sender() {
        let wp = |x| Waypoint::new("w", [x, 0, 0], 0, OVERWORLD);
        let mut inbox = Vec::new();
        for x in 0..8 {
            offer(&mut inbox, wp(x), 7);
        }
        let xs: Vec<i32> = inbox.iter().map(|e| e.wp.pos[0]).collect();
        assert_eq!(xs, [3, 4, 5, 6, 7], "one sender keeps its newest {INBOX_PER_SENDER}");
        for s in 0..30u128 {
            offer(&mut inbox, wp(100 + s as i32), 1000 + s);
        }
        assert_eq!(inbox.len(), INBOX_MAX);
        assert_eq!(inbox.last().unwrap().wp.pos[0], 129, "the newest is kept");
        assert_eq!(inbox[0].wp.pos[0], 110, "the oldest went");
    }

    #[test]
    fn beams_fade_near_and_far() {
        assert_eq!(beam_alpha(5.0, 0.0, f64::INFINITY), 1.0);
        assert_eq!(beam_alpha(10.0, 16.0, f64::INFINITY), 0.0);
        assert_eq!(beam_alpha(16.0, 16.0, f64::INFINITY), 0.0);
        let mid = beam_alpha(18.0, 16.0, f64::INFINITY);
        assert!(mid > 0.3 && mid < 0.7, "{mid}");
        assert_eq!(beam_alpha(20.0, 16.0, f64::INFINITY), 1.0);
        assert_eq!(beam_alpha(700.0, 16.0, 1000.0), 1.0);
        assert!(beam_alpha(900.0, 16.0, 1000.0) < 0.5);
        assert_eq!(beam_alpha(1000.0, 16.0, 1000.0), 0.0);
        let small = beam_alpha(9.0, 8.0, f64::INFINITY);
        assert!(small > 0.0 && small < 1.0, "a small near distance still fades over two blocks");
    }

    #[test]
    fn arriving_takes_a_temporary_waypoint() {
        let w = Waypoint::new("Monument", [100, 60, 100], 0, OVERWORLD);
        assert!(reached(&w, [102.0, 63.0, 101.0]));
        assert!(!reached(&w, [104.0, 60.0, 100.5]));
        assert!(!reached(&w, [100.5, 80.0, 100.5]), "far above");
        assert!(!away(&w, [104.0, 60.0, 100.5]) && away(&w, [110.0, 60.0, 100.0]));
    }
}
