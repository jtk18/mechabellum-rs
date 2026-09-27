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
    pub units: Vec<BoardUnit>,
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
            format!("{value:?} isn't a coordinate (whole or half squares)"),
        )
    };
    let (whole, half) = match value.split_once('.') {
        Some((w, "5")) => (w, 5),
        Some((w, "0")) | Some((w, "")) => (w, 0),
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
    let names = GameNames::bundled();
    let mut map = BTreeMap::new();
    for id in names.ids(NameKind::Unit) {
        if let Some(name) = names.resolve(NameKind::Unit, id) {
            map.entry(normalize(name))
                .or_insert((id as u32, name.to_owned()));
        }
    }
    map
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
                        units: Vec::new(),
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
                    s.units.push(parse_unit(&p, &tag, &units)?);
                }
                ("unit", false, None) => {
                    return Err(p.error(tag.at, "a [unit] must be inside a [side]"))
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
                            .map(|e| e.name.clone().unwrap_or_else(|| e.id.clone()))
                            .collect(),
                    })
                    .collect();
                Some(Side {
                    name: Some(player.name.clone()),
                    hp: Some(side.hp),
                    units,
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
    if !placed {
        return Err(p.error(tag.at, "every unit needs a position: at=x,y"));
    }
    Ok(unit)
}

fn squares(units: i32) -> String {
    let sign = if units < 0 { "-" } else { "" };
    let (whole, rest) = (units.abs() / 10, units.abs() % 10);
    match rest {
        0 => format!("{sign}{whole}"),
        5 => format!("{sign}{whole}.5"),
        // Not on the half-square grid; keep the nearest half square.
        _ => format!("{sign}{whole}.5"),
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
