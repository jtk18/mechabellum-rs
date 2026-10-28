//! Whole matches rather than final armies: match cards, and stats over every fight, every unit
//! fielded, and how armies were deployed. Built on the rows `insert_games` stores.

use crate::store::{StatsFilter, Store, SCOPE};

use anyhow::Result;
use serde::Serialize;

/// One page of match cards, newest first.
#[derive(Debug, Serialize)]
pub struct CardPage {
    /// Matches the filter allows, across all pages.
    pub total: u64,
    pub cards: Vec<MatchCard>,
}

/// What a match card shows: the match, and each player's whole game.
#[derive(Debug, Serialize)]
pub struct MatchCard {
    pub battle_id: String,
    pub version: u32,
    pub match_mode: String,
    pub game_mode: String,
    /// Map name, or its ID when the name is unknown.
    pub map: String,
    pub occurred_on: String,
    /// Winners first. Names as stored: a shared upload keeps only the uploader's.
    pub players: Vec<CardPlayer>,
}

#[derive(Debug, Serialize)]
pub struct CardPlayer {
    pub name: String,
    pub won: bool,
    /// MMR at match start, when the game's log had it.
    pub elo: Option<i64>,
    pub starter_pack: Option<mechabellum::StarterPack>,
    /// `mechabellum::PlayerGame` as stored, passed through without re-parsing it; `None` for a
    /// match stored before schema 10 that hasn't been rebuilt.
    pub game: Option<Box<serde_json::value::RawValue>>,
}

/// Stats over whole matches.
#[derive(Debug, Serialize)]
pub struct GameStats {
    pub matches: u64,
    pub player_results: u64,
    /// (fights, matches): how long matches ran.
    pub lengths: Vec<(u32, u64)>,
    /// Per round 1 deployment, most used first. Matches are classed by round 1's layout.
    pub deployments: Vec<DeploymentStat>,
    /// 1v1 matches by both players' round 1 deployments, from the first one's side.
    pub matchups: Vec<Matchup>,
    /// 1v1 matches with both round 1 deployments classed, by how they met: "Asym headbutt",
    /// "Asym crossfire", or `None` when one or neither was one-sided.
    pub meetings: Vec<MeetingStat>,
    /// Means over match winners and over losers.
    pub winners: Option<ArmyMeans>,
    pub losers: Option<ArmyMeans>,
    /// Every unit type fielded at any point in a match, most used first.
    pub units: Vec<UnitUseStat>,
}

#[derive(Debug, Serialize)]
pub struct DeploymentStat {
    pub label: String,
    /// Player results whose round 1 was deployed this way, and how many won the match. A match
    /// counts once for each player in the filter, so with both players in it a match where both
    /// deployed the same way counts twice (one win, one loss).
    pub results: u64,
    pub wins: u64,
    /// Of those, how many won round 1's fight.
    pub round_one_wins: u64,
}

#[derive(Debug, Serialize)]
pub struct Matchup {
    /// Round 1 deployments: this player's and the opponent's.
    pub deployment: String,
    pub against: String,
    /// Player results, as for `DeploymentStat::results`, and the matches they come from. On the
    /// diagonal with both players in the filter, results are twice the matches and the wins
    /// are half the results by construction.
    pub results: u64,
    pub matches: u64,
    /// Results `deployment` won.
    pub wins: u64,
}

#[derive(Debug, Serialize)]
pub struct MeetingStat {
    pub meeting: Option<String>,
    pub matches: u64,
}

#[derive(Debug, Serialize)]
pub struct ArmyMeans {
    pub results: u64,
    pub final_units: f64,
    pub peak_units: f64,
    /// Supply the armies were worth (`PlayerGame::final_value`), over the `priced` results.
    pub final_value: Option<f64>,
    pub peak_value: Option<f64>,
    /// Results whose every fight is priced. The rest leave the value means.
    pub priced: u64,
    pub mean_level: f64,
    pub max_level: f64,
    /// Over the `complete` results (see `PlayerGame::units_added`); `None` when there are none.
    pub level_ups: Option<f64>,
    pub units_added: Option<f64>,
    pub units_sold: Option<f64>,
    /// Results whose fights run from the first without a gap, outside AsyncBattle: the ones
    /// level-ups, units added and sold, and cards picked are averaged over.
    pub complete: u64,
    pub techs: f64,
    pub cards: Option<f64>,
}

#[derive(Debug, Serialize)]
pub struct UnitUseStat {
    pub name: String,
    /// Player results that fielded it in any fight, and how many of those won the match.
    pub games: u64,
    pub wins: u64,
    pub use_rate: f64,
    pub win_rate: f64,
    /// Results that still had it in the last fight.
    pub kept: u64,
    pub added: u64,
    pub sold: u64,
    /// Mean most copies in one fight, over the results that fielded it.
    pub mean_peak: f64,
    /// Mean highest level a copy reached, 1-based.
    pub mean_max_level: f64,
}

fn ratio(n: u64, d: u64) -> f64 {
    if d == 0 {
        0.0
    } else {
        n as f64 / d as f64
    }
}

impl Store {
    /// One stored match from each player's side, in `MatchDetail::players` order.
    pub fn player_games(&self, battle_id: &str) -> Result<Option<Vec<mechabellum::PlayerGame>>> {
        Ok(self.match_detail(battle_id)?.map(|d| d.player_games()))
    }

    /// Match cards for the matches `filter` allows, newest first. A player filter picks the
    /// matches; the cards still show every player in them.
    pub fn cards(&self, filter: &StatsFilter, limit: u32, offset: u32) -> Result<CardPage> {
        let conn = self.conn();
        let scope = SCOPE;
        let args = filter.params();
        let total: u64 = conn.query_row(
            &format!("{scope} SELECT COUNT(DISTINCT battle_id) FROM scope"),
            args,
            |r| r.get(0),
        )?;
        let mut paged: Vec<&dyn rusqlite::ToSql> = args.to_vec();
        paged.push(&limit);
        paged.push(&offset);
        let mut cards: Vec<MatchCard> = conn
            .prepare(&format!(
                "{scope} SELECT m.battle_id, m.version, m.match_mode, m.game_mode, m.map_id,
                        m.occurred_on
                 FROM matches m WHERE m.battle_id IN (SELECT battle_id FROM scope)
                 ORDER BY m.occurred_on DESC, m.battle_id DESC LIMIT ?9 OFFSET ?10"
            ))?
            .query_map(paged.as_slice(), |r| {
                let map_id: String = r.get(4)?;
                Ok(MatchCard {
                    battle_id: r.get(0)?,
                    version: r.get(1)?,
                    match_mode: r.get(2)?,
                    game_mode: r.get(3)?,
                    map: mechabellum::resolve_name(mechabellum::NameKind::Map, &map_id)
                        .map_or(map_id, str::to_owned),
                    occurred_on: r.get(5)?,
                    players: Vec::new(),
                })
            })?
            .collect::<rusqlite::Result<_>>()?;

        let mut players = conn.prepare_cached(
            "SELECT p.name, p.won, r.elo, p.starter_pack, g.card FROM players p
             LEFT JOIN player_ratings r ON r.battle_id = p.battle_id AND r.player_id = p.player_id
             LEFT JOIN player_games g ON g.battle_id = p.battle_id AND g.player_id = p.player_id
             WHERE p.battle_id = ?1 ORDER BY p.won DESC, p.player_id",
        )?;
        for card in &mut cards {
            card.players = players
                .query_map([&card.battle_id], |r| {
                    let pack: Option<String> = r.get(3)?;
                    let game: Option<String> = r.get(4)?;
                    Ok(CardPlayer {
                        name: r.get(0)?,
                        won: r.get(1)?,
                        elo: r.get(2)?,
                        starter_pack: pack.and_then(|p| serde_json::from_str(&p).ok()),
                        game: game.and_then(|g| serde_json::value::RawValue::from_string(g).ok()),
                    })
                })?
                .collect::<rusqlite::Result<_>>()?;
        }
        Ok(CardPage { total, cards })
    }

    /// Whole-match stats over the player results `filter` allows. Plain counts and rates, not
    /// capped per uploader like `robust_stats`.
    pub fn game_stats(&self, filter: &StatsFilter) -> Result<GameStats> {
        let conn = self.conn();
        let scope = SCOPE;
        let args = filter.params();

        let (matches, player_results): (u64, u64) = conn.query_row(
            &format!("{scope} SELECT COUNT(DISTINCT battle_id), COUNT(*) FROM scope"),
            args,
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?;

        let lengths = conn
            .prepare(&format!(
                "{scope} SELECT fights, COUNT(*) FROM (
                     SELECT f.battle_id, MAX(f.fight) AS fights FROM player_fights f
                     WHERE f.battle_id IN (SELECT battle_id FROM scope) GROUP BY f.battle_id)
                 GROUP BY fights ORDER BY fights"
            ))?
            .query_map(args, |r| Ok((r.get(0)?, r.get(1)?)))?
            .collect::<rusqlite::Result<Vec<(u32, u64)>>>()?;

        // Each match is classed by round 1's deployment (`opening`).
        let deployments = conn
            .prepare(&format!(
                "{scope} SELECT g.opening, COUNT(*), SUM(s.won), SUM(COALESCE(f.result = 'Win', 0))
                 FROM player_games g
                 JOIN scope s ON s.battle_id = g.battle_id AND s.player_id = g.player_id
                 LEFT JOIN player_fights f ON f.battle_id = g.battle_id
                                          AND f.player_id = g.player_id AND f.fight = 1
                 WHERE g.opening IS NOT NULL
                 GROUP BY 1 ORDER BY COUNT(*) DESC, 1"
            ))?
            .query_map(args, |r| {
                Ok(DeploymentStat {
                    label: r.get(0)?,
                    results: r.get(1)?,
                    wins: r.get(2)?,
                    round_one_wins: r.get(3)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;

        let matchups = conn
            .prepare(&format!(
                "{scope} SELECT a.opening, b.opening, COUNT(*), COUNT(DISTINCT s.battle_id), SUM(s.won)
                 FROM scope s
                 JOIN matches m ON m.battle_id = s.battle_id AND m.match_mode = 'VS_1_1'
                 JOIN player_games a ON a.battle_id = s.battle_id AND a.player_id = s.player_id
                 JOIN player_games b ON b.battle_id = a.battle_id AND b.player_id != a.player_id
                 WHERE a.opening IS NOT NULL AND b.opening IS NOT NULL
                 GROUP BY 1, 2"
            ))?
            .query_map(args, |r| {
                Ok(Matchup {
                    deployment: r.get(0)?,
                    against: r.get(1)?,
                    results: r.get(2)?,
                    matches: r.get(3)?,
                    wins: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;

        // A 1v1's two rows carry the same meeting, so each match counts once.
        let meetings = conn
            .prepare(&format!(
                "{scope} SELECT a.meeting, COUNT(DISTINCT s.battle_id)
                 FROM scope s
                 JOIN matches m ON m.battle_id = s.battle_id AND m.match_mode = 'VS_1_1'
                 JOIN player_games a ON a.battle_id = s.battle_id AND a.player_id = s.player_id
                 JOIN player_games b ON b.battle_id = a.battle_id AND b.player_id != a.player_id
                 WHERE a.opening IS NOT NULL AND b.opening IS NOT NULL
                 GROUP BY 1 ORDER BY 1"
            ))?
            .query_map(args, |r| {
                Ok(MeetingStat {
                    meeting: r.get(0)?,
                    matches: r.get(1)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;

        let (mut winners, mut losers) = (None, None);
        let mut stmt = conn.prepare(&format!(
            "{scope} SELECT s.won, COUNT(*), AVG(g.final_units), AVG(g.peak_units),
                    AVG(g.mean_level), AVG(g.max_level), AVG(g.level_ups), AVG(g.units_added),
                    AVG(g.units_sold), AVG(g.techs), AVG(g.cards),
                    -- Both values over one set: the results with every fight priced.
                    AVG(CASE WHEN g.peak_value IS NOT NULL THEN g.final_value END),
                    AVG(g.peak_value), COUNT(g.peak_value), COUNT(g.units_added)
             FROM scope s
             JOIN player_games g ON g.battle_id = s.battle_id AND g.player_id = s.player_id
             GROUP BY s.won"
        ))?;
        let rows = stmt.query_map(args, |r| {
            Ok((
                r.get::<_, bool>(0)?,
                ArmyMeans {
                    results: r.get(1)?,
                    final_units: r.get(2)?,
                    peak_units: r.get(3)?,
                    mean_level: r.get(4)?,
                    max_level: r.get(5)?,
                    level_ups: r.get(6)?,
                    units_added: r.get(7)?,
                    units_sold: r.get(8)?,
                    techs: r.get(9)?,
                    cards: r.get(10)?,
                    final_value: r.get(11)?,
                    peak_value: r.get(12)?,
                    priced: r.get(13)?,
                    complete: r.get(14)?,
                },
            ))
        })?;
        for row in rows {
            let (won, means) = row?;
            if won {
                winners = Some(means);
            } else {
                losers = Some(means);
            }
        }

        let units = conn
            .prepare(&format!(
                "{scope} SELECT u.unit, COUNT(*), SUM(s.won), SUM(u.last > 0), SUM(u.added),
                        SUM(u.sold), AVG(u.peak), AVG(u.max_level)
                 FROM player_unit_use u
                 JOIN scope s ON s.battle_id = u.battle_id AND s.player_id = u.player_id
                 GROUP BY u.unit ORDER BY COUNT(*) DESC, u.unit"
            ))?
            .query_map(args, |r| {
                let games: u64 = r.get(1)?;
                let wins: u64 = r.get(2)?;
                Ok(UnitUseStat {
                    name: r.get(0)?,
                    games,
                    wins,
                    use_rate: ratio(games, player_results),
                    win_rate: ratio(wins, games),
                    kept: r.get(3)?,
                    added: r.get(4)?,
                    sold: r.get(5)?,
                    mean_peak: r.get(6)?,
                    mean_max_level: r.get(7)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;

        Ok(GameStats {
            matches,
            player_results,
            lengths,
            deployments,
            matchups,
            meetings,
            winners,
            losers,
            units,
        })
    }
}
