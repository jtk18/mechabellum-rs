//! Comparing the board a replay recorded with the board code that was set up.

use mechabellum::board::{Board, BoardUnit, Half, Side};

/// Largest distance between a unit's planned and recorded center, in replay units (10 per
/// square). The game snaps centers to half squares and nudges units off each other.
const TOLERANCE: i32 = 10;

fn half_of(side: &Side, index: usize, count: usize) -> Half {
    side.half.unwrap_or(if count == 2 && index == 1 {
        Half::North
    } else {
        Half::South
    })
}

fn describe(u: &BoardUnit) -> String {
    format!(
        "{} at {:.1},{:.1}",
        u.name,
        f64::from(u.x) / 10.0,
        f64::from(u.y) / 10.0
    )
}

fn same_techs(a: &[String], b: &[String]) -> bool {
    let key = |s: &String| {
        s.chars()
            .filter(|c| c.is_alphanumeric())
            .flat_map(char::to_lowercase)
            .collect::<String>()
    };
    let mut a: Vec<String> = a.iter().map(key).collect();
    let mut b: Vec<String> = b.iter().map(key).collect();
    a.sort();
    b.sort();
    a == b
}

/// Items compared by ID where they resolve, else by name.
fn same_items(a: &[String], b: &[String]) -> bool {
    let key = |s: &String| {
        mechabellum::board::resolve_item(s).map_or_else(
            |_| {
                s.chars()
                    .filter(|c| c.is_alphanumeric())
                    .flat_map(char::to_lowercase)
                    .collect()
            },
            |id| id.to_string(),
        )
    };
    let mut a: Vec<String> = a.iter().map(key).collect();
    let mut b: Vec<String> = b.iter().map(key).collect();
    a.sort();
    b.sort();
    a == b
}

/// Everything in `recorded` that differs from `planned`, one line each. Sides are matched by
/// half. Empty means the game set up exactly what the code asked for.
pub fn differences(planned: &Board, recorded: &Board) -> Vec<String> {
    let mut out = Vec::new();
    for (i, want) in planned.sides.iter().enumerate() {
        let half = half_of(want, i, planned.sides.len());
        let Some(got) = recorded
            .sides
            .iter()
            .enumerate()
            .find(|(j, s)| half_of(s, *j, recorded.sides.len()) == half)
            .map(|(_, s)| s)
        else {
            if !want.units.is_empty() {
                out.push(format!("{half:?} half: no units were recorded"));
            }
            continue;
        };
        let mut unmatched: Vec<&BoardUnit> = got.units.iter().collect();
        for w in &want.units {
            // The nearest recorded unit of the same type.
            let best = unmatched
                .iter()
                .enumerate()
                .filter(|(_, g)| g.unit_id == w.unit_id)
                .min_by_key(|(_, g)| (g.x - w.x).abs() + (g.y - w.y).abs());
            let Some((k, g)) = best.map(|(k, g)| (k, *g)) else {
                out.push(format!("{half:?}: missing {}", describe(w)));
                continue;
            };
            if (g.x - w.x).abs() > TOLERANCE || (g.y - w.y).abs() > TOLERANCE {
                out.push(format!(
                    "{half:?}: {} landed at {}",
                    describe(w),
                    describe(g)
                ));
            }
            if g.level != w.level {
                out.push(format!(
                    "{half:?}: {} is level {}, not {}",
                    describe(w),
                    g.level,
                    w.level
                ));
            }
            if g.rotated != w.rotated {
                out.push(format!(
                    "{half:?}: {} is {}rotated",
                    describe(w),
                    if g.rotated { "" } else { "not " }
                ));
            }
            if !same_techs(&g.techs, &w.techs) {
                out.push(format!(
                    "{half:?}: {} has techs [{}], not [{}]",
                    describe(w),
                    g.techs.join(", "),
                    w.techs.join(", ")
                ));
            }
            if !same_items(&g.items, &w.items) {
                out.push(format!(
                    "{half:?}: {} has items [{}], not [{}]",
                    describe(w),
                    g.items.join(", "),
                    w.items.join(", ")
                ));
            }
            unmatched.remove(k);
        }
        for extra in unmatched {
            out.push(format!("{half:?}: extra {}", describe(extra)));
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_differences() {
        let planned = Board::parse(
            "[board][side half=south][unit=Crawler at=1,-5 level=2 techs=\"Impact Drill\"][unit=Fang at=10,-5][/side][side half=north][unit=Mustang at=0,5 rotated][/side][/board]",
        )
        .unwrap();
        // The game nudged the Crawler half a square: still a match.
        let same = Board::parse(
            "[board][side half=south][unit=Fang at=10,-5][unit=Crawler at=1.5,-5 level=2 techs=\"impact drill\"][/side][side half=north][unit=Mustang at=0,5 rotated][/side][/board]",
        )
        .unwrap();
        assert!(
            differences(&planned, &same).is_empty(),
            "{:?}",
            differences(&planned, &same)
        );

        let wrong = Board::parse(
            "[board][side half=south][unit=Crawler at=5,-5][unit=Hound at=0,-9][/side][side half=north][unit=Mustang at=0,5][/side][/board]",
        )
        .unwrap();
        let d = differences(&planned, &wrong);
        assert!(
            d.iter().any(|l| l.contains("Crawler at 1.0,-5.0 landed")),
            "{d:?}"
        );
        assert!(d.iter().any(|l| l.contains("level 1, not 2")), "{d:?}");
        assert!(d.iter().any(|l| l.contains("techs []")), "{d:?}");
        assert!(d.iter().any(|l| l.contains("missing Fang")), "{d:?}");
        assert!(d.iter().any(|l| l.contains("extra Hound")), "{d:?}");
        assert!(d.iter().any(|l| l.contains("not rotated")), "{d:?}");
    }
}
