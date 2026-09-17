use crate::world::list_installed_worlds;
use craft_core::Result;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct SaveItem {
    pub name: String,
    pub path: PathBuf,
    pub size_bytes: u64,
    pub is_active: bool,
    pub game: String,
    pub format_desc: String,
}

pub fn list_saves_for_server(server_path: &Path, game_id: &str) -> Result<Vec<SaveItem>> {
    let lower_game = game_id.to_lowercase();
    match lower_game.as_str() {
        "minecraft" => {
            let worlds = list_installed_worlds(server_path);
            let mut saves = Vec::new();
            for w in worlds {
                saves.push(SaveItem {
                    name: w.name,
                    path: w.path,
                    size_bytes: w.size_bytes,
                    is_active: w.is_default || w.is_nether || w.is_end,
                    game: "minecraft".to_string(),
                    format_desc: "Minecraft Anvil / NBT".to_string(),
                });
            }
            Ok(saves)
        }
        "terraria" => {
            let mut saves = Vec::new();
            let worlds_dir = server_path.join("Worlds");
            let target_dir = if worlds_dir.is_dir() {
                worlds_dir
            } else {
                server_path.to_path_buf()
            };

            if let Ok(entries) = std::fs::read_dir(&target_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("wld") {
                        let name = path
                            .file_stem()
                            .and_then(|s| s.to_str())
                            .unwrap_or("world")
                            .to_string();
                        let size = std::fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                        saves.push(SaveItem {
                            name,
                            path,
                            size_bytes: size,
                            is_active: false,
                            game: "terraria".to_string(),
                            format_desc: "Terraria World (.wld)".to_string(),
                        });
                    }
                }
            }
            Ok(saves)
        }
        "palworld" => {
            let mut saves = Vec::new();
            let save_base = server_path.join("Pal/Saved/SaveGames");
            if save_base.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&save_base) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p.is_dir() {
                            let size = compute_dir_size(&p);
                            saves.push(SaveItem {
                                name: entry.file_name().to_string_lossy().to_string(),
                                path: p,
                                size_bytes: size,
                                is_active: false,
                                game: "palworld".to_string(),
                                format_desc: "Palworld Save (GVAS .sav)".to_string(),
                            });
                        }
                    }
                }
            }
            Ok(saves)
        }
        "factorio" => {
            let mut saves = Vec::new();
            let saves_dir = server_path.join("saves");
            if saves_dir.is_dir() {
                if let Ok(entries) = std::fs::read_dir(&saves_dir) {
                    for entry in entries.flatten() {
                        let p = entry.path();
                        if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("zip") {
                            let name = p
                                .file_stem()
                                .and_then(|s| s.to_str())
                                .unwrap_or("save")
                                .to_string();
                            let size = std::fs::metadata(&p).map(|m| m.len()).unwrap_or(0);
                            saves.push(SaveItem {
                                name,
                                path: p,
                                size_bytes: size,
                                is_active: false,
                                game: "factorio".to_string(),
                                format_desc: "Factorio Save (.zip)".to_string(),
                            });
                        }
                    }
                }
            }
            Ok(saves)
        }
        _ => {
            // Generic save scan
            let mut saves = Vec::new();
            let saves_dir = server_path.join("saves");
            let target_dir = if saves_dir.is_dir() {
                saves_dir
            } else {
                server_path.to_path_buf()
            };

            if let Ok(entries) = std::fs::read_dir(&target_dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    let name = entry.file_name().to_string_lossy().to_string();
                    if p.is_dir() && (name.contains("save") || name.contains("world")) {
                        let size = compute_dir_size(&p);
                        saves.push(SaveItem {
                            name,
                            path: p,
                            size_bytes: size,
                            is_active: false,
                            game: game_id.to_string(),
                            format_desc: "Custom / Directory Save".to_string(),
                        });
                    }
                }
            }
            Ok(saves)
        }
    }
}

fn compute_dir_size(dir: &Path) -> u64 {
    let mut total = 0;
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let p = entry.path();
            if p.is_dir() {
                total += compute_dir_size(&p);
            } else if let Ok(meta) = std::fs::metadata(&p) {
                total += meta.len();
            }
        }
    }
    total
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_list_saves_terraria() {
        let temp = tempfile::tempdir().expect("tempdir");
        let worlds_dir = temp.path().join("Worlds");
        std::fs::create_dir_all(&worlds_dir).expect("create worlds dir");
        std::fs::write(worlds_dir.join("MyWorld.wld"), b"terraria binary data").expect("write wld");

        let saves = list_saves_for_server(temp.path(), "terraria").expect("list saves");
        assert_eq!(saves.len(), 1);
        assert_eq!(saves[0].name, "MyWorld");
        assert_eq!(saves[0].game, "terraria");
        assert_eq!(saves[0].size_bytes, 20);
    }

    #[test]
    fn test_list_saves_factorio() {
        let temp = tempfile::tempdir().expect("tempdir");
        let saves_dir = temp.path().join("saves");
        std::fs::create_dir_all(&saves_dir).expect("create saves dir");
        std::fs::write(saves_dir.join("map1.zip"), b"dummy factorio zip").expect("write zip");

        let saves = list_saves_for_server(temp.path(), "factorio").expect("list saves");
        assert_eq!(saves.len(), 1);
        assert_eq!(saves[0].name, "map1");
        assert_eq!(saves[0].game, "factorio");
    }
}
