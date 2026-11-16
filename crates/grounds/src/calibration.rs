//! Per-map calibrations: board squares to screen pixels for each half, in the fully zoomed-out
//! Testing Grounds view. Every Testing Grounds map's measurements are compiled in (from
//! `data/`); `grounds calibrate` measures a map again and saves it in a JSON file, whose maps
//! take precedence over the compiled-in ones.

use crate::{Projection, TOP_DOWN_NORTH, TOP_DOWN_SOUTH};

use serde::{Deserialize, Serialize};

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// Measured points: (board x, board y, screen x, screen y).
pub type Points = Vec<(f64, f64, f64, f64)>;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct MapCalibration {
    pub south: Points,
    pub north: Points,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Calibrations {
    /// Keyed by map name as the game shows it (e.g. "Forest Eye").
    pub maps: BTreeMap<String, MapCalibration>,
}

/// Maps whose deployment zones match Railway Town's (same layout, different lighting).
const RAILWAY_TOWN_VARIANTS: [&str; 3] = [
    "Railway Town",
    "Railway Town - Dusk",
    "Railway Town - Night",
];

/// The file calibrations for the straight-down view are kept in (`calibrations.json` holds the
/// earlier ones, for the starting tilt).
const FILE: &str = "top-down.json";
/// The same for the starting tilt (`GROUNDS_VIEW=tilted`).
const TILTED_FILE: &str = "tilted.json";

/// Where calibrations are kept: the repository's data folder when run from a checkout, else next
/// to the program.
pub fn default_path() -> PathBuf {
    let file = if crate::tilted_view() {
        TILTED_FILE
    } else {
        FILE
    };
    let checkout = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("data")
        .join(file);
    if checkout.parent().is_some_and(Path::is_dir) {
        return checkout;
    }
    std::env::current_exe()
        .ok()
        .and_then(|p| p.parent().map(|d| d.join(file)))
        .unwrap_or_else(|| PathBuf::from(file))
}

impl Calibrations {
    /// The measurements compiled into the program, for the current view. A copy of the program
    /// without its data folder (the desktop app, a moved binary) still knows every map.
    pub fn bundled() -> Calibrations {
        let text = if crate::tilted_view() {
            include_str!("../data/tilted.json")
        } else {
            include_str!("../data/top-down.json")
        };
        serde_json::from_str(text).unwrap_or_default()
    }

    /// The compiled-in measurements, with the maps in the file at `path` (if any) in place of
    /// theirs.
    pub fn load(path: &Path) -> Calibrations {
        let mut all = Calibrations::bundled();
        let saved: Option<Calibrations> = std::fs::read_to_string(path)
            .ok()
            .and_then(|t| serde_json::from_str(&t).ok());
        if let Some(saved) = saved {
            all.maps.extend(saved.maps);
        }
        all
    }

    pub fn save(&self, path: &Path) -> anyhow::Result<()> {
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        std::fs::write(path, serde_json::to_string_pretty(self)? + "\n")?;
        Ok(())
    }

    /// Projections (south, north) for a map. Railway Town and its variants fall back to the
    /// built-in measurements.
    pub fn projections(&self, map: &str) -> Option<(Projection, Projection)> {
        let key = |s: &str| s.to_lowercase().replace([' ', '-'], "");
        if let Some((_, m)) = self.maps.iter().find(|(name, _)| key(name) == key(map)) {
            return Some((Projection::fit(&m.south)?, Projection::fit(&m.north)?));
        }
        if RAILWAY_TOWN_VARIANTS.iter().any(|v| key(v) == key(map)) {
            if crate::tilted_view() {
                return Some((
                    Projection::fit(&crate::RAILWAY_TOWN_SOUTH)?,
                    Projection::fit(&crate::RAILWAY_TOWN_NORTH)?,
                ));
            }
            return Some((
                Projection::fit(&TOP_DOWN_SOUTH)?,
                Projection::fit(&TOP_DOWN_NORTH)?,
            ));
        }
        None
    }
}

/// Match recorded board positions to the screen points units were dropped at. Drops that
/// failed leave fewer recorded units than targets; recorded units are matched in placement
/// order when that fits well, else by trying every assignment. Returns the matched points and
/// the fit's worst error in pixels.
pub fn match_points(screen: &[(f64, f64)], recorded: &[(f64, f64)]) -> Option<(Points, f64)> {
    let fit_error = |pts: &Points| -> Option<f64> {
        let p = Projection::fit(pts)?;
        Some(
            pts.iter()
                .map(|&(x, y, u, v)| {
                    let (su, sv) = p.to_screen(x, y);
                    (su - u).hypot(sv - v)
                })
                .fold(0.0, f64::max),
        )
    };
    let n = recorded.len().min(screen.len());
    if n < 4 {
        return None;
    }
    // Replays list units in placement order, so try that first. A refused drop can leave an
    // extra unit in the list (it stays where it spawned), so also try skipping recorded units,
    // keeping the order. Trying every assignment is a last resort: a symmetric layout can fit
    // just as well rotated half a turn.
    let extra = recorded.len().saturating_sub(screen.len());
    let mut best_ordered: Option<(Points, f64)> = None;
    for skip in skip_sets(recorded.len(), extra.min(2)) {
        let kept: Vec<(f64, f64)> = recorded
            .iter()
            .enumerate()
            .filter(|(i, _)| !skip.contains(i))
            .map(|(_, p)| *p)
            .collect();
        let pts: Points = (0..kept.len().min(screen.len()))
            .map(|i| (kept[i].0, kept[i].1, screen[i].0, screen[i].1))
            .collect();
        if pts.len() < 4 {
            continue;
        }
        if let Some(e) = fit_error(&pts) {
            if best_ordered.as_ref().is_none_or(|(_, b)| e < *b) {
                best_ordered = Some((pts, e));
            }
        }
    }
    if let Some((pts, e)) = best_ordered {
        if e <= 25.0 {
            return Some((pts, e));
        }
    }
    let mut best: Option<(Points, f64)> = None;
    let mut chosen = Vec::with_capacity(n);
    fn search(
        screen: &[(f64, f64)],
        recorded: &[(f64, f64)],
        chosen: &mut Vec<usize>,
        best: &mut Option<(Points, f64)>,
        fit_error: &dyn Fn(&Points) -> Option<f64>,
    ) {
        if chosen.len() == recorded.len().min(screen.len()) {
            let pts: Points = chosen
                .iter()
                .enumerate()
                .map(|(r, &s)| (recorded[r].0, recorded[r].1, screen[s].0, screen[s].1))
                .collect();
            if let Some(e) = fit_error(&pts) {
                if best.as_ref().is_none_or(|(_, b)| e < *b) {
                    *best = Some((pts, e));
                }
            }
            return;
        }
        for s in 0..screen.len() {
            if !chosen.contains(&s) {
                chosen.push(s);
                search(screen, recorded, chosen, best, fit_error);
                chosen.pop();
            }
        }
    }
    if screen.len() <= 8 {
        search(screen, &recorded[..n], &mut chosen, &mut best, &fit_error);
    } else {
        let pts: Points = (0..n)
            .map(|i| (recorded[i].0, recorded[i].1, screen[i].0, screen[i].1))
            .collect();
        let e = fit_error(&pts)?;
        best = Some((pts, e));
    }
    best
}

/// Match drops to recorded units using a calibration that's already close: each drop takes the
/// nearest unrecorded unit where `guess` puts it, then the fit is redone and the matching
/// repeated once with it. Needs the drops spaced well apart compared with the guess's error.
pub fn match_by_guess(
    screen: &[(f64, f64)],
    recorded: &[(f64, f64)],
    guess: &Projection,
) -> Option<(Points, f64)> {
    let assign = |p: &Projection| -> Points {
        let mut taken = vec![false; recorded.len()];
        let mut pts = Points::new();
        for &(u, v) in screen {
            let nearest = recorded
                .iter()
                .enumerate()
                .filter(|(i, _)| !taken[*i])
                .map(|(i, &(x, y))| {
                    let (su, sv) = p.to_screen(x, y);
                    (i, (su - u).hypot(sv - v))
                })
                .min_by(|a, b| a.1.total_cmp(&b.1));
            // Farther than about three squares is a refused drop, not a match.
            if let Some((i, _)) = nearest.filter(|(_, d)| *d < 60.0) {
                taken[i] = true;
                pts.push((recorded[i].0, recorded[i].1, u, v));
            }
        }
        pts
    };
    let first = assign(guess);
    let refit = Projection::fit(&first)?;
    let pts = assign(&refit);
    if pts.len() < 4 {
        return None;
    }
    let p = Projection::fit(&pts)?;
    let err = pts
        .iter()
        .map(|&(x, y, u, v)| {
            let (su, sv) = p.to_screen(x, y);
            (su - u).hypot(sv - v)
        })
        .fold(0.0, f64::max);
    Some((pts, err))
}

/// Every set of `k` indices below `n` (k is 0, 1, or 2 here).
fn skip_sets(n: usize, k: usize) -> Vec<Vec<usize>> {
    match k {
        0 => vec![vec![]],
        1 => (0..n).map(|i| vec![i]).collect(),
        _ => (0..n)
            .flat_map(|i| (i + 1..n).map(move |j| vec![i, j]))
            .collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::RAILWAY_TOWN_SOUTH;

    #[test]
    fn test_match_points_recovers_order() {
        let truth = Projection::fit(&RAILWAY_TOWN_SOUTH).unwrap();
        let board = [
            (-5.5, -17.0),
            (18.5, -21.0),
            (0.5, -17.0),
            (23.5, -10.0),
            (-9.5, -25.0),
            (10.0, -5.0),
        ];
        let screen: Vec<(f64, f64)> = board.iter().map(|&(x, y)| truth.to_screen(x, y)).collect();
        // Recorded in a scrambled order.
        let recorded = vec![board[3], board[0], board[5], board[1], board[4], board[2]];
        let (pts, err) = match_points(&screen, &recorded).unwrap();
        assert!(err < 1.0, "{err}");
        for (x, y, u, v) in pts {
            let (su, sv) = truth.to_screen(x, y);
            assert!((su - u).hypot(sv - v) < 1.0);
        }
    }

    #[test]
    fn test_match_points_skips_a_stray_unit() {
        let truth = Projection::fit(&RAILWAY_TOWN_SOUTH).unwrap();
        let board = [
            (-22.0, -6.0),
            (18.0, -9.0),
            (-4.0, -3.0),
            (-25.0, -27.0),
            (24.0, -22.0),
        ];
        let screen: Vec<(f64, f64)> = board.iter().map(|&(x, y)| truth.to_screen(x, y)).collect();
        // A refused drop left a unit at the spawn pad, recorded between the others.
        let mut recorded = board.to_vec();
        recorded.insert(2, (0.5, -16.0));
        let (pts, err) = match_points(&screen, &recorded).unwrap();
        assert!(err < 1.0, "{err}");
        assert_eq!(pts.len(), 5);
    }

    #[test]
    fn test_match_by_guess_handles_many_points_out_of_order() {
        let truth = Projection::fit(&RAILWAY_TOWN_SOUTH).unwrap();
        // A guess about a square off.
        let shifted: Points = RAILWAY_TOWN_SOUTH
            .iter()
            .map(|&(x, y, u, v)| (x, y, u + 12.0, v - 10.0))
            .collect();
        let guess = Projection::fit(&shifted).unwrap();
        let board: Vec<(f64, f64)> = (0..14)
            .map(|i| (-26.0 + 4.0 * i as f64, -3.0 - 2.0 * (i % 7) as f64 * 3.5))
            .collect();
        let screen: Vec<(f64, f64)> = board.iter().map(|&(x, y)| truth.to_screen(x, y)).collect();
        // Recorded in another order, and one drop was refused (never recorded).
        let mut recorded: Vec<(f64, f64)> = board.iter().rev().copied().collect();
        recorded.remove(5);
        let (pts, err) = match_by_guess(&screen, &recorded, &guess).unwrap();
        assert_eq!(pts.len(), 13);
        assert!(err < 1.0, "{err}");
    }

    #[test]
    fn test_bundled_calibrations_cover_every_map() {
        // The compiled-in file must parse and fit both halves of every Testing Grounds map; a
        // file that failed to parse would come back empty and fail this.
        let all = Calibrations::bundled();
        for map in [
            "Railway Town",
            "Forest Eye",
            "Training Base",
            "Railway Town - Dusk",
            "Railway Town - Night",
        ] {
            assert!(all.projections(map).is_some(), "{map}");
        }
        assert!(all.maps.contains_key("Forest Eye"));
        // A file with no such map leaves the compiled-in one in place.
        let missing = Calibrations::load(Path::new("/nonexistent/top-down.json"));
        assert!(missing.projections("Forest Eye").is_some());
    }

    #[test]
    fn test_railway_variants_share_calibration() {
        let c = Calibrations::default();
        assert!(c.projections("Railway Town - Night").is_some());
        assert!(c.projections("Forest Eye").is_none());
    }
}
