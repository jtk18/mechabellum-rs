//! Board code: a small forum-style markup for sharing a board layout, and for setting one up in
//! Testing Grounds. See docs/board-code.md.
//!
//! ```text
//! [board version=2324 map="Railway Town" round=5]
//! [side name="jtk18" hp=4500]
//! [unit=Crawler at=-12.5,-20 level=2 techs="Acid Crawler, Flame Crawler"]
//! [unit=Fortress at=0,-25 rotated items="Heavy Armor"]
//! [/side]
//! [side name="Opponent"]
//! [unit=Marksman at=3,22]
//! [/side]
//! [/board]
//! ```
//!
//! Text around the `[board]` block is ignored, so a code can sit inside a forum post or chat
//! message. Everything else is strict: unknown tags, attributes, and units are errors, and every
//! value is bounded, since codes come from strangers.

use crate::{GameNames, MatchDetail, NameKind};

use serde::Serialize;

use std::collections::BTreeMap;
use std::fmt;

/// Longest code accepted, in bytes.
pub const MAX_INPUT: usize = 64 * 1024;
const MAX_SIDES: usize = 8;
const MAX_UNITS_PER_SIDE: usize = 300;
const MAX_BUILDINGS_PER_SIDE: usize = 50;
const MAX_LIST: usize = 16;
const MAX_TEXT: usize = 80;
const MAX_LEVEL: u32 = 20;
/// Farthest a unit center may be from the middle of the board, in replay units (10 per square).
const MAX_COORD: i32 = 1000;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Board {
    pub version: Option<u32>,
    pub map: Option<String>,
    pub round: Option<u32>,
    pub sides: Vec<Side>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Side {
    pub name: Option<String>,
    pub hp: Option<i32>,
    /// Which half of the board the side deploys on. Needed for anchored positions.
    pub half: Option<Half>,
    pub units: Vec<BoardUnit>,
    /// Buildings the side has put up: walls, cannons, barricades.
    pub buildings: Vec<BoardBuilding>,
    /// Devices: Shield Generator, Sentry Missile, Missile Interceptor. Same shape as buildings;
    /// `building_id` holds the device's ID.
    pub devices: Vec<BoardBuilding>,
}

/// A building or a device: something placed that isn't a unit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BoardBuilding {
    /// A `ConstructionData` ID, or for a device its `...ContraptionData` ID.
    pub building_id: u32,
    /// The name as the game shows it.
    pub name: String,
    /// Center, in replay units (10 per grid square).
    pub x: i32,
    pub y: i32,
}

/// A half of the board. In replays, the south half has negative y.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Half {
    South,
    North,
}

impl Half {
    fn name(self) -> &'static str {
        match self {
            Half::South => "south",
            Half::North => "north",
        }
    }
}

// Deployment geometry, the same on every 2.0 map measured (82 games, 5 maps), in replay units
// and a side's own frame: x to the side's right, y toward the middle ("forward"). Each side's
// zone is 60 squares wide and 30 deep, ending 1 square short of the middle. From round 3,
// flank columns 6 squares wide open along both edges of the opponent's half. Units in a flank
// column stand on end: there `rotated = false` means turned 90° from the main zone (seen in
// play, and the only way units at x = ±310 fit inside a 300-360 column).
const ZONE_HALF_WIDTH: i32 = 300;
const ZONE_FRONT: i32 = -10;
const ZONE_BACK: i32 = -310;
const FLANK_CENTER_X: i32 = 330;
const FLANK_NEAR: i32 = 10;
const FLANK_OPENS_ROUND: u32 = 3;
/// Command Center and Research Center: the same place on every 2.0 map, mirrored left and right.
const TOWER_X: i32 = 140;
const TOWER_Y: i32 = -175;

/// A named point on a side's half. `inward` says which way the unit extends from it, so a unit
/// anchored to a corner sits flush in the corner rather than centered on it.
struct Anchor {
    x: i32,
    y: i32,
    inward: (i32, i32),
}

/// Anchor names, for error messages.
const ANCHORS: &str = "center, front, back, left, right, front-left, front-right, back-left, \
back-right, left-flank, right-flank, command-center, research-center";

fn anchor(name: &str) -> Option<Anchor> {
    let mid_y = (ZONE_FRONT + ZONE_BACK) / 2;
    let (x, y, inward) = match name {
        "center" => (0, 0, (0, 0)),
        "front" => (0, ZONE_FRONT, (0, -1)),
        "back" => (0, ZONE_BACK, (0, 1)),
        "left" => (-ZONE_HALF_WIDTH, mid_y, (1, 0)),
        "right" => (ZONE_HALF_WIDTH, mid_y, (-1, 0)),
        "front-left" => (-ZONE_HALF_WIDTH, ZONE_FRONT, (1, -1)),
        "front-right" => (ZONE_HALF_WIDTH, ZONE_FRONT, (-1, -1)),
        "back-left" => (-ZONE_HALF_WIDTH, ZONE_BACK, (1, 1)),
        "back-right" => (ZONE_HALF_WIDTH, ZONE_BACK, (-1, 1)),
        // The near end of a flank column, just past the middle.
        "left-flank" => (-FLANK_CENTER_X, FLANK_NEAR, (0, 1)),
        "right-flank" => (FLANK_CENTER_X, FLANK_NEAR, (0, 1)),
        // Tower centers; a unit is centered on the offset from them, since the tower itself
        // takes the spot. Measured in Testing Grounds on every 2.0 map (2026-09-28), within
        // half a square.
        "command-center" => (-TOWER_X, TOWER_Y, (0, 0)),
        "research-center" => (TOWER_X, TOWER_Y, (0, 0)),
        _ => return None,
    };
    Some(Anchor { x, y, inward })
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BoardUnit {
    pub unit_id: u32,
    /// The unit's name as the game shows it.
    pub name: String,
    /// Center, in replay units (10 per grid square), as replays store it.
    pub x: i32,
    pub y: i32,
    /// 1-based, as the game shows it.
    pub level: u32,
    pub rotated: bool,
    pub techs: Vec<String>,
    pub items: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ParseError {
    /// 1-based position in the input.
    pub line: usize,
    pub column: usize,
    pub message: String,
}

impl fmt::Display for ParseError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "line {}, column {}: {}",
            self.line, self.column, self.message
        )
    }
}

impl std::error::Error for ParseError {}

/// One `[tag ...]` or `[/tag]`.
#[derive(Debug)]
struct Tag<'a> {
    at: usize,
    name: String,
    closing: bool,
    /// `[tag=value]`
    value: Option<&'a str>,
    attrs: Vec<(String, Option<&'a str>, usize)>,
}

struct Parser<'a> {
    text: &'a str,
    pos: usize,
}

impl<'a> Parser<'a> {
    fn error(&self, at: usize, message: impl Into<String>) -> ParseError {
        let before = &self.text[..at.min(self.text.len())];
        let line = before.matches('\n').count() + 1;
        let column = before.rsplit('\n').next().map_or(0, |l| l.chars().count()) + 1;
        ParseError {
            line,
            column,
            message: message.into(),
        }
    }

    fn peek(&self) -> Option<char> {
        self.text[self.pos..].chars().next()
    }

    fn skip_space(&mut self) {
        while let Some(c) = self.peek() {
            if !c.is_whitespace() {
                break;
            }
            self.pos += c.len_utf8();
        }
    }

    fn word(&mut self) -> &'a str {
        let start = self.pos;
        while let Some(c) = self.peek() {
            if !(c.is_ascii_alphanumeric() || c == '_' || c == '-') {
                break;
            }
            self.pos += c.len_utf8();
        }
        &self.text[start..self.pos]
    }

    /// A bare value (up to whitespace or `]`) or a "quoted" one.
    fn value(&mut self) -> Result<&'a str, ParseError> {
        if self.peek() == Some('"') {
            let start = self.pos + 1;
            let Some(len) = self.text[start..].find(['"', '\n']) else {
                return Err(self.error(self.pos, "unclosed quote"));
            };
            if self.text[start + len..].starts_with('\n') {
                return Err(self.error(self.pos, "unclosed quote"));
            }
            self.pos = start + len + 1;
            return Ok(&self.text[start..start + len]);
        }
        let start = self.pos;
        while let Some(c) = self.peek() {
            if c.is_whitespace() || c == ']' || c == '[' {
                break;
            }
            self.pos += c.len_utf8();
        }
        if self.pos == start {
            return Err(self.error(start, "expected a value"));
        }
        Ok(&self.text[start..self.pos])
    }

    /// The next tag; only whitespace may come before it.
    fn tag(&mut self) -> Result<Tag<'a>, ParseError> {
        self.skip_space();
        let at = self.pos;
        match self.peek() {
            Some('[') => self.pos += 1,
            Some(_) => {
                return Err(self.error(at, "expected a [tag]; text between tags isn't allowed"))
            }
            None => return Err(self.error(at, "the code ends before [/board]")),
        }
        let closing = self.peek() == Some('/');
        if closing {
            self.pos += 1;
        }
        let name = self.word().to_ascii_lowercase();
        if name.is_empty() {
            return Err(self.error(self.pos, "expected a tag name"));
        }
        let mut tag = Tag {
            at,
            name,
            closing,
            value: None,
            attrs: Vec::new(),
        };
        if !closing && self.peek() == Some('=') {
            self.pos += 1;
            tag.value = Some(self.value()?);
        }
        loop {
            self.skip_space();
            match self.peek() {
                Some(']') => {
                    self.pos += 1;
                    return Ok(tag);
                }
                None => return Err(self.error(at, "unclosed tag")),
                Some(_) if closing => return Err(self.error(self.pos, "expected ]")),
                Some(_) => {
                    let key_at = self.pos;
                    let key = self.word().to_ascii_lowercase();
                    if key.is_empty() {
                        return Err(self.error(key_at, "expected an attribute name"));
                    }
                    let value = if self.peek() == Some('=') {
                        self.pos += 1;
                        Some(self.value()?)
                    } else {
                        None
                    };
                    if tag.attrs.iter().any(|(k, _, _)| *k == key) {
                        return Err(self.error(key_at, format!("{key} is given twice")));
                    }
                    tag.attrs.push((key, value, key_at));
                }
            }
        }
    }
}

fn text_value(p: &Parser, value: &str, at: usize) -> Result<String, ParseError> {
    if value.chars().count() > MAX_TEXT {
        return Err(p.error(at, format!("longer than {MAX_TEXT} characters")));
    }
    if value.chars().any(|c| c.is_control()) {
        return Err(p.error(at, "control characters aren't allowed"));
    }
    Ok(value.trim().to_owned())
}

fn number<T: std::str::FromStr>(p: &Parser, value: &str, at: usize) -> Result<T, ParseError> {
    value
        .parse()
        .map_err(|_| p.error(at, format!("{value:?} isn't a valid number here")))
}

/// Squares, with halves allowed ("12.5"), to replay units.
fn coordinate(p: &Parser, value: &str, at: usize) -> Result<i32, ParseError> {
    let bad = || {
        p.error(
            at,
            format!("{value:?} isn't a coordinate (squares, to one decimal place)"),
        )
    };
    // Replays store tenths of a square; units sit on halves, devices anywhere.
    let (whole, half) = match value.split_once('.') {
        Some((w, "")) => (w, 0),
        Some((w, d)) if d.len() == 1 => (w, d.parse::<i32>().map_err(|_| bad())?),
        Some(_) => return Err(bad()),
        None => (value, 0),
    };
    let negative = whole.starts_with('-');
    let squares: i32 = match whole {
        "-" | "" if half > 0 => 0,
        _ => whole.parse().map_err(|_| bad())?,
    };
    let units = squares.checked_mul(10).ok_or_else(bad)?;
    let units = if negative { units - half } else { units + half };
    if units.abs() > MAX_COORD {
        return Err(p.error(at, "that position is off the board"));
    }
    Ok(units)
}

fn list(p: &Parser, value: &str, at: usize) -> Result<Vec<String>, ParseError> {
    let items: Vec<String> = value
        .split([',', ';'])
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|s| text_value(p, s, at))
        .collect::<Result<_, _>>()?;
    if items.len() > MAX_LIST {
        return Err(p.error(at, format!("more than {MAX_LIST} entries")));
    }
    Ok(items)
}

/// Unit names (as the game shows them, case and spacing ignored) and IDs. Where a name has
/// several IDs (Star Expedition copies), the regular unit's (lowest) ID wins.
fn unit_lookup() -> BTreeMap<String, (u32, String)> {
    name_lookup(NameKind::Unit)
}

/// Names and IDs of one kind of thing; where a name has several IDs, the lowest wins.
fn name_lookup(kind: NameKind) -> BTreeMap<String, (u32, String)> {
    let names = GameNames::bundled();
    let mut map = BTreeMap::new();
    for id in names.ids(kind) {
        if let Some(name) = names.resolve(kind, id) {
            map.entry(normalize(name))
                .or_insert((id as u32, name.to_owned()));
        }
    }
    map
}

/// Item IDs that go by each name (case and spacing ignored). Several names cover more than one
/// item: Heavy Armor comes in two ranks, and rank 1 alone has two Haste Modules.
fn item_ids_by_name() -> BTreeMap<String, Vec<u32>> {
    let names = GameNames::bundled();
    let mut map: BTreeMap<String, Vec<u32>> = BTreeMap::new();
    for id in names.ids(NameKind::Equipment) {
        if let (Some(name), Ok(id)) = (names.resolve(NameKind::Equipment, id), u32::try_from(id)) {
            map.entry(normalize(name)).or_default().push(id);
        }
    }
    map
}

/// How a code writes an item: its name, with ` #ID` added when the name alone would be
/// ambiguous.
fn item_entry(id: u32, name: &str) -> String {
    let shared = item_ids_by_name()
        .get(&normalize(name))
        .is_some_and(|ids| ids.len() > 1);
    if shared {
        format!("{name} #{id}")
    } else {
        name.to_owned()
    }
}

/// The item ID an `items` entry means: a name, `Name #ID`, or `#ID`. A name several items share
/// needs the ID; the error lists them.
pub fn resolve_item(entry: &str) -> Result<u32, String> {
    let names = GameNames::bundled();
    let known = |id: u32| names.resolve(NameKind::Equipment, u64::from(id));
    let entry = entry.trim();
    if let Some((name, id)) = entry.rsplit_once('#') {
        let id: u32 = id
            .trim()
            .parse()
            .map_err(|_| format!("{entry:?}: after # comes the item's ID"))?;
        let actual = known(id).ok_or_else(|| format!("{entry:?}: no item has ID {id}"))?;
        let name = name.trim();
        if !name.is_empty() && normalize(name) != normalize(actual) {
            return Err(format!("{entry:?}: item {id} is {actual}, not {name}"));
        }
        return Ok(id);
    }
    match item_ids_by_name().get(&normalize(entry)).map(Vec::as_slice) {
        Some([id]) => Ok(*id),
        Some(ids) => Err(format!(
            "{entry} is {} different items; add the ID: {}",
            ids.len(),
            ids.iter()
                .map(|id| format!("{entry} #{id}"))
                .collect::<Vec<_>>()
                .join(", ")
        )),
        None => Err(format!("unknown item {entry:?}")),
    }
}

fn normalize(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(char::to_lowercase)
        .collect()
}

impl Board {
    /// Find and read the first `[board]...[/board]` block in `text`.
    pub fn parse(text: &str) -> Result<Board, ParseError> {
        let mut p = Parser { text, pos: 0 };
        if text.len() > MAX_INPUT {
            return Err(p.error(0, format!("longer than {MAX_INPUT} bytes")));
        }
        let lower = text.to_ascii_lowercase();
        let Some(start) = lower.find("[board") else {
            return Err(p.error(0, "no [board] block found"));
        };
        p.pos = start;
        let units = unit_lookup();
        let buildings = name_lookup(NameKind::Construction);
        let devices = name_lookup(NameKind::Device);

        let open = p.tag()?;
        if open.name != "board" || open.closing {
            return Err(p.error(open.at, "expected [board]"));
        }
        let mut board = Board {
            version: None,
            map: None,
            round: None,
            sides: Vec::new(),
        };
        for (key, value, at) in &open.attrs {
            let value = value.ok_or_else(|| p.error(*at, format!("{key} needs a value")))?;
            match key.as_str() {
                "version" => board.version = Some(number(&p, value, *at)?),
                "map" => board.map = Some(text_value(&p, value, *at)?),
                "round" => board.round = Some(number(&p, value, *at)?),
                _ => return Err(p.error(*at, format!("[board] has no {key} attribute"))),
            }
        }

        let mut side: Option<Side> = None;
        loop {
            let tag = p.tag()?;
            match (tag.name.as_str(), tag.closing, side.as_mut()) {
                ("board", true, None) => break,
                ("board", true, Some(_)) => return Err(p.error(tag.at, "[/side] is missing")),
                ("side", false, None) => {
                    if board.sides.len() == MAX_SIDES {
                        return Err(p.error(tag.at, format!("more than {MAX_SIDES} sides")));
                    }
                    let mut s = Side {
                        name: None,
                        hp: None,
                        half: None,
                        units: Vec::new(),
                        buildings: Vec::new(),
                        devices: Vec::new(),
                    };
                    if let Some(value) = tag.value {
                        s.name = Some(text_value(&p, value, tag.at)?);
                    }
                    for (key, value, at) in &tag.attrs {
                        let value =
                            value.ok_or_else(|| p.error(*at, format!("{key} needs a value")))?;
                        match key.as_str() {
                            "name" => s.name = Some(text_value(&p, value, *at)?),
                            "hp" => s.hp = Some(number(&p, value, *at)?),
                            "half" => {
                                s.half = Some(match value.to_ascii_lowercase().as_str() {
                                    "south" | "bottom" => Half::South,
                                    "north" | "top" => Half::North,
                                    _ => return Err(p.error(*at, "half is south or north")),
                                })
                            }
                            _ => return Err(p.error(*at, format!("[side] has no {key} attribute"))),
                        }
                    }
                    side = Some(s);
                }
                ("side", true, Some(_)) => board.sides.push(side.take().unwrap()),
                ("unit", false, Some(s)) => {
                    if s.units.len() == MAX_UNITS_PER_SIDE {
                        return Err(
                            p.error(tag.at, format!("more than {MAX_UNITS_PER_SIDE} units"))
                        );
                    }
                    let (half, round) = (s.half, board.round);
                    s.units.push(parse_unit(&p, &tag, &units, half, round)?);
                }
                ("unit", false, None) => {
                    return Err(p.error(tag.at, "a [unit] must be inside a [side]"))
                }
                ("building", false, Some(s)) => {
                    if s.buildings.len() == MAX_BUILDINGS_PER_SIDE {
                        return Err(p.error(
                            tag.at,
                            format!("more than {MAX_BUILDINGS_PER_SIDE} buildings"),
                        ));
                    }
                    s.buildings.push(parse_structure(
                        &p,
                        &tag,
                        NameKind::Construction,
                        "building",
                        &buildings,
                    )?);
                }
                ("device", false, Some(s)) => {
                    if s.devices.len() == MAX_BUILDINGS_PER_SIDE {
                        return Err(p.error(
                            tag.at,
                            format!("more than {MAX_BUILDINGS_PER_SIDE} devices"),
                        ));
                    }
                    s.devices.push(parse_structure(
                        &p,
                        &tag,
                        NameKind::Device,
                        "device",
                        &devices,
                    )?);
                }
                ("building" | "device", false, None) => {
                    return Err(p.error(tag.at, format!("a [{}] must be inside a [side]", tag.name)))
                }
                ("side", false, Some(_)) => {
                    return Err(p.error(tag.at, "close the previous side with [/side] first"))
                }
                (name, closing, _) => {
                    let slash = if closing { "/" } else { "" };
                    return Err(p.error(tag.at, format!("unexpected [{slash}{name}]")));
                }
            }
        }
        Ok(board)
    }

    /// The board as it stood before round `round_index`'s fight (0-based into `detail.rounds`).
    pub fn from_round(detail: &MatchDetail, round_index: usize) -> Option<Board> {
        let round = detail.rounds.get(round_index)?;
        let sides = round
            .sides
            .iter()
            .zip(&detail.players)
            .filter_map(|(side, player)| {
                let side = side.as_ref()?;
                let units = side
                    .units
                    .iter()
                    .map(|u| BoardUnit {
                        unit_id: u.unit_id,
                        name: u.name.clone(),
                        x: u.x,
                        y: u.y,
                        level: u.level + 1,
                        rotated: u.rotated,
                        techs: side
                            .enabled_techs
                            .iter()
                            .find(|t| t.unit_id == u.unit_id)
                            .map(|t| {
                                t.techs
                                    .iter()
                                    .map(|t| t.name.clone().unwrap_or_else(|| t.id.clone()))
                                    .collect()
                            })
                            .unwrap_or_default(),
                        items: u
                            .equipment
                            .iter()
                            .map(|e| match (&e.name, e.id.trim().parse::<u32>()) {
                                (Some(name), Ok(id)) => item_entry(id, name),
                                (Some(name), Err(_)) => name.clone(),
                                (None, _) => format!("#{}", e.id.trim()),
                            })
                            .collect(),
                    })
                    .collect();
                let mut ys: Vec<i32> = side.units.iter().map(|u| u.y).collect();
                ys.sort_unstable();
                let half = ys
                    .get(ys.len() / 2)
                    .map(|&y| if y < 0 { Half::South } else { Half::North });
                let placed = |list: &[crate::PlacedBuilding]| -> Vec<BoardBuilding> {
                    list.iter()
                        .map(|b| BoardBuilding {
                            building_id: b.id,
                            name: b.name.clone().unwrap_or_else(|| b.id.to_string()),
                            x: b.x,
                            y: b.y,
                        })
                        .collect()
                };
                Some(Side {
                    name: Some(player.name.clone()),
                    hp: Some(side.hp),
                    half,
                    units,
                    buildings: placed(&side.buildings),
                    devices: placed(&side.devices),
                })
            })
            .collect();
        Some(Board {
            version: Some(detail.version),
            map: detail.map_name.clone(),
            // Rounds as the app counts them, from 1 (replays number some modes from 0).
            round: Some(round_index as u32 + 1),
            sides,
        })
    }
}

fn parse_unit(
    p: &Parser,
    tag: &Tag,
    units: &BTreeMap<String, (u32, String)>,
    half: Option<Half>,
    round: Option<u32>,
) -> Result<BoardUnit, ParseError> {
    let name = tag
        .value
        .ok_or_else(|| p.error(tag.at, "say which unit: [unit=Crawler ...]"))?;
    let (unit_id, name) = match name.parse::<u32>() {
        Ok(id) => units
            .values()
            .find(|(i, _)| *i == id)
            .cloned()
            .ok_or_else(|| p.error(tag.at, format!("unknown unit ID {id}")))?,
        Err(_) => units
            .get(&normalize(name))
            .cloned()
            .ok_or_else(|| p.error(tag.at, format!("unknown unit {name:?}")))?,
    };
    let mut unit = BoardUnit {
        unit_id,
        name,
        x: 0,
        y: 0,
        level: 1,
        rotated: false,
        techs: Vec::new(),
        items: Vec::new(),
    };
    let mut placed = false;
    let mut from: Option<(Anchor, usize)> = None;
    // Squares right and forward of the anchor, in the side's own view.
    let (mut right, mut forward) = (0, 0);
    for (key, value, at) in &tag.attrs {
        let at = *at;
        let need = || value.ok_or_else(|| p.error(at, format!("{key} needs a value")));
        match key.as_str() {
            "at" => {
                let v = need()?;
                let (x, y) = v
                    .split_once(',')
                    .ok_or_else(|| p.error(at, "at needs x,y in squares, e.g. at=-12.5,20"))?;
                unit.x = coordinate(p, x.trim(), at)?;
                unit.y = coordinate(p, y.trim(), at)?;
                placed = true;
            }
            "from" => {
                let name = need()?.to_ascii_lowercase();
                let a = anchor(&name).ok_or_else(|| {
                    p.error(at, format!("unknown anchor {name:?}; use one of {ANCHORS}"))
                })?;
                if name.ends_with("-flank") && round.is_some_and(|r| r < FLANK_OPENS_ROUND) {
                    return Err(p.error(
                        at,
                        format!("flank columns open in round {FLANK_OPENS_ROUND}"),
                    ));
                }
                from = Some((a, at));
            }
            "right" => right += coordinate(p, need()?, at)?,
            "left" => right -= coordinate(p, need()?, at)?,
            "forward" => forward += coordinate(p, need()?, at)?,
            "back" => forward -= coordinate(p, need()?, at)?,
            "level" => {
                unit.level = number(p, need()?, at)?;
                if !(1..=MAX_LEVEL).contains(&unit.level) {
                    return Err(p.error(at, format!("level must be 1 to {MAX_LEVEL}")));
                }
            }
            "techs" => unit.techs = list(p, need()?, at)?,
            "items" => unit.items = list(p, need()?, at)?,
            "rotated" => {
                if value.is_some() {
                    return Err(p.error(at, "rotated takes no value"));
                }
                unit.rotated = true;
            }
            _ => return Err(p.error(at, format!("[unit] has no {key} attribute"))),
        }
    }
    match (placed, from) {
        (true, Some((_, at))) => {
            return Err(p.error(at, "give either at=x,y or from=anchor, not both"))
        }
        (true, None) if right != 0 || forward != 0 => {
            return Err(p.error(tag.at, "left, right, forward, and back need from=anchor"))
        }
        (true, None) => {}
        (false, None) => {
            return Err(p.error(
                tag.at,
                "every unit needs a position: at=x,y, or from=anchor with left/right/forward/back",
            ))
        }
        (false, Some((a, at))) => {
            let half = half.ok_or_else(|| {
                p.error(
                    at,
                    "anchors need the side's half: [side half=south] or half=north",
                )
            })?;
            // Half the unit's size, so it lines up against the anchor's edges.
            let (w, d) = crate::footprint(unit.unit_id).unwrap_or((2, 2));
            let (w, d) = if unit.rotated { (d, w) } else { (w, d) };
            let x = a.x + a.inward.0 * i32::from(w) * 5 + right;
            let y = a.y + a.inward.1 * i32::from(d) * 5 + forward;
            (unit.x, unit.y) = match half {
                Half::South => (x, y),
                Half::North => (-x, -y),
            };
            if unit.x.abs() > MAX_COORD || unit.y.abs() > MAX_COORD {
                return Err(p.error(at, "that position is off the board"));
            }
        }
    }
    Ok(unit)
}

/// A `[building=...]` or `[device=...]` tag: `what` is the tag name, `kind` its names.
fn parse_structure(
    p: &Parser,
    tag: &Tag,
    kind: NameKind,
    what: &str,
    names: &BTreeMap<String, (u32, String)>,
) -> Result<BoardBuilding, ParseError> {
    let example = if what == "device" {
        "\"Sentry Missile\""
    } else {
        "\"Defensive Wall\""
    };
    let name = tag
        .value
        .ok_or_else(|| p.error(tag.at, format!("say which {what}: [{what}={example} ...]")))?;
    // An ID keeps the exact variant (several IDs share a name); a name takes the lowest ID.
    let (building_id, name) = match name.parse::<u32>() {
        Ok(id) => {
            let name = GameNames::bundled()
                .resolve(kind, u64::from(id))
                .ok_or_else(|| p.error(tag.at, format!("unknown {what} ID {id}")))?;
            (id, name.to_owned())
        }
        Err(_) => names
            .get(&normalize(name))
            .cloned()
            .ok_or_else(|| p.error(tag.at, format!("unknown {what} {name:?}")))?,
    };
    let mut building = BoardBuilding {
        building_id,
        name,
        x: 0,
        y: 0,
    };
    let mut placed = false;
    for (key, value, at) in &tag.attrs {
        let at = *at;
        let value = value.ok_or_else(|| p.error(at, format!("{key} needs a value")))?;
        match key.as_str() {
            "at" => {
                let (x, y) = value
                    .split_once(',')
                    .ok_or_else(|| p.error(at, "at needs x,y in squares, e.g. at=-14,-5.5"))?;
                building.x = coordinate(p, x.trim(), at)?;
                building.y = coordinate(p, y.trim(), at)?;
                placed = true;
            }
            _ => return Err(p.error(at, format!("[{what}] has no {key} attribute"))),
        }
    }
    if !placed {
        return Err(p.error(tag.at, format!("every {what} needs a position: at=x,y")));
    }
    Ok(building)
}

fn squares(units: i32) -> String {
    let sign = if units < 0 { "-" } else { "" };
    let (whole, rest) = (units.abs() / 10, units.abs() % 10);
    match rest {
        0 => format!("{sign}{whole}"),
        _ => format!("{sign}{whole}.{rest}"),
    }
}

/// Quote a value when it has spaces or brackets; drop characters a code can't carry.
fn quoted(value: &str) -> String {
    let clean: String = value
        .chars()
        .filter(|c| *c != '"' && !c.is_control())
        .collect();
    if clean.is_empty() || clean.contains(|c: char| c.is_whitespace() || c == ']' || c == '[') {
        format!("\"{clean}\"")
    } else {
        clean
    }
}

impl fmt::Display for Board {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[board")?;
        if let Some(v) = self.version {
            write!(f, " version={v}")?;
        }
        if let Some(map) = &self.map {
            write!(f, " map={}", quoted(map))?;
        }
        if let Some(r) = self.round {
            write!(f, " round={r}")?;
        }
        writeln!(f, "]")?;
        for side in &self.sides {
            write!(f, "[side")?;
            if let Some(name) = &side.name {
                write!(f, " name={}", quoted(name))?;
            }
            if let Some(hp) = side.hp {
                write!(f, " hp={hp}")?;
            }
            if let Some(half) = side.half {
                write!(f, " half={}", half.name())?;
            }
            writeln!(f, "]")?;
            for u in &side.units {
                write!(
                    f,
                    "[unit={} at={},{}",
                    quoted(&u.name),
                    squares(u.x),
                    squares(u.y)
                )?;
                if u.level != 1 {
                    write!(f, " level={}", u.level)?;
                }
                if u.rotated {
                    write!(f, " rotated")?;
                }
                if !u.techs.is_empty() {
                    write!(f, " techs={}", quoted(&u.techs.join(", ")))?;
                }
                if !u.items.is_empty() {
                    write!(f, " items={}", quoted(&u.items.join(", ")))?;
                }
                writeln!(f, "]")?;
            }
            for (tag, kind, list) in [
                ("building", NameKind::Construction, &side.buildings),
                ("device", NameKind::Device, &side.devices),
            ] {
                let names = name_lookup(kind);
                for b in list {
                    // Several IDs share a name; write the name only when it picks the same one.
                    let by_name = names
                        .get(&normalize(&b.name))
                        .is_some_and(|(id, _)| *id == b.building_id);
                    let what = if by_name {
                        quoted(&b.name)
                    } else {
                        b.building_id.to_string()
                    };
                    writeln!(f, "[{tag}={what} at={},{}]", squares(b.x), squares(b.y))?;
                }
            }
            writeln!(f, "[/side]")?;
        }
        write!(f, "[/board]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const EXAMPLE: &str = r#"Try this opener:
[board version=2324 map="Railway Town" round=5]
[side name="jtk18" hp=4500]
[unit=Crawler at=-12.5,-20 level=2 techs="Acid Crawler, Flame Crawler"]
[unit=fortress at=0,-25 rotated]
[/side]
[side=Opponent]
[unit=Marksman at=3,22.5]
[/side]
[/board]
good luck"#;

    #[test]
    fn test_parse_example() {
        let board = Board::parse(EXAMPLE).unwrap();
        assert_eq!(board.version, Some(2324));
        assert_eq!(board.map.as_deref(), Some("Railway Town"));
        assert_eq!(board.sides.len(), 2);
        let crawler = &board.sides[0].units[0];
        assert_eq!(
            (crawler.name.as_str(), crawler.x, crawler.y),
            ("Crawler", -125, -200)
        );
        assert_eq!(crawler.level, 2);
        assert_eq!(crawler.techs, vec!["Acid Crawler", "Flame Crawler"]);
        let fortress = &board.sides[0].units[1];
        assert_eq!(fortress.name, "Fortress", "names are matched loosely");
        assert!(fortress.rotated);
        assert_eq!(board.sides[1].name.as_deref(), Some("Opponent"));
        assert_eq!(board.sides[1].units[0].y, 225);
    }

    #[test]
    fn test_buildings() {
        let code = r#"[board][side half=south]
[unit=Crawler at=0,-20]
[building="defensive wall" at=-14,-5.5]
[building=3 at=10,-8]
[/side][/board]"#;
        let board = Board::parse(code).unwrap();
        let b = &board.sides[0].buildings;
        assert_eq!(
            (b[0].building_id, b[0].name.as_str(), b[0].x, b[0].y),
            (1, "Defensive Wall", -140, -55)
        );
        assert_eq!(
            (b[1].building_id, b[1].name.as_str()),
            (3, "Rapid-Fire Cannon")
        );
        assert_eq!(Board::parse(&board.to_string()).unwrap(), board);
        // A variant that shares its name with a lower ID keeps its ID through a round trip.
        let variant = Board::parse("[board][side][building=5 at=0,-3][/side][/board]").unwrap();
        assert_eq!(Board::parse(&variant.to_string()).unwrap(), variant);

        for (bad, why) in [
            ("[board][building=1 at=0,0][/board]", "inside a [side]"),
            (
                "[board][side][building=Moat at=0,0][/side][/board]",
                "unknown building",
            ),
            (
                "[board][side][building=1][/side][/board]",
                "needs a position",
            ),
            (
                "[board][side][building=1 at=0,0 level=2][/side][/board]",
                "no level",
            ),
        ] {
            let e = Board::parse(bad).unwrap_err();
            assert!(e.message.contains(why), "{bad}: {e}");
        }
    }

    #[test]
    fn test_devices() {
        let code = r#"[board][side half=south]
[unit=Crawler at=0,-20]
[device="sentry missile" at=23.5,-27]
[device=10001 at=14,-10.5]
[/side][/board]"#;
        let board = Board::parse(code).unwrap();
        let d = &board.sides[0].devices;
        assert_eq!(
            (d[0].building_id, d[0].name.as_str(), d[0].x, d[0].y),
            (20001, "Sentry Missile", 235, -270)
        );
        assert_eq!(
            (d[1].building_id, d[1].name.as_str()),
            (10001, "Shield Generator")
        );
        let text = board.to_string();
        assert!(
            text.contains("[device=\"Sentry Missile\" at=23.5,-27]"),
            "{text}"
        );
        assert_eq!(Board::parse(&text).unwrap(), board);
        let e = Board::parse("[board][side][device=Moat at=0,0][/side][/board]").unwrap_err();
        assert!(e.message.contains("unknown device"), "{e}");
        let e = Board::parse("[board][device=10001 at=0,0][/board]").unwrap_err();
        assert!(e.message.contains("[device] must be inside"), "{e}");
    }

    #[test]
    fn test_items_resolve_by_name_or_id() {
        assert_eq!(resolve_item("Deployment Module"), Ok(13040001));
        assert_eq!(resolve_item("deployment module"), Ok(13040001));
        assert_eq!(resolve_item("Heavy Armor #13030013"), Ok(13030013));
        assert_eq!(resolve_item("#13030013"), Ok(13030013));
        let e = resolve_item("Heavy Armor").unwrap_err();
        assert!(
            e.contains("Heavy Armor #13030002") && e.contains("#13030013"),
            "{e}"
        );
        assert!(resolve_item("Laser Sights #13030013")
            .unwrap_err()
            .contains("not Laser"));
        assert!(resolve_item("Moat").unwrap_err().contains("unknown item"));
        assert_eq!(item_entry(13030013, "Heavy Armor"), "Heavy Armor #13030013");
        assert_eq!(
            item_entry(13040001, "Deployment Module"),
            "Deployment Module"
        );
        // An entry with an ID survives a code round trip.
        let code = r#"[board][side][unit=Fortress at=0,-20 items="Heavy Armor #13030013, Deployment Module"][/side][/board]"#;
        let board = Board::parse(code).unwrap();
        assert_eq!(
            board.sides[0].units[0].items,
            vec!["Heavy Armor #13030013", "Deployment Module"]
        );
        assert_eq!(Board::parse(&board.to_string()).unwrap(), board);
    }

    #[test]
    fn test_round_trip() {
        let board = Board::parse(EXAMPLE).unwrap();
        let code = board.to_string();
        assert_eq!(Board::parse(&code).unwrap(), board);
    }

    #[test]
    fn test_from_replay_round_trips() {
        let fixtures = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures");
        let record =
            crate::BattleRecord::new_from_file(fixtures.join("battle_record_2324.xml"), false)
                .unwrap();
        let detail = record.match_detail();
        let last = detail.rounds.len() - 1;
        let board = Board::from_round(&detail, last).unwrap();
        assert_eq!(board.sides.len(), 2);
        assert!(board.sides.iter().all(|s| !s.units.is_empty()));
        let code = board.to_string();
        assert_eq!(Board::parse(&code).unwrap(), board, "{code}");
    }

    #[test]
    fn test_anchored_positions() {
        let code = r#"[board round=4]
[side half=south]
[unit=Crawler from=front-left right=2 back=1]
[unit=Crawler from=center]
[unit=Crawler from=right-flank forward=3]
[unit=Fang from=command-center forward=4]
[/side]
[side half=north]
[unit=Crawler from=front-left right=2 back=1]
[/side]
[/board]"#;
        let board = Board::parse(code).unwrap();
        let south = &board.sides[0].units;
        // Crawler is 3 wide, 2 deep: flush in the front-left corner, then 2 right, 1 back.
        assert_eq!((south[0].x, south[0].y), (-300 + 15 + 20, -10 - 10 - 10));
        assert_eq!((south[1].x, south[1].y), (0, 0));
        assert_eq!((south[2].x, south[2].y), (330, 10 + 10 + 30));
        assert_eq!((south[3].x, south[3].y), (-140, -175 + 40));
        // The same code for the north half mirrors through the middle.
        let north = &board.sides[1].units[0];
        assert_eq!((north.x, north.y), (-south[0].x, -south[0].y));

        let err = |text: &str| Board::parse(text).unwrap_err().message;
        assert!(err("[board][side][unit=Crawler from=front][/side][/board]").contains("half"));
        assert!(
            err("[board][side half=south][unit=Crawler from=moon][/side][/board]")
                .contains("unknown anchor")
        );
        assert!(err(
            "[board round=2][side half=south][unit=Crawler from=left-flank][/side][/board]"
        )
        .contains("round 3"));
        assert!(
            err("[board][side half=south][unit=Crawler at=1,1 from=front][/side][/board]")
                .contains("not both")
        );
        assert!(err("[board][side][unit=Crawler at=1,1 right=2][/side][/board]").contains("from"));
    }

    #[test]
    fn test_errors_point_at_the_problem() {
        let err = |text: &str| Board::parse(text).unwrap_err();
        let e = err("[board]\n[side]\n[unit=Nope at=1,1]\n[/side]\n[/board]");
        assert_eq!((e.line, e.column), (3, 1));
        assert!(e.message.contains("unknown unit"), "{e}");
        assert!(err("[board][side][unit=Crawler][/side][/board]")
            .message
            .contains("position"));
        assert!(
            err("[board][side][unit=Crawler at=1,1 speed=9][/side][/board]")
                .message
                .contains("no speed")
        );
        assert!(
            err("[board][side][unit=Crawler at=1,1] hello [/side][/board]")
                .message
                .contains("text between tags")
        );
        assert!(err("[board][side][unit=Crawler at=1.25,1][/side][/board]")
            .message
            .contains("coordinate"));
        assert!(err("[board][side][unit=Crawler at=9999,1][/side][/board]")
            .message
            .contains("off the board"));
        assert!(
            err("[board][side][unit=Crawler at=1,1 level=99][/side][/board]")
                .message
                .contains("level")
        );
        assert!(err("[board][side]").message.contains("ends before"));
        assert!(err("no code here").message.contains("no [board]"));
    }

    #[test]
    fn test_hostile_input_is_bounded() {
        let huge = format!("[board]{}", " ".repeat(MAX_INPUT));
        assert!(Board::parse(&huge).unwrap_err().message.contains("longer"));
        let many_sides = format!("[board]{}[/board]", "[side][/side]".repeat(MAX_SIDES + 1));
        assert!(Board::parse(&many_sides)
            .unwrap_err()
            .message
            .contains("sides"));
        let long_name = format!("[board][side name=\"{}\"][/side][/board]", "x".repeat(200));
        assert!(Board::parse(&long_name).is_err());
        // Quotes and brackets in names can't break out of the code when written back.
        let board = Board {
            version: None,
            map: Some("a\"] [unit=Overlord at=0,0]".into()),
            round: None,
            sides: vec![],
        };
        let reparsed = Board::parse(&board.to_string()).unwrap();
        assert!(reparsed.sides.is_empty());
    }
}
