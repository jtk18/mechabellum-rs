//! Game art, extracted on this machine from the player's own Mechabellum install. Nothing here
//! is shipped with the app or kept in the repository.
//!
//! Sprites are read straight out of the game's Unity files by the `unity-sprites` crate (pure
//! Rust, no .NET, nothing downloaded or run). The background is the "clean" key art from the
//! official press kit.

use anyhow::{anyhow, Context, Result};
use mechabellum::{GameNames, NameKind};
use serde::{Deserialize, Serialize};

use rayon::prelude::*;

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};
/// Official press kit: Mechabellum_Keyart_16x9_Clean.png
const KEY_ART_URL: &str = "https://drive.usercontent.google.com/download?id=11k5SALLz_OV_59W86rjYG9RjuGN1QomM&export=download&confirm=t";
/// The steps of an extraction, in order, as the splash screen lists them.
pub const STEPS: [&str; 4] = [
    "Find the game's data files",
    "Extract unit, tech, and specialist art",
    "Match art to units, techs, and specialists",
    "Download the key art background",
];

/// Where an extraction is: overall fraction done, the step it's on (index into [`STEPS`]),
/// and a line about what that step is doing right now.
#[derive(Debug, Clone, Serialize)]
pub struct Progress {
    pub fraction: f32,
    pub step: usize,
    pub steps: [&'static str; 4],
    pub detail: String,
}

impl Progress {
    fn new(fraction: f32, step: usize, detail: impl Into<String>) -> Progress {
        Progress {
            fraction,
            step,
            steps: STEPS,
            detail: detail.into(),
        }
    }
}

/// Only the sprites the app uses:
/// `Mech_Default_{n}_1`, `Mech_Portrait_{n}`, `Mech_Portrait_Research_Center_{X}`,
/// `Mech_Portrait_Tower`, `UT_*`, `O_*`, `RC_*`, `TD_*_SBUI`, `E_*_SBUI`, `Icon_officers`.
fn wanted(name: &str) -> bool {
    let word = |s: &str| !s.is_empty() && s.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_');
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|c| c.is_ascii_digit());
    if name == "Mech_Portrait_Tower" || name == "Icon_officers" {
        return true;
    }
    if let Some(rest) = name.strip_prefix("Mech_Default_") {
        return rest.strip_suffix("_1").is_some_and(digits);
    }
    if let Some(rest) = name.strip_prefix("Mech_Portrait_Research_Center_") {
        return !rest.is_empty()
            && rest
                .bytes()
                .all(|c| c.is_ascii_digit() || c.is_ascii_uppercase());
    }
    if let Some(rest) = name.strip_prefix("Mech_Portrait_") {
        return digits(rest);
    }
    for prefix in ["UT_", "O_", "RC_"] {
        if let Some(rest) = name.strip_prefix(prefix) {
            return word(rest);
        }
    }
    for prefix in ["TD_", "E_"] {
        if let Some(rest) = name
            .strip_prefix(prefix)
            .and_then(|r| r.strip_suffix("_SBUI"))
        {
            return word(rest);
        }
    }
    false
}
/// Bump when the manifest's contents change, so older extractions are redone.
const MANIFEST_FORMAT: u32 = 6;

#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct UnitArt {
    /// Square card art.
    pub icon: Option<String>,
    /// Grayscale silhouette.
    pub portrait: Option<String>,
}

/// File names relative to the art folder.
#[derive(Debug, Default, Clone, Serialize, Deserialize)]
pub struct ArtManifest {
    pub format: u32,
    pub units: BTreeMap<u32, UnitArt>,
    pub techs: BTreeMap<u64, String>,
    pub specialists: BTreeMap<u64, String>,
    /// Equipment (items) by ID.
    #[serde(default)]
    pub equipment: BTreeMap<u64, String>,
    pub specialist_fallback: Option<String>,
    pub command_center: Option<String>,
    pub research_center: Option<String>,
    pub background: Option<String>,
}

impl ArtManifest {
    pub fn load(art_dir: &Path) -> Option<ArtManifest> {
        let manifest: ArtManifest =
            serde_json::from_str(&std::fs::read_to_string(art_dir.join("manifest.json")).ok()?)
                .ok()?;
        (manifest.format == MANIFEST_FORMAT).then_some(manifest)
    }
}

/// Tech ID -> icon sprite name, read from the game's first scene. Each tech's config record
/// stores its icon name (length-prefixed) a few bytes after its ID; the gap depends on the
/// kind of tech.
fn tech_icons(level0: &[u8], tech_ids: &BTreeSet<u64>) -> BTreeMap<u64, String> {
    const GAPS: [usize; 5] = [24, 28, 32, 36, 40];
    let mut map = BTreeMap::new();
    let mut i = 4 + GAPS[GAPS.len() - 1];
    while let Some(pos) = level0[i..].windows(3).position(|w| w == b"UT_") {
        let start = i + pos;
        i = start + 3;
        let len = i32::from_le_bytes(level0[start - 4..start].try_into().unwrap());
        if !(4..=64).contains(&len) {
            continue;
        }
        let Some(name) = level0
            .get(start..start + len as usize)
            .and_then(|b| std::str::from_utf8(b).ok())
            .filter(|n| n.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'))
        else {
            continue;
        };
        for gap in GAPS {
            let id = i32::from_le_bytes(level0[start - gap..start - gap + 4].try_into().unwrap());
            if id > 0 && tech_ids.contains(&(id as u64)) {
                map.entry(id as u64)
                    .or_insert_with(|| format!("{name}.png"));
                break;
            }
        }
    }
    map
}

/// Specialist ID -> card art sprite name. Each specialist's config record in level0 stores its
/// ID shortly before its card art's name: `O_###_SBUI`, `O_M_{unit}_SBUI`, a Research Center
/// card `RC_*`, a Star Expedition card `TD_*_SBUI`, or an item card `E_*_SBUI` for the
/// "Mass-produced" specialists that hand out equipment.
fn specialist_cards(level0: &[u8], officer_ids: &BTreeSet<u64>) -> BTreeMap<u64, String> {
    cards_by_record(level0, &[b"O_", b"RC_", b"TD_", b"E_"], officer_ids, 320)
}

/// Equipment ID -> item card sprite name (`E_*_SBUI`), stored the same way as specialists.
fn equipment_cards(level0: &[u8], equipment_ids: &BTreeSet<u64>) -> BTreeMap<u64, String> {
    cards_by_record(level0, &[b"E_"], equipment_ids, 400)
}

/// Config records in level0 store an ID a varying distance before a length-prefixed sprite
/// name starting with one of `prefixes`. The nearest preceding known ID within `max_gap` bytes
/// wins.
fn cards_by_record(
    level0: &[u8],
    prefixes: &[&[u8]],
    ids: &BTreeSet<u64>,
    max_gap: usize,
) -> BTreeMap<u64, String> {
    let mut map = BTreeMap::new();
    // One pass over the bytes. (Searching ahead for each prefix after every hit rescanned the
    // file up to the next rare prefix each time: 12 s in a debug build.)
    let starts = (max_gap + 4..level0.len()).filter(|&p| {
        prefixes
            .iter()
            .any(|prefix| level0[p..].starts_with(prefix))
    });
    for start in starts {
        let len = i32::from_le_bytes(level0[start - 4..start].try_into().unwrap());
        if !(5..=64).contains(&len) {
            continue;
        }
        let Some(name) = level0
            .get(start..start + len as usize)
            .and_then(|b| std::str::from_utf8(b).ok())
            .filter(|n| n.bytes().all(|c| c.is_ascii_alphanumeric() || c == b'_'))
            .filter(|n| n.starts_with("RC_") || n.ends_with("_SBUI"))
        else {
            continue;
        };
        let id = (8..=max_gap).step_by(4).find_map(|gap| {
            let v = i32::from_le_bytes(level0[start - gap..start - gap + 4].try_into().unwrap());
            (v > 0 && ids.contains(&(v as u64))).then_some(v as u64)
        });
        if let Some(id) = id {
            map.entry(id).or_insert_with(|| format!("{name}.png"));
        }
    }
    map
}

/// Download the press kit key art and shrink it to a background-sized JPEG.
fn background(art_dir: &Path) -> Result<String> {
    let mut bytes = Vec::new();
    ureq::get(KEY_ART_URL)
        .call()?
        .into_reader()
        .take(100 * 1024 * 1024)
        .read_to_end(&mut bytes)?;
    let image = image::load_from_memory(&bytes).context("decoding key art")?;
    let resized = image.resize(1920, 1920, image::imageops::FilterType::Triangle);
    let file = "background.jpg";
    let mut out = std::fs::File::create(art_dir.join(file))?;
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut out, 82)
        .encode_image(&resized.to_rgb8())?;
    Ok(file.into())
}

/// Extract everything into `art_dir` and write its manifest. `progress` hears about each step
/// and, during sprite extraction, each image; it's called from several threads.
pub fn extract(
    game_dir: &Path,
    art_dir: &Path,
    progress: &(dyn Fn(Progress) + Sync),
) -> Result<ArtManifest> {
    progress(Progress::new(
        0.0,
        0,
        format!("Looking in {}", game_dir.display()),
    ));
    let data = mechabellum::install::unity_data_dir(game_dir).ok_or_else(|| {
        anyhow!(
            "Couldn't find the game's data files under {}",
            game_dir.display()
        )
    })?;
    progress(Progress::new(0.02, 0, "Reading sharedassets0.assets"));
    std::fs::create_dir_all(art_dir)?;
    let assets = unity_sprites::Assets::open(&data.join("sharedassets0.assets"))
        .context("reading sharedassets0.assets")?;

    // Names repeat occasionally; the first sprite of a name wins.
    let mut seen = BTreeSet::new();
    let mut by_texture: BTreeMap<i64, Vec<unity_sprites::Sprite>> = BTreeMap::new();
    for sprite in assets.sprites(wanted)? {
        if !seen.insert(sprite.name.clone()) {
            continue;
        }
        match assets.texture_id(&sprite) {
            Ok(id) => by_texture.entry(id).or_default().push(sprite),
            Err(e) => log::warn!("art: {}: {e}", sprite.name),
        }
    }
    // One task per texture: decode it once, cut its sprites. Big atlases go first so the
    // long tasks start early.
    let mut groups: Vec<_> = by_texture.into_iter().collect();
    groups.sort_by_key(|(_, sprites)| std::cmp::Reverse(sprites.len()));
    let total: usize = groups.iter().map(|(_, s)| s.len()).sum();
    let done = AtomicUsize::new(0);
    progress(Progress::new(
        0.05,
        1,
        format!("{total} images from {} textures", groups.len()),
    ));
    groups
        .par_iter()
        .try_for_each(|(texture_id, sprites)| -> Result<()> {
            let texture = match assets.decode_texture(*texture_id) {
                Ok(texture) => texture,
                Err(e) => {
                    log::warn!("art: texture {texture_id}: {e}");
                    done.fetch_add(sprites.len(), Ordering::Relaxed);
                    return Ok(());
                }
            };
            for sprite in sprites {
                match assets.cut(sprite, &texture) {
                    Ok(image) => image::save_buffer(
                        art_dir.join(format!("{}.png", sprite.name)),
                        &image.rgba,
                        image.width,
                        image.height,
                        image::ColorType::Rgba8,
                    )
                    .with_context(|| format!("writing {}.png", sprite.name))?,
                    Err(e) => log::warn!("art: {}: {e}", sprite.name),
                }
                let n = done.fetch_add(1, Ordering::Relaxed) + 1;
                progress(Progress::new(
                    0.05 + 0.8 * n as f32 / total.max(1) as f32,
                    1,
                    format!("{n} of {total} images · {}", sprite.name),
                ));
            }
            Ok(())
        })?;

    progress(Progress::new(
        0.86,
        2,
        "Reading unit and card records from level0",
    ));
    let exists = |file: &str| art_dir.join(file).is_file();
    let names = GameNames::bundled();
    let mut manifest = ArtManifest {
        format: MANIFEST_FORMAT,
        ..Default::default()
    };
    for &id in &names.ids(NameKind::Unit) {
        let id = id as u32;
        let icon = format!("Mech_Default_{id}_1.png");
        let portrait = format!("Mech_Portrait_{id}.png");
        let art = UnitArt {
            icon: exists(&icon).then_some(icon),
            portrait: exists(&portrait).then_some(portrait),
        };
        if art.icon.is_some() || art.portrait.is_some() {
            manifest.units.insert(id, art);
        }
    }
    let level0 = std::fs::read(data.join("level0")).context("reading level0")?;
    manifest.techs = tech_icons(&level0, &names.ids(NameKind::Tech))
        .into_iter()
        .filter(|(_, file)| exists(file))
        .collect();
    manifest.equipment = equipment_cards(&level0, &names.ids(NameKind::Equipment))
        .into_iter()
        .filter(|(_, file)| exists(file))
        .collect();
    let officer_ids = names.ids(NameKind::Officer);
    manifest.specialists = specialist_cards(&level0, &officer_ids)
        .into_iter()
        .filter(|(_, file)| exists(file))
        .collect();
    for id in officer_ids {
        if manifest.specialists.contains_key(&id) {
            continue;
        }
        // The rest ("Vanguard-Vulcan", "Supplies-Fortress") use their unit's art when named after one.
        let Some(unit) = names
            .resolve(NameKind::Officer, id)
            .and_then(mechabellum::unit_id_in_name)
        else {
            continue;
        };
        let card = format!("O_M_{unit}_SBUI.png");
        let icon = format!("Mech_Default_{unit}_1.png");
        if let Some(file) = [card, icon].into_iter().find(|f| exists(f)) {
            manifest.specialists.insert(id, file);
        }
    }
    manifest.specialist_fallback = exists("Icon_officers.png").then(|| "Icon_officers.png".into());
    manifest.command_center =
        exists("Mech_Portrait_Tower.png").then(|| "Mech_Portrait_Tower.png".into());
    manifest.research_center = exists("Mech_Portrait_Research_Center_1A.png")
        .then(|| "Mech_Portrait_Research_Center_1A.png".into());

    progress(Progress::new(
        0.9,
        3,
        "Official press kit art, resized to 1920 px",
    ));
    manifest.background = match background(art_dir) {
        Ok(file) => Some(file),
        Err(e) => {
            log::warn!("background: {e:#}");
            None
        }
    };

    std::fs::write(
        art_dir.join("manifest.json"),
        serde_json::to_string_pretty(&manifest)?,
    )?;
    Ok(manifest)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_wanted_matches_the_old_extractor_filter() {
        for name in [
            "Mech_Default_10_1",
            "Mech_Portrait_101",
            "Mech_Portrait_Research_Center_1A",
            "Mech_Portrait_Tower",
            "UT_Rage",
            "O_004_SBUI",
            "O_M_10_SBUI",
            "RC_AttackNoArrow_n",
            "TD_Something_SBUI",
            "E_Shield_SBUI",
            "Icon_officers",
        ] {
            assert!(wanted(name), "{name}");
        }
        for name in [
            "Mech_Default_1_1_1_1",
            "Mech_Default_10_2",
            "Mech_Portrait_1_a",
            "Mech_Portrait_Research_Center_1a",
            "TD_Something",
            "E_Shield",
            "UT_",
            "UT_Bad-Name",
            "Icon_officers_2",
        ] {
            assert!(!wanted(name), "{name}");
        }
    }

    #[test]
    fn test_specialist_cards_from_records() {
        let mut b = vec![0u8; 400];
        for (id, gap, name) in [
            (10004u32, 144usize, "O_004_SBUI"),
            (20310, 60, "RC_AttackNoArrow_n"),
        ] {
            let start = b.len() + gap;
            b.extend(id.to_le_bytes());
            b.resize(start - 4, 0);
            b.extend((name.len() as i32).to_le_bytes());
            b.extend(name.as_bytes());
            b.extend([0u8; 16]);
        }
        let ids: BTreeSet<u64> = [10004, 20310].into();
        let map = specialist_cards(&b, &ids);
        assert_eq!(map.get(&10004).map(String::as_str), Some("O_004_SBUI.png"));
        assert!(equipment_cards(&b, &ids).is_empty(), "no E_ names here");
        assert_eq!(
            map.get(&20310).map(String::as_str),
            Some("RC_AttackNoArrow_n.png")
        );
    }

    #[test]
    fn test_tech_icons_from_records() {
        // Two synthetic records: ID, padding to the gap, length-prefixed icon name.
        let mut b = vec![0u8; 64];
        for (id, gap, name) in [(701u32, 24usize, "UT_Double_Shot"), (10510, 28, "UT_Rage")] {
            let start = b.len() + gap;
            b.extend(id.to_le_bytes());
            b.resize(start - 4, 0);
            b.extend((name.len() as i32).to_le_bytes());
            b.extend(name.as_bytes());
            b.extend([0u8; 16]);
        }
        let ids: BTreeSet<u64> = [701, 10510].into();
        let map = tech_icons(&b, &ids);
        assert_eq!(
            map.get(&701).map(String::as_str),
            Some("UT_Double_Shot.png")
        );
        assert_eq!(map.get(&10510).map(String::as_str), Some("UT_Rage.png"));
    }
}
