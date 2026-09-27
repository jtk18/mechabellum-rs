//! Stats that no single uploader can move much (docs/upload-trust.md, layer 4): each uploader's
//! weight is capped, the headline number is a median of per-uploader win rates, and uploaders
//! whose results sit far from everyone else's are flagged for review.

use serde::Serialize;

use std::collections::BTreeMap;

/// One uploader's results for one unit (or specialist).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UploaderCount {
    pub uploader: String,
    pub games: u64,
    pub wins: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct RobustStat {
    pub name: String,
    pub games: u64,
    pub wins: u64,
    /// Plain pooled win rate, for comparison.
    pub win_rate: f64,
    /// Pooled win rate after capping each uploader's games at `cap`.
    pub capped_win_rate: f64,
    /// Median of per-uploader win rates, over uploaders with at least `MIN_GROUP` games.
    pub median_uploader_win_rate: Option<f64>,
    pub uploaders: usize,
    /// Most games one uploader may contribute to this stat.
    pub cap: u64,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Outlier {
    pub name: String,
    pub uploader: String,
    pub games: u64,
    pub win_rate: f64,
    /// Everyone else's pooled win rate for the same stat.
    pub others_win_rate: f64,
    /// How many standard errors apart the two are.
    pub z: f64,
}

/// An uploader's share of a stat is capped at this fraction of its games...
const CAP_SHARE: f64 = 0.02;
/// ...but never below this many games, so small stats still count everyone.
const CAP_FLOOR: u64 = 20;
/// Uploaders need this many games in a stat to join the median.
const MIN_GROUP: u64 = 5;
/// Uploaders need this many games in a stat to be judged an outlier.
const OUTLIER_MIN_GAMES: u64 = 30;
/// Standard errors from everyone else that count as an outlier.
const OUTLIER_Z: f64 = 4.0;

fn ratio(a: f64, b: f64) -> f64 {
    if b == 0.0 {
        0.0
    } else {
        a / b
    }
}

fn median(mut values: Vec<f64>) -> Option<f64> {
    if values.is_empty() {
        return None;
    }
    values.sort_by(|a, b| a.total_cmp(b));
    let mid = values.len() / 2;
    Some(if values.len() % 2 == 1 {
        values[mid]
    } else {
        (values[mid - 1] + values[mid]) / 2.0
    })
}

/// Robust stats per name, from (name, uploader, games, wins) rows.
pub fn robust_stats(rows: &BTreeMap<String, Vec<UploaderCount>>) -> Vec<RobustStat> {
    let mut out: Vec<RobustStat> = rows
        .iter()
        .map(|(name, groups)| {
            let games: u64 = groups.iter().map(|g| g.games).sum();
            let wins: u64 = groups.iter().map(|g| g.wins).sum();
            let cap = ((games as f64 * CAP_SHARE).ceil() as u64).max(CAP_FLOOR);
            // Scale an over-cap uploader down to `cap` games, keeping their win rate.
            let (capped_games, capped_wins) = groups.iter().fold((0.0, 0.0), |(g, w), group| {
                let kept = group.games.min(cap) as f64;
                (
                    g + kept,
                    w + kept * ratio(group.wins as f64, group.games as f64),
                )
            });
            RobustStat {
                name: name.clone(),
                games,
                wins,
                win_rate: ratio(wins as f64, games as f64),
                capped_win_rate: ratio(capped_wins, capped_games),
                median_uploader_win_rate: median(
                    groups
                        .iter()
                        .filter(|g| g.games >= MIN_GROUP)
                        .map(|g| ratio(g.wins as f64, g.games as f64))
                        .collect(),
                ),
                uploaders: groups.len(),
                cap,
            }
        })
        .collect();
    out.sort_by(|a, b| b.games.cmp(&a.games).then_with(|| a.name.cmp(&b.name)));
    out
}

/// Uploaders whose win rate in a stat is implausibly far from everyone else's.
pub fn outliers(rows: &BTreeMap<String, Vec<UploaderCount>>) -> Vec<Outlier> {
    let mut out = Vec::new();
    for (name, groups) in rows {
        let games: u64 = groups.iter().map(|g| g.games).sum();
        let wins: u64 = groups.iter().map(|g| g.wins).sum();
        for g in groups.iter().filter(|g| g.games >= OUTLIER_MIN_GAMES) {
            let (other_games, other_wins) = (games - g.games, wins - g.wins);
            if other_games < OUTLIER_MIN_GAMES {
                continue;
            }
            let others = ratio(other_wins as f64, other_games as f64);
            let mine = ratio(g.wins as f64, g.games as f64);
            // Standard error of the uploader's rate if they were like everyone else. Keep it
            // off zero so a 0% or 100% crowd doesn't make every difference infinite.
            let se = (others.clamp(0.02, 0.98) * (1.0 - others.clamp(0.02, 0.98)) / g.games as f64)
                .sqrt();
            let z = (mine - others) / se;
            if z.abs() >= OUTLIER_Z {
                out.push(Outlier {
                    name: name.clone(),
                    uploader: g.uploader.clone(),
                    games: g.games,
                    win_rate: mine,
                    others_win_rate: others,
                    z,
                });
            }
        }
    }
    out.sort_by(|a, b| b.z.abs().total_cmp(&a.z.abs()));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn group(uploader: &str, games: u64, wins: u64) -> UploaderCount {
        UploaderCount {
            uploader: uploader.into(),
            games,
            wins,
        }
    }

    /// 50 honest uploaders at about 50%, and one poisoner with many fake wins.
    fn poisoned() -> BTreeMap<String, Vec<UploaderCount>> {
        let mut groups: Vec<UploaderCount> = (0..50)
            .map(|i| group(&format!("honest{i}"), 20, 10 + (i % 3) - 1))
            .collect();
        groups.push(group("poisoner", 1000, 1000));
        BTreeMap::from([("Crawler".to_string(), groups)])
    }

    #[test]
    fn test_one_uploader_cant_move_the_numbers() {
        let stats = robust_stats(&poisoned());
        let crawler = &stats[0];
        assert!(
            crawler.win_rate > 0.7,
            "the plain rate is poisoned: {}",
            crawler.win_rate
        );
        assert!(
            (crawler.capped_win_rate - 0.5).abs() < 0.05,
            "capped: {}",
            crawler.capped_win_rate
        );
        let median = crawler.median_uploader_win_rate.unwrap();
        assert!((median - 0.5).abs() < 0.03, "median: {median}");
        assert_eq!(crawler.uploaders, 51);
    }

    #[test]
    fn test_outliers_flag_the_poisoner_only() {
        let flagged = outliers(&poisoned());
        assert_eq!(flagged.len(), 1);
        assert_eq!(flagged[0].uploader, "poisoner");
        assert!(flagged[0].z > OUTLIER_Z);
    }

    #[test]
    fn test_small_stats_count_everyone() {
        let rows = BTreeMap::from([(
            "Overlord".to_string(),
            vec![group("a", 3, 2), group("b", 4, 1)],
        )]);
        let s = &robust_stats(&rows)[0];
        assert_eq!(s.cap, CAP_FLOOR);
        assert!((s.capped_win_rate - s.win_rate).abs() < 1e-9);
        assert_eq!(s.median_uploader_win_rate, None, "no uploader has 5 games");
        assert!(outliers(&rows).is_empty());
    }
}
