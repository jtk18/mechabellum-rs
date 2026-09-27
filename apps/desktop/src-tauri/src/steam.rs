//! Locating Mechabellum's replay folder through Steam on Windows, macOS, and Linux.

use std::path::{Path, PathBuf};

/// Steam install roots to try, most likely first.
fn steam_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    let home =
        std::env::var_os(if cfg!(windows) { "USERPROFILE" } else { "HOME" }).map(PathBuf::from);

    if cfg!(windows) {
        for var in ["ProgramFiles(x86)", "ProgramFiles"] {
            if let Some(dir) = std::env::var_os(var) {
                roots.push(PathBuf::from(dir).join("Steam"));
            }
        }
        roots.push(PathBuf::from(r"C:\Program Files (x86)\Steam"));
    } else if cfg!(target_os = "macos") {
        if let Some(home) = &home {
            roots.push(home.join("Library/Application Support/Steam"));
        }
    } else if let Some(home) = &home {
        roots.push(home.join(".steam/steam"));
        roots.push(home.join(".local/share/Steam"));
        roots.push(home.join(".var/app/com.valvesoftware.Steam/.local/share/Steam"));
        roots.push(home.join("snap/steam/common/.local/share/Steam"));
    }
    roots
}

/// Library folders listed in `steamapps/libraryfolders.vdf`, plus the root itself.
fn library_folders(root: &Path) -> Vec<PathBuf> {
    let mut libraries = vec![root.to_path_buf()];
    if let Ok(vdf) = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")) {
        libraries.extend(parse_library_paths(&vdf));
    }
    libraries
}

/// Pull `"path"  "<dir>"` values out of a libraryfolders.vdf.
fn parse_library_paths(vdf: &str) -> Vec<PathBuf> {
    vdf.lines()
        .filter_map(|line| {
            let mut quoted = line.split('"').skip(1).step_by(2);
            match (quoted.next(), quoted.next()) {
                (Some(key), Some(value)) if key.eq_ignore_ascii_case("path") => {
                    Some(PathBuf::from(value.replace("\\\\", "\\")))
                }
                _ => None,
            }
        })
        .collect()
}

/// The replay folder inside a Mechabellum install. It sits at `ProjectDatas/Replay` on Windows
/// and under Proton. Other layouts (the macOS app bundle) are found by a shallow search.
pub fn replay_dir_in_install(game_dir: &Path) -> Option<PathBuf> {
    let direct = game_dir.join("ProjectDatas").join("Replay");
    if direct.is_dir() {
        return Some(direct);
    }
    find_dir_named(game_dir, "Replay", 5)
}

fn find_dir_named(dir: &Path, name: &str, depth: u32) -> Option<PathBuf> {
    if depth == 0 {
        return None;
    }
    let entries = std::fs::read_dir(dir).ok()?;
    let mut subdirs = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        if path.file_name().is_some_and(|n| n == name) {
            return Some(path);
        }
        subdirs.push(path);
    }
    subdirs
        .into_iter()
        .find_map(|sub| find_dir_named(&sub, name, depth - 1))
}

/// Find the replay folder of a Steam install of Mechabellum, if any.
pub fn find_replay_dir() -> Option<PathBuf> {
    steam_roots()
        .iter()
        .filter(|root| root.is_dir())
        .flat_map(|root| library_folders(root))
        .map(|lib| lib.join("steamapps").join("common").join("Mechabellum"))
        .filter(|game| game.is_dir())
        .find_map(|game| replay_dir_in_install(&game))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_library_paths() {
        let vdf = r#"
"libraryfolders"
{
	"0"
	{
		"path"		"C:\\Program Files (x86)\\Steam"
		"label"		""
		"apps"
		{
			"669330"		"13042152832"
		}
	}
	"1"
	{
		"path"		"/home/jason/.local/share/Steam"
	}
}"#;
        assert_eq!(
            parse_library_paths(vdf),
            vec![
                PathBuf::from(r"C:\Program Files (x86)\Steam"),
                PathBuf::from("/home/jason/.local/share/Steam"),
            ]
        );
    }

    #[test]
    fn test_replay_dir_in_install() {
        let game = tempfile::tempdir().unwrap();
        // macOS-style: inside the app bundle
        let nested = game
            .path()
            .join("Mechabellum.app/Contents/ProjectDatas/Replay");
        std::fs::create_dir_all(&nested).unwrap();
        assert_eq!(replay_dir_in_install(game.path()), Some(nested));

        // Windows/Proton layout wins when present
        let direct = game.path().join("ProjectDatas/Replay");
        std::fs::create_dir_all(&direct).unwrap();
        assert_eq!(replay_dir_in_install(game.path()), Some(direct));
    }
}
