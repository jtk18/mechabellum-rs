//! Consistency checks for uploaded replays. None of these prove a replay is genuine, but real
//! replays always pass them, and a hand-edited one usually breaks at least one. See
//! docs/upload-trust.md.

use super::battle_record::{known_result, FIGHT_RESULTS_LISTED_FROM};
use super::game_data::{xp_to_next_level, XP_TABLE_VERIFIED_THROUGH};
use super::names::{GameNames, NameKind};
use super::{BattleRecord, Unit};

use serde::Serialize;

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    /// Stable identifier, e.g. "army-mismatch".
    pub code: &'static str,
    pub detail: String,
}

/// XP thresholds were measured on 2.0 replays; older versions may have used others. Versions
/// after `XP_TABLE_VERIFIED_THROUGH` skip the check: a balance patch can raise a unit's caps, and
/// real replays must never be rejected for that.
const XP_CHECKS_FROM: u32 = 2259;

/// Most units one side fields in a round. Real armies stay far below it (the board code caps a
/// side at 300 too); a crafted replay with thousands bloats every card and match view built
/// from it.
const MAX_ARMY: usize = 300;

fn problem(code: &'static str, detail: impl Into<String>) -> Problem {
    Problem {
        code,
        detail: detail.into(),
    }
}

/// Unit (id, x, y) counts, for comparing two lists of the same army regardless of order.
fn army<'a>(
    units: impl Iterator<Item = (&'a str, &'a str, &'a str)>,
) -> BTreeMap<(String, String, String), u32> {
    let mut counts = BTreeMap::new();
    for (id, x, y) in units {
        *counts
            .entry((
                id.trim().to_owned(),
                x.trim().to_owned(),
                y.trim().to_owned(),
            ))
            .or_default() += 1;
    }
    counts
}

impl BattleRecord {
    /// Everything about this replay that doesn't add up. Empty for every real replay seen so far.
    pub fn validate(&self) -> Vec<Problem> {
        let mut problems = Vec::new();
        let version: u32 = self.version.trim().parse().unwrap_or(0);
        if version == 0 {
            problems.push(problem(
                "version",
                format!("unreadable version {:?}", self.version),
            ));
        }

        let id = self.battle_info.battle_id.trim();
        let well_formed = id.split_once("--").is_some_and(|(date, n)| {
            date.len() == 8
                && date.bytes().all(|b| b.is_ascii_digit())
                && !n.is_empty()
                && n.bytes().all(|b| b.is_ascii_digit())
        });
        // Offline games (against bots, in the sandbox) are stamped with the local time instead,
        // e.g. 25-01-27__20-36-14-492. They're real but can't be shared or corroborated.
        let offline = id.contains("__")
            && id
                .bytes()
                .all(|b| b.is_ascii_digit() || b == b'-' || b == b'_');
        if offline {
            problems.push(problem("offline", format!("offline game {id:?}")));
        } else if !well_formed {
            problems.push(problem("battle-id", format!("unexpected battle ID {id:?}")));
        }

        let players = &self.player_records.player_record;
        if players.is_empty() {
            problems.push(problem("players", "no players"));
        }

        let max_round: u32 = self.battle_info.max_round.trim().parse().unwrap_or(0);
        for player in players {
            let last = player
                .player_round_records
                .player_round_record
                .iter()
                .filter_map(|r| r.round.trim().parse::<u32>().ok())
                .max()
                .unwrap_or(0);
            // Round records run one past the last fight.
            if max_round > 0 && last > max_round + 1 {
                problems.push(problem(
                    "round-count",
                    format!(
                        "{} has round {last}, but the match allows {max_round}",
                        player.name
                    ),
                ));
            }
        }

        // Each player's round record r holds exactly the army that fought round r-1, which
        // snapshot r reports. Star Expedition opponents are recorded armies replayed against
        // you, and their records don't line up this way, so that mode is skipped.
        let asynchronous = self.battle_info.game_mode.trim() == "AsyncBattle";
        for snapshot in self
            .match_datas
            .match_snapshot_data
            .iter()
            .filter(|_| !asynchronous)
        {
            let Some(result) = &snapshot.last_fight_result else {
                continue;
            };
            let round = snapshot.round.trim();
            for (i, report) in result.reports.fight_report.iter().enumerate() {
                let reported = army(
                    report
                        .unit_datas
                        .new_unit_data
                        .iter()
                        .map(|u| (u.id.as_str(), u.position.x.as_str(), u.position.y.as_str())),
                );
                if reported.is_empty() {
                    continue;
                }
                let Some(record) = players.get(i).and_then(|p| {
                    p.player_round_records
                        .player_round_record
                        .iter()
                        .find(|r| r.round.trim() == round)
                }) else {
                    continue;
                };
                let recorded = army(
                    record
                        .player_data
                        .units
                        .new_unit_data
                        .iter()
                        .map(|u| (u.id.as_str(), u.position.x.as_str(), u.position.y.as_str())),
                );
                if !recorded.is_empty() && recorded != reported {
                    problems.push(problem(
                        "army-mismatch",
                        format!(
                            "{}'s round {round} army differs from that round's fight report",
                            players[i].name
                        ),
                    ));
                }
            }
        }

        let names = GameNames::bundled();
        let known_units = names.ids(NameKind::Unit);
        for player in players {
            for record in &player.player_round_records.player_round_record {
                for unit in &record.player_data.units.new_unit_data {
                    let known = matches!(Unit::from_replay_id(&unit.id), Unit::Unknown(_))
                        .then(|| unit.id.trim().parse::<u64>().ok())
                        .flatten()
                        .is_none_or(|n| known_units.contains(&n));
                    if !known {
                        problems.push(problem(
                            "unknown-unit",
                            format!("{} fields unknown unit {}", player.name, unit.id),
                        ));
                    }
                    if (XP_CHECKS_FROM..=XP_TABLE_VERIFIED_THROUGH).contains(&version) {
                        let unit_id: u32 = unit.id.trim().parse().unwrap_or(0);
                        let level: u32 = unit.level.trim().parse().unwrap_or(0);
                        let exp: u32 = unit.exp.trim().parse().unwrap_or(0);
                        if let Some(cap) = xp_to_next_level(unit_id, level) {
                            // The measured multipliers are rounded; allow for that.
                            if exp > cap + cap / 100 + 2 {
                                problems.push(problem(
                                    "xp",
                                    format!(
                                        "{}'s unit {} has {exp} XP at level {level}, above the {cap} that level allows",
                                        player.name, unit.id
                                    ),
                                ));
                            }
                        }
                    }
                }
            }
        }

        for player in players {
            for record in &player.player_round_records.player_round_record {
                let units = record.player_data.units.new_unit_data.len();
                if units > MAX_ARMY {
                    problems.push(problem(
                        "army-size",
                        format!(
                            "{} fields {units} units in round {}, more than {MAX_ARMY}",
                            player.name,
                            record.round.trim()
                        ),
                    ));
                }
            }
        }

        if version >= FIGHT_RESULTS_LISTED_FROM && !asynchronous {
            problems.extend(self.result_problems());
        }

        problems.sort_by(|a, b| a.code.cmp(b.code).then_with(|| a.detail.cmp(&b.detail)));
        problems.dedup();
        problems
    }
}

impl BattleRecord {
    /// From version 2340 the listed fight results decide every fight and the match, so they must
    /// agree with what else the replay records: a lost fight costs reactor HP and a won one
    /// doesn't (110 of 110 fights in the 9 replays of version 2340 on hand, 2026-10-27), and in
    /// a 1v1 the two lists are opposite fight for fight.
    fn result_problems(&self) -> Vec<Problem> {
        let mut problems = Vec::new();
        let players = &self.player_records.player_record;
        let mut lists = Vec::new();
        for player in players {
            let results = player.fight_results().unwrap_or_default();
            let fights = player.last_round().unwrap_or(0).saturating_sub(1) as usize;
            // A concession during deployment ends the match a fight early.
            let fought = if self.last_round_fought() {
                fights + 1
            } else {
                fights
            };
            if results.len() != fought && fought > 0 {
                problems.push(problem(
                    "fight-results",
                    format!(
                        "{} lists {} fight results for {fought} fights",
                        player.name,
                        results.len()
                    ),
                ));
            }
            if let Some(bad) = results.iter().find(|r| known_result(r).is_none()) {
                problems.push(problem(
                    "fight-results",
                    format!("{} has fight result {bad:?}", player.name),
                ));
            }
            let hp: BTreeMap<u32, i64> = player
                .player_round_records
                .player_round_record
                .iter()
                .filter_map(|r| {
                    Some((
                        r.round.trim().parse().ok()?,
                        r.player_data.reactor_core.trim().parse().ok()?,
                    ))
                })
                .collect();
            for (n, result) in results.iter().enumerate() {
                let fight = n as u32 + 1;
                let (Some(before), Some(after)) = (hp.get(&fight), hp.get(&(fight + 1))) else {
                    continue;
                };
                let explained = match known_result(result) {
                    Some("Lose") => after < before,
                    Some("Win") => after >= before,
                    _ => true,
                };
                if !explained {
                    problems.push(problem(
                        "results-hp",
                        format!(
                            "{} {} fight {fight}, but reactor HP went from {before} to {after}",
                            player.name,
                            if result.trim() == "Win" {
                                "won"
                            } else {
                                "lost"
                            }
                        ),
                    ));
                }
            }
            lists.push(results);
        }
        if let [a, b] = lists.as_slice() {
            let opposite = |x: &str, y: &str| {
                matches!(
                    (known_result(x), known_result(y)),
                    (Some("Win"), Some("Lose"))
                        | (Some("Lose"), Some("Win"))
                        | (Some("Deuce"), Some("Deuce"))
                )
            };
            if let Some(n) = a.iter().zip(b.iter()).position(|(x, y)| !opposite(x, y)) {
                problems.push(problem(
                    "fight-results",
                    format!("the players' results for fight {} don't oppose", n + 1),
                ));
            }
        }
        problems
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn fixture(name: &str) -> BattleRecord {
        BattleRecord::new_from_file(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("fixtures")
                .join(name),
            false,
        )
        .unwrap()
    }

    #[test]
    fn test_real_replays_pass() {
        for name in [
            "battle_record_2324.xml",
            "battle_record_2340.xml",
            "1425_20241123--67298547_[小疯子]VS[kaltorak18].grbr",
            "battle_record.xml",
            "4player_brawl2.xml",
        ] {
            assert_eq!(fixture(name).validate(), vec![], "{name}");
        }
    }

    #[test]
    fn test_edits_are_caught() {
        // Move one unit in a round record: it no longer matches that round's fight report.
        let mut moved = fixture("battle_record_2324.xml");
        let record = moved.player_records.player_record[0]
            .player_round_records
            .player_round_record
            .iter_mut()
            // Round 3: the first round whose snapshot carries a fight report to compare with.
            .find(|r| r.round.trim() == "3")
            .unwrap();
        record.player_data.units.new_unit_data[0].position.x = "12345".into();
        assert!(moved.validate().iter().any(|p| p.code == "army-mismatch"));

        // Pump a unit's XP past what its level allows.
        let mut boosted = fixture("battle_record_2324.xml");
        let unit = boosted.player_records.player_record[0]
            .player_round_records
            .player_round_record
            .iter_mut()
            .flat_map(|r| r.player_data.units.new_unit_data.iter_mut())
            .next()
            .unwrap();
        unit.level = "0".into();
        unit.exp = "999999".into();
        assert!(boosted.validate().iter().any(|p| p.code == "xp"));

        let mut bad_id = fixture("battle_record_2324.xml");
        bad_id.battle_info.battle_id = "not-a-battle".into();
        assert!(bad_id.validate().iter().any(|p| p.code == "battle-id"));
    }

    /// The red team's forgery (2026-10-27): the loser of a 2340 match flips every listed result
    /// to claim the win. HP still records the losses, and the lists no longer oppose.
    #[test]
    fn test_forged_results_are_caught() {
        let flip = |r: &str| if r == "Win" { "Lose" } else { "Win" }.to_owned();
        let mut forged = fixture("battle_record_2340.xml");
        for record in &mut forged.player_records.player_record[0]
            .player_round_records
            .player_round_record
        {
            let results = &mut record.player_data.round_fight_result.results;
            *results = results.iter().map(|r| flip(r.trim())).collect();
        }
        let codes: Vec<&str> = forged.validate().iter().map(|p| p.code).collect();
        assert!(codes.contains(&"results-hp"), "{codes:?}");
        assert!(codes.contains(&"fight-results"), "{codes:?}");

        // Both lists flipped consistently still contradict the HP record.
        let mut both = fixture("battle_record_2340.xml");
        for player in &mut both.player_records.player_record {
            for record in &mut player.player_round_records.player_round_record {
                let results = &mut record.player_data.round_fight_result.results;
                *results = results.iter().map(|r| flip(r.trim())).collect();
            }
        }
        assert!(both.validate().iter().any(|p| p.code == "results-hp"));

        // A list one short, or a value the game doesn't write, is flagged too.
        let mut short = fixture("battle_record_2340.xml");
        for record in &mut short.player_records.player_record[1]
            .player_round_records
            .player_round_record
        {
            record.player_data.round_fight_result.results.pop();
        }
        assert!(short.validate().iter().any(|p| p.code == "fight-results"));
        let mut garbage = fixture("battle_record_2340.xml");
        for record in &mut garbage.player_records.player_record[0]
            .player_round_records
            .player_round_record
        {
            record.player_data.round_fight_result.results[0] = "Banana".into();
        }
        assert!(garbage.validate().iter().any(|p| p.code == "fight-results"));
    }
}
