//! Game art, extracted on this machine from the player's own Mechabellum install. Nothing here
//! is shipped with the app or kept in the repository.
//!
//! Sprites come out of the game's Unity files with AssetStudioMod's command-line tool (MIT,
//! github.com/aelurum/AssetStudio), downloaded from its official release on first use. The
//! background is the "clean" key art from the official press kit.

use anyhow::{anyhow, bail, Context, Result};
use mechabellum::{GameNames, NameKind};
use serde::{Deserialize, Serialize};

use std::collections::{BTreeMap, BTreeSet};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;

const TOOL_VERSION: &str = "v0.19.0";
/// Official press kit: Mechabellum_Keyart_16x9_Clean.png
const KEY_ART_URL: &str = "https://drive.usercontent.google.com/download?id=11k5SALLz_OV_59W86rjYG9RjuGN1QomM&export=download&confirm=t";
/// Only the sprites the app uses.
const SPRITE_FILTER: &str = "^(Mech_Default_[0-9]+_1|Mech_Portrait_[0-9]+|Mech_Portrait_Research_Center_[0-9A-Z]+|Mech_Portrait_Tower|UT_[A-Za-z0-9_]+|O_[A-Za-z0-9_]+|Icon_officers)$";
/// Bump when the manifest's contents change, so older extractions are redone.
const MANIFEST_FORMAT: u32 = 3;

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

/// The folder holding the game's Unity data files (`sharedassets0.assets`, `level0`).
fn unity_data_dir(game_dir: &Path) -> Option<PathBuf> {
    fn search(dir: &Path, depth: u32) -> Option<PathBuf> {
        if dir.join("sharedassets0.assets").is_file() {
            return Some(dir.to_path_buf());
        }
        if depth == 0 {
            return None;
        }
        std::fs::read_dir(dir)
            .ok()?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .find_map(|p| search(&p, depth - 1))
    }
    search(game_dir, 4)
}

fn tool_asset_name() -> Result<&'static str> {
    Ok(if cfg!(windows) {
        "AssetStudioModCLI_net9_win64"
    } else if cfg!(target_os = "macos") {
        "AssetStudioModCLI_net9_mac64"
    } else if cfg!(target_os = "linux") {
        "AssetStudioModCLI_net9_linux64"
    } else {
        bail!("no extractor build for this platform")
    })
}

fn find_tool(dir: &Path) -> Option<PathBuf> {
    let exe = if cfg!(windows) {
        "AssetStudioModCLI.exe"
    } else {
        "AssetStudioModCLI"
    };
    walk(dir, 3)
        .into_iter()
        .find(|p| p.file_name().is_some_and(|n| n == exe))
}

fn walk(dir: &Path, depth: u32) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for path in entries.flatten().map(|e| e.path()) {
        if path.is_dir() && depth > 0 {
            out.extend(walk(&path, depth - 1));
        } else {
            out.push(path);
        }
    }
    out
}

/// The extractor's small builds run on the shared .NET 9 runtime.
fn check_dotnet() -> Result<()> {
    let output = Command::new("dotnet")
        .arg("--list-runtimes")
        .output()
        .map_err(|_| anyhow!("Extracting game art needs the .NET 9 runtime. Install it from https://dotnet.microsoft.com/download/dotnet/9.0 and try again."))?;
    let runtimes = String::from_utf8_lossy(&output.stdout);
    if !runtimes
        .lines()
        .any(|l| l.starts_with("Microsoft.NETCore.App 9."))
    {
        bail!("Extracting game art needs the .NET 9 runtime. Install it from https://dotnet.microsoft.com/download/dotnet/9.0 and try again.");
    }
    Ok(())
}

fn ensure_tool(tools_dir: &Path) -> Result<PathBuf> {
    let dir = tools_dir.join(format!("AssetStudioModCLI-{TOOL_VERSION}"));
    if let Some(exe) = find_tool(&dir) {
        return Ok(exe);
    }
    let name = tool_asset_name()?;
    let url = format!(
        "https://github.com/aelurum/AssetStudio/releases/download/{TOOL_VERSION}/{name}.zip"
    );
    log::info!("downloading {url}");
    let mut bytes = Vec::new();
    ureq::get(&url)
        .call()
        .with_context(|| format!("downloading {url}"))?
        .into_reader()
        .read_to_end(&mut bytes)?;
    zip::ZipArchive::new(std::io::Cursor::new(bytes))?
        .extract(&dir)
        .context("unpacking the extractor")?;
    let exe = find_tool(&dir).ok_or_else(|| anyhow!("extractor not found in {name}.zip"))?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&exe, std::fs::Permissions::from_mode(0o755))?;
    }
    Ok(exe)
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

/// Extract everything into `art_dir` and write its manifest. `progress` receives short status
/// lines for the UI.
pub fn extract(
    game_dir: &Path,
    art_dir: &Path,
    tools_dir: &Path,
    progress: &dyn Fn(&str),
) -> Result<ArtManifest> {
    let data = unity_data_dir(game_dir).ok_or_else(|| {
        anyhow!(
            "Couldn't find the game's data files under {}",
            game_dir.display()
        )
    })?;
    check_dotnet()?;
    progress("Getting the extractor");
    let tool = ensure_tool(tools_dir)?;

    progress("Extracting unit, tech, and specialist art");
    std::fs::create_dir_all(art_dir)?;
    let status = Command::new(&tool)
        .arg(data.join("sharedassets0.assets"))
        .args(["-m", "export", "-t", "sprite", "-g", "none", "-r"])
        .args(["--filter-by-name", SPRITE_FILTER, "--filter-with-regex"])
        .args(["--log-level", "error", "-o"])
        .arg(art_dir)
        .output()
        .context("running the extractor")?;
    if !status.status.success() {
        bail!(
            "the extractor failed: {}",
            String::from_utf8_lossy(&status.stderr)
        );
    }

    progress("Matching art to units and techs");
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
    for id in names.ids(NameKind::Officer) {
        // Specialists named after a unit ("Elite Rhino", "Subsidized Mustang") use its card art.
        let Some(unit) = names
            .resolve(NameKind::Officer, id)
            .and_then(mechabellum::unit_id_in_name)
        else {
            continue;
        };
        let file = format!("O_M_{unit}_SBUI.png");
        if exists(&file) {
            manifest.specialists.insert(id, file);
        }
    }
    manifest.specialist_fallback = exists("Icon_officers.png").then(|| "Icon_officers.png".into());
    manifest.command_center =
        exists("Mech_Portrait_Tower.png").then(|| "Mech_Portrait_Tower.png".into());
    manifest.research_center = exists("Mech_Portrait_Research_Center_1A.png")
        .then(|| "Mech_Portrait_Research_Center_1A.png".into());

    progress("Downloading the key art background");
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
