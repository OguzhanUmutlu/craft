use craft_core::{get_dimension_worlds, CraftError, NbtFile, NbtTag, Result};
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use zip::ZipArchive;

#[derive(Debug, Clone)]
pub struct CuratedMap {
    pub name: &'static str,
    pub category: &'static str,
    pub description: &'static str,
    pub download_url: &'static str,
    pub default_folder: &'static str,
}

pub fn get_curated_maps() -> Vec<CuratedMap> {
    vec![
        CuratedMap {
            name: "SkyBlock 2.1",
            category: "Survival",
            description: "The classic floating island survival challenge with minimal resources.",
            download_url: "https://media.forgecdn.net/files/2260/798/SkyBlock2.1.zip",
            default_folder: "SkyBlock",
        },
        CuratedMap {
            name: "The Dropper",
            category: "Puzzle / Minigame",
            description: "Classic vertical fall puzzle map: survive the drop avoiding obstacles.",
            download_url: "https://media.forgecdn.net/files/2221/413/The%20Dropper.zip",
            default_folder: "TheDropper",
        },
        CuratedMap {
            name: "Parkour Spiral",
            category: "Parkour",
            description: "A giant tower spiral featuring varied parkour biomes and checkpoints.",
            download_url: "https://media.forgecdn.net/files/3120/555/Parkour_Spiral.zip",
            default_folder: "ParkourSpiral",
        },
        CuratedMap {
            name: "Diversity 3",
            category: "Adventure",
            description:
                "Multi-genre adventure map featuring Escape, Trivia, Survival, Puzzle, and Boss.",
            download_url: "https://media.forgecdn.net/files/2908/333/Diversity_3.zip",
            default_folder: "Diversity3",
        },
        CuratedMap {
            name: "Medieval Village Spawn",
            category: "Build / Spawn",
            description:
                "High-detail starter village with castle, market, and houses for server lobby.",
            download_url: "https://media.forgecdn.net/files/3321/102/MedievalVillage.zip",
            default_folder: "MedievalVillage",
        },
        CuratedMap {
            name: "Herobrine's Mansion",
            category: "Adventure / RPG",
            description:
                "Legendary gothic adventure map with custom bosses, shops, and elite mobs.",
            download_url: "https://media.forgecdn.net/files/2218/956/Herobrines_Mansion.zip",
            default_folder: "HerobrinesMansion",
        },
        CuratedMap {
            name: "Terra Swoop Force",
            category: "Elytra / Arcade",
            description:
                "Fast-paced thermal glide arcade flight journey into the center of the Earth.",
            download_url: "https://media.forgecdn.net/files/2347/662/Terra_Swoop_Force.zip",
            default_folder: "TerraSwoopForce",
        },
        CuratedMap {
            name: "Castaway Island",
            category: "Survival / Island",
            description:
                "Shipwrecked on a tropical island with custom caves, dungeons, and monuments.",
            download_url: "https://media.forgecdn.net/files/2253/117/Castaway.zip",
            default_folder: "CastawayIsland",
        },
        CuratedMap {
            name: "Super Hostile: Sea of Flame",
            category: "CTM / Hardcore",
            description:
                "Vechs' iconic Complete the Monument map set in a perilous subterranean underworld.",
            download_url: "https://media.forgecdn.net/files/2242/881/SeaOfFlameII.zip",
            default_folder: "SeaOfFlame",
        },
        CuratedMap {
            name: "Futuristic Lobby Spawn",
            category: "Build / Spawn",
            description:
                "High-tech cyberpunk sci-fi portal hub and spawn structure for server networks.",
            download_url: "https://media.forgecdn.net/files/3421/980/FuturisticLobby.zip",
            default_folder: "FuturisticLobby",
        },
    ]
}

pub fn search_curated_maps(query: &str) -> Vec<CuratedMap> {
    let q = query.trim().to_lowercase();
    get_curated_maps()
        .into_iter()
        .filter(|m| {
            m.name.to_lowercase().contains(&q)
                || m.category.to_lowercase().contains(&q)
                || m.description.to_lowercase().contains(&q)
        })
        .collect()
}

#[derive(Debug, Clone)]
pub struct InstalledWorldItem {
    pub name: String,
    pub path: PathBuf,
    pub is_default: bool,
    pub is_nether: bool,
    pub is_end: bool,
    pub size_bytes: u64,
}

#[derive(Debug, Clone)]
pub struct WorldMetadataSummary {
    pub level_name: String,
    pub game_type: String,
    pub difficulty: String,
    pub hardcore: bool,
    pub spawn_x: i32,
    pub spawn_y: i32,
    pub spawn_z: i32,
    pub time: i64,
    pub day_time: i64,
    pub version_name: Option<String>,
    pub seed: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct PlayerDataSummary {
    pub uuid: String,
    pub name: String,
    pub pos: (f64, f64, f64),
    pub dimension: String,
    pub health: f32,
    pub food_level: i32,
    pub xp_level: i32,
    pub gamemode: String,
    pub inventory_count: usize,
}

#[derive(Debug, Clone)]
pub struct AdvancementEntry {
    pub id: String,
    pub criteria_count: usize,
    pub completed: bool,
}

#[derive(Debug, Clone)]
pub struct DataStorageEntry {
    pub filename: String,
    pub size_bytes: u64,
    pub description: &'static str,
}

pub fn list_installed_worlds(server_path: &Path) -> Vec<InstalledWorldItem> {
    let mut worlds = Vec::new();
    let (default_world, nether_world, end_world) = get_dimension_worlds(server_path);

    if let Ok(entries) = fs::read_dir(server_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("level.dat").exists() {
                let name = entry.file_name().to_string_lossy().to_string();
                let is_default = name.eq_ignore_ascii_case(&default_world);
                let is_nether = nether_world
                    .as_deref()
                    .map(|n| n.eq_ignore_ascii_case(&name))
                    .unwrap_or(false);
                let is_end = end_world
                    .as_deref()
                    .map(|e| e.eq_ignore_ascii_case(&name))
                    .unwrap_or(false);
                let size_bytes = dir_size(&path).unwrap_or(0);
                worlds.push(InstalledWorldItem {
                    name,
                    path,
                    is_default,
                    is_nether,
                    is_end,
                    size_bytes,
                });
            }
        }
    }

    worlds.sort_by(|a, b| {
        // Active default world first, then nether, then end, then alphabetically
        b.is_default
            .cmp(&a.is_default)
            .then_with(|| b.is_nether.cmp(&a.is_nether))
            .then_with(|| b.is_end.cmp(&a.is_end))
            .then_with(|| a.name.cmp(&b.name))
    });
    worlds
}

/// Inspects `level.dat` inside a world directory and returns human-readable summary
pub fn inspect_world_metadata(world_path: &Path) -> Result<WorldMetadataSummary> {
    let level_dat = world_path.join("level.dat");
    if !level_dat.exists() {
        return Err(CraftError::Other(format!(
            "level.dat not found in '{}'",
            world_path.display()
        )));
    }

    let nbt_file = NbtFile::read(&level_dat)?;
    let data = nbt_file.root.get("Data").ok_or_else(|| {
        CraftError::Other("Invalid level.dat: missing 'Data' root compound".to_string())
    })?;

    let level_name = data.get_str("LevelName").unwrap_or("Unknown").to_string();
    let gamemode_num = data.get_i32("GameType").unwrap_or(0);
    let game_type = match gamemode_num {
        0 => "Survival",
        1 => "Creative",
        2 => "Adventure",
        3 => "Spectator",
        _ => "Custom",
    }
    .to_string();

    let diff_num = data.get_i8("Difficulty").unwrap_or(1);
    let difficulty = match diff_num {
        0 => "Peaceful",
        1 => "Easy",
        2 => "Normal",
        3 => "Hard",
        _ => "Normal",
    }
    .to_string();

    let hardcore = data.get_i8("hardcore").map(|b| b != 0).unwrap_or(false);
    let spawn_x = data.get_i32("SpawnX").unwrap_or(0);
    let spawn_y = data.get_i32("SpawnY").unwrap_or(64);
    let spawn_z = data.get_i32("SpawnZ").unwrap_or(0);
    let time = data.get_i64("Time").unwrap_or(0);
    let day_time = data.get_i64("DayTime").unwrap_or(0);

    let seed = data.get_i64("RandomSeed");

    let version_name = data
        .get("Version")
        .and_then(|v| v.get_str("Name"))
        .map(|s| s.to_string());

    Ok(WorldMetadataSummary {
        level_name,
        game_type,
        difficulty,
        hardcore,
        spawn_x,
        spawn_y,
        spawn_z,
        time,
        day_time,
        version_name,
        seed,
    })
}

/// Resolves player UUID to username by reading usercache.json in the server root directory
pub fn resolve_usercache_name(server_root: &Path, uuid_str: &str) -> Option<String> {
    let cache_path = server_root.join("usercache.json");
    if !cache_path.exists() {
        return None;
    }
    let content = fs::read_to_string(&cache_path).ok()?;
    let parsed: serde_json::Value = serde_json::from_str(&content).ok()?;
    if let Some(arr) = parsed.as_array() {
        let clean_target = uuid_str.replace('-', "").to_lowercase();
        for item in arr {
            if let Some(u) = item.get("uuid").and_then(|v| v.as_str()) {
                if u.replace('-', "").to_lowercase() == clean_target {
                    if let Some(name) = item.get("name").and_then(|v| v.as_str()) {
                        return Some(name.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Lists playerdata files inside world_path/playerdata/*.dat
pub fn list_world_player_data(
    world_path: &Path,
    server_root: &Path,
) -> Result<Vec<PlayerDataSummary>> {
    let playerdata_dir = world_path.join("playerdata");
    let mut list = Vec::new();

    if !playerdata_dir.exists() {
        return Ok(list);
    }

    if let Ok(entries) = fs::read_dir(&playerdata_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().map(|e| e == "dat").unwrap_or(false) {
                let filename = entry.file_name().to_string_lossy().to_string();
                let uuid = filename.trim_end_matches(".dat").to_string();
                let name =
                    resolve_usercache_name(server_root, &uuid).unwrap_or_else(|| uuid.clone());

                // Attempt to read NBT player info
                if let Ok(nbt) = NbtFile::read(&path) {
                    let health = nbt.root.get_f32("Health").unwrap_or(20.0);
                    let food_level = nbt.root.get_i32("foodLevel").unwrap_or(20);
                    let xp_level = nbt.root.get_i32("XpLevel").unwrap_or(0);
                    let gm_num = nbt.root.get_i32("playerGameType").unwrap_or(0);
                    let gamemode = match gm_num {
                        0 => "Survival",
                        1 => "Creative",
                        2 => "Adventure",
                        3 => "Spectator",
                        _ => "Survival",
                    }
                    .to_string();

                    let dimension = nbt
                        .root
                        .get_str("Dimension")
                        .unwrap_or("minecraft:overworld")
                        .to_string();

                    let pos = if let Some(coords) = nbt.root.get_list("Pos") {
                        if coords.len() >= 3 {
                            let x = match coords[0] {
                                NbtTag::Double(v) => v,
                                _ => 0.0,
                            };
                            let y = match coords[1] {
                                NbtTag::Double(v) => v,
                                _ => 0.0,
                            };
                            let z = match coords[2] {
                                NbtTag::Double(v) => v,
                                _ => 0.0,
                            };
                            (x, y, z)
                        } else {
                            (0.0, 0.0, 0.0)
                        }
                    } else {
                        (0.0, 0.0, 0.0)
                    };

                    let inventory_count =
                        nbt.root.get_list("Inventory").map(|l| l.len()).unwrap_or(0);

                    list.push(PlayerDataSummary {
                        uuid,
                        name,
                        pos,
                        dimension,
                        health,
                        food_level,
                        xp_level,
                        gamemode,
                        inventory_count,
                    });
                } else {
                    list.push(PlayerDataSummary {
                        uuid: uuid.clone(),
                        name,
                        pos: (0.0, 0.0, 0.0),
                        dimension: "Unknown".to_string(),
                        health: 20.0,
                        food_level: 20,
                        xp_level: 0,
                        gamemode: "Survival".to_string(),
                        inventory_count: 0,
                    });
                }
            }
        }
    }

    list.sort_by_key(|a| a.name.to_lowercase());
    Ok(list)
}

/// Lists completed advancements from advancements/<uuid>.json
pub fn list_world_advancements(world_path: &Path, uuid: &str) -> Result<Vec<AdvancementEntry>> {
    let clean_uuid = uuid.trim().replace('-', "");
    let adv_dir = world_path.join("advancements");
    if !adv_dir.exists() {
        return Ok(Vec::new());
    }

    // Try both with hyphens and without
    let mut target_file = None;
    if let Ok(entries) = fs::read_dir(&adv_dir) {
        for entry in entries.flatten() {
            let fname = entry.file_name().to_string_lossy().to_string();
            if fname.replace('-', "").starts_with(&clean_uuid) && fname.ends_with(".json") {
                target_file = Some(entry.path());
                break;
            }
        }
    }

    let file_path = match target_file {
        Some(p) => p,
        None => return Ok(Vec::new()),
    };

    let content = fs::read_to_string(file_path)?;
    let parsed: serde_json::Value = serde_json::from_str(&content)
        .map_err(|e| CraftError::Other(format!("Failed to parse advancements JSON: {}", e)))?;

    let mut result = Vec::new();
    if let Some(map) = parsed.as_object() {
        for (key, val) in map {
            if key == "DataVersion" {
                continue;
            }
            let completed = val.get("done").and_then(|d| d.as_bool()).unwrap_or(false);
            let criteria_count = val
                .get("criteria")
                .and_then(|c| c.as_object())
                .map(|m| m.len())
                .unwrap_or(0);
            result.push(AdvancementEntry {
                id: key.clone(),
                criteria_count,
                completed,
            });
        }
    }

    result.sort_by(|a, b| b.completed.cmp(&a.completed).then_with(|| a.id.cmp(&b.id)));
    Ok(result)
}

/// Lists all known persistent data storages in world_path/data/*.dat
pub fn list_world_data_storages(world_path: &Path) -> Vec<DataStorageEntry> {
    let data_dir = world_path.join("data");
    let mut list = Vec::new();
    if !data_dir.exists() {
        return list;
    }

    if let Ok(entries) = fs::read_dir(&data_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() && path.extension().map(|e| e == "dat").unwrap_or(false) {
                let filename = entry.file_name().to_string_lossy().to_string();
                let size_bytes = entry.metadata().map(|m| m.len()).unwrap_or(0);
                let description = match filename.as_str() {
                    "raids.dat" => "Village Raid encounters and statuses",
                    "scoreboard.dat" => "Scoreboard objectives, teams, and player scores",
                    "idcounts.dat" => "Item ID sequential allocation counters",
                    "villages.dat" | "village.dat" => "Legacy village and iron golem records",
                    s if s.starts_with("map_") => "In-game exploratory map data",
                    _ => "Game rule or persistent world data storage",
                };
                list.push(DataStorageEntry {
                    filename,
                    size_bytes,
                    description,
                });
            }
        }
    }

    list.sort_by(|a, b| a.filename.cmp(&b.filename));
    list
}

pub fn dir_size(path: &Path) -> io::Result<u64> {
    let mut total = 0;
    if path.is_dir() {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let ft = entry.file_type()?;
            if ft.is_dir() {
                total += dir_size(&entry.path())?;
            } else {
                total += entry.metadata()?.len();
            }
        }
    }
    Ok(total)
}

pub fn extract_zip(zip_path: &Path, extract_to: &Path) -> Result<()> {
    let file = File::open(zip_path)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| CraftError::Other(format!("Failed to open zip archive: {}", e)))?;

    for i in 0..archive.len() {
        let mut entry = archive
            .by_index(i)
            .map_err(|e| CraftError::Other(format!("Failed to read zip entry: {}", e)))?;

        let enclosed = match entry.enclosed_name() {
            Some(p) => p.to_owned(),
            None => continue,
        };

        let out_path = extract_to.join(enclosed);

        if entry.is_dir() {
            fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out_file = File::create(&out_path)?;
            io::copy(&mut entry, &mut out_file)?;
        }
    }
    Ok(())
}

pub fn find_world_root(extracted_dir: &Path) -> Option<PathBuf> {
    if extracted_dir.join("level.dat").exists() {
        return Some(extracted_dir.to_path_buf());
    }

    if let Ok(entries) = fs::read_dir(extracted_dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                if path.join("level.dat").exists() {
                    return Some(path);
                }
                if let Ok(sub_entries) = fs::read_dir(&path) {
                    for sub in sub_entries.flatten() {
                        let sub_path = sub.path();
                        if sub_path.is_dir() && sub_path.join("level.dat").exists() {
                            return Some(sub_path);
                        }
                    }
                }
            }
        }
    }
    None
}

pub fn copy_dir_all(src: &Path, dst: &Path) -> Result<()> {
    fs::create_dir_all(dst)?;
    for entry in fs::read_dir(src)?.flatten() {
        let ty = entry.file_type()?;
        let target = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_all(&entry.path(), &target)?;
        } else {
            fs::copy(entry.path(), target)?;
        }
    }
    Ok(())
}

pub async fn install_world_from_url(
    server_path: &Path,
    url: &str,
    world_name: Option<&str>,
) -> Result<(PathBuf, String)> {
    let resolved_url = crate::map_resolver::resolve_map_download_url(url).await?;

    let store = craft_core::CraftPaths::new().ok().and_then(|paths| {
        let settings = craft_core::GlobalSettings::load(&paths).unwrap_or_default();
        craft_core::CacheStore::new(paths.cache_dir, settings.cache_max_bytes).ok()
    });

    let (zip_path, _temp_dir) = if let Some(store) = store {
        let hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(resolved_url.as_bytes());
            hex::encode(hasher.finalize())
        };
        let rel_subpath = format!("worlds/{}.zip", &hash[..24]);

        if !store.has_artifact(&rel_subpath) {
            let resp = reqwest::get(&resolved_url).await.map_err(|e| {
                CraftError::Download(format!("Failed to download world archive: {}", e))
            })?;
            if !resp.status().is_success() {
                return Err(CraftError::Download(format!(
                    "Failed to download world archive with status {}: {}",
                    resp.status(),
                    resolved_url
                )));
            }
            let bytes = resp.bytes().await.map_err(|e| {
                CraftError::Download(format!("Failed to read world bytes: {}", e))
            })?;
            store.put_artifact_compressed(
                &rel_subpath,
                &bytes,
                world_name.or(Some("Custom Map")),
                Some("map"),
                None,
            )?;
        }

        let temp_dir = tempfile::tempdir()?;
        let temp_zip = temp_dir.path().join("world_archive.zip");
        store.extract_artifact_to(&rel_subpath, &temp_zip)?;
        (temp_zip, Some(temp_dir))
    } else {
        let resp = reqwest::get(&resolved_url).await.map_err(|e| {
            CraftError::Download(format!("Failed to download world archive: {}", e))
        })?;
        if !resp.status().is_success() {
            return Err(CraftError::Download(format!(
                "Failed to download world archive with status {}: {}",
                resp.status(),
                resolved_url
            )));
        }
        let bytes = resp
            .bytes()
            .await
            .map_err(|e| CraftError::Download(format!("Failed to read world bytes: {}", e)))?;
        let temp_dir = tempfile::tempdir()?;
        let temp_zip = temp_dir.path().join("world_archive.zip");
        fs::write(&temp_zip, &bytes)?;
        (temp_zip, Some(temp_dir))
    };

    install_world_from_zip(server_path, &zip_path, world_name)
}

pub fn list_cached_maps() -> Vec<craft_core::CacheEntryMeta> {
    let store = craft_core::CraftPaths::new().ok().and_then(|paths| {
        let settings = craft_core::GlobalSettings::load(&paths).unwrap_or_default();
        craft_core::CacheStore::new(paths.cache_dir, settings.cache_max_bytes).ok()
    });

    if let Some(store) = store {
        list_cached_maps_with_store(&store)
    } else {
        Vec::new()
    }
}

pub fn list_cached_maps_with_store(store: &craft_core::CacheStore) -> Vec<craft_core::CacheEntryMeta> {
    store.list_cached_artifacts(Some("map"))
}

pub fn install_cached_map_with_store(
    server_path: &Path,
    store: &craft_core::CacheStore,
    rel_subpath: &str,
    world_name: Option<&str>,
) -> Result<(PathBuf, String)> {
    let temp_dir = tempfile::tempdir()?;
    let temp_zip = temp_dir.path().join("cached_map.zip");
    store.extract_artifact_to(rel_subpath, &temp_zip)?;

    install_world_from_zip(server_path, &temp_zip, world_name)
}

pub fn install_cached_map(
    server_path: &Path,
    rel_subpath: &str,
    world_name: Option<&str>,
) -> Result<(PathBuf, String)> {
    let store = craft_core::CraftPaths::new().ok().and_then(|paths| {
        let settings = craft_core::GlobalSettings::load(&paths).unwrap_or_default();
        craft_core::CacheStore::new(paths.cache_dir, settings.cache_max_bytes).ok()
    }).ok_or_else(|| CraftError::Other("Cache store is not available".to_string()))?;

    install_cached_map_with_store(server_path, &store, rel_subpath, world_name)
}

pub fn install_world_from_zip(
    server_path: &Path,
    zip_path: &Path,
    world_name: Option<&str>,
) -> Result<(PathBuf, String)> {
    let temp_extract = tempfile::tempdir()?;
    extract_zip(zip_path, temp_extract.path())?;

    let world_root = find_world_root(temp_extract.path()).ok_or_else(|| {
        CraftError::Other("Invalid Minecraft world archive: level.dat was not found.".to_string())
    })?;

    let target_name = match world_name {
        Some(name) if !name.trim().is_empty() => sanitize_world_name(name),
        _ => {
            let folder_name = world_root
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "custom_world".to_string());
            if folder_name.is_empty()
                || folder_name == temp_extract.path().file_name().unwrap().to_string_lossy()
            {
                "custom_world".to_string()
            } else {
                sanitize_world_name(&folder_name)
            }
        }
    };

    let mut dest = server_path.join(&target_name);
    let mut final_name = target_name.clone();
    let mut counter = 1;
    while dest.exists() {
        final_name = format!("{}_{}", target_name, counter);
        dest = server_path.join(&final_name);
        counter += 1;
    }

    copy_dir_all(&world_root, &dest)?;
    Ok((dest, final_name))
}

pub fn install_world_from_folder(
    server_path: &Path,
    folder_path: &Path,
    world_name: Option<&str>,
) -> Result<(PathBuf, String)> {
    if !folder_path.is_dir() || !folder_path.join("level.dat").exists() {
        return Err(CraftError::Other(
            "Invalid world directory: level.dat was not found in specified folder.".to_string(),
        ));
    }

    let target_name = match world_name {
        Some(name) if !name.trim().is_empty() => sanitize_world_name(name),
        _ => {
            let folder_name = folder_path
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "custom_world".to_string());
            sanitize_world_name(&folder_name)
        }
    };

    let mut dest = server_path.join(&target_name);
    let mut final_name = target_name.clone();
    let mut counter = 1;
    while dest.exists() {
        final_name = format!("{}_{}", target_name, counter);
        dest = server_path.join(&final_name);
        counter += 1;
    }

    copy_dir_all(folder_path, &dest)?;
    Ok((dest, final_name))
}

fn sanitize_world_name(raw: &str) -> String {
    let cleaned: String = raw
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let trimmed = cleaned.trim_matches('_');
    if trimmed.is_empty() {
        "custom_world".to_string()
    } else {
        trimmed.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_world_name() {
        assert_eq!(sanitize_world_name("SkyBlock 2.1!"), "SkyBlock_2_1");
        assert_eq!(sanitize_world_name("///"), "custom_world");
        assert_eq!(sanitize_world_name("Normal_World-1"), "Normal_World-1");
    }

    #[test]
    fn test_curated_maps_catalog() {
        let maps = get_curated_maps();
        assert!(!maps.is_empty());
        let results = search_curated_maps("skyblock");
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name, "SkyBlock 2.1");
    }

    #[test]
    fn test_extract_and_find_world_root() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;

        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("test_map.zip");

        // Create a zip with nested world structure: TestWorld/level.dat
        {
            let file = File::create(&zip_path).unwrap();
            let mut zip = ZipWriter::new(file);
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zip.start_file("TestWorld/level.dat", options).unwrap();
            zip.write_all(b"dummy level data").unwrap();
            zip.start_file("TestWorld/region/r.0.0.mca", options)
                .unwrap();
            zip.write_all(b"dummy chunk data").unwrap();
            zip.finish().unwrap();
        }

        let server_dir = tmp.path().join("server");
        fs::create_dir_all(&server_dir).unwrap();

        let (installed_path, world_name) =
            install_world_from_zip(&server_dir, &zip_path, Some("MyCoolMap")).unwrap();
        assert_eq!(world_name, "MyCoolMap");
        assert!(installed_path.exists());
        assert!(installed_path.join("level.dat").exists());
        assert!(installed_path.join("region/r.0.0.mca").exists());

        let worlds = list_installed_worlds(&server_dir);
        assert_eq!(worlds.len(), 1);
        assert_eq!(worlds[0].name, "MyCoolMap");
    }

    #[test]
    fn test_install_world_from_folder() {
        let tmp = tempfile::tempdir().unwrap();
        let src_world = tmp.path().join("source_world");
        fs::create_dir_all(&src_world).unwrap();
        fs::write(src_world.join("level.dat"), b"data").unwrap();

        let server_dir = tmp.path().join("server");
        fs::create_dir_all(&server_dir).unwrap();

        let (installed_path, world_name) =
            install_world_from_folder(&server_dir, &src_world, Some("Lobby World")).unwrap();
        assert_eq!(world_name, "Lobby_World");
        assert!(installed_path.join("level.dat").exists());
    }

    #[test]
    fn test_inspect_world_and_players() {
        use std::collections::BTreeMap;

        let tmp = tempfile::tempdir().unwrap();
        let server_dir = tmp.path().join("server");
        let world_dir = server_dir.join("world");
        fs::create_dir_all(&world_dir).unwrap();

        // Create mock level.dat
        let mut data = BTreeMap::new();
        data.insert(
            "LevelName".to_string(),
            NbtTag::String("AlphaWorld".to_string()),
        );
        data.insert("GameType".to_string(), NbtTag::Int(1)); // Creative
        data.insert("Difficulty".to_string(), NbtTag::Byte(3)); // Hard
        data.insert("hardcore".to_string(), NbtTag::Byte(0));
        data.insert("SpawnX".to_string(), NbtTag::Int(42));
        data.insert("SpawnY".to_string(), NbtTag::Int(64));
        data.insert("SpawnZ".to_string(), NbtTag::Int(108));
        data.insert("RandomSeed".to_string(), NbtTag::Long(999888777));

        let mut root = BTreeMap::new();
        root.insert("Data".to_string(), NbtTag::Compound(data));

        let nbt_file = NbtFile {
            root_name: "".to_string(),
            root: NbtTag::Compound(root),
            is_compressed: true,
        };
        nbt_file.write(world_dir.join("level.dat")).unwrap();

        // Create mock playerdata
        let pdata_dir = world_dir.join("playerdata");
        fs::create_dir_all(&pdata_dir).unwrap();

        let mut p_root = BTreeMap::new();
        p_root.insert("Health".to_string(), NbtTag::Float(18.5));
        p_root.insert("foodLevel".to_string(), NbtTag::Int(19));
        p_root.insert("XpLevel".to_string(), NbtTag::Int(30));
        p_root.insert("playerGameType".to_string(), NbtTag::Int(0));
        p_root.insert(
            "Pos".to_string(),
            NbtTag::List(vec![
                NbtTag::Double(100.0),
                NbtTag::Double(70.0),
                NbtTag::Double(-200.0),
            ]),
        );

        let p_nbt = NbtFile {
            root_name: "".to_string(),
            root: NbtTag::Compound(p_root),
            is_compressed: true,
        };
        p_nbt
            .write(pdata_dir.join("069a79f4-44e9-4726-a5be-fca90e38aaf5.dat"))
            .unwrap();

        // Create mock usercache.json
        fs::write(server_dir.join("usercache.json"), r#"[{"name":"Notch","uuid":"069a79f4-44e9-4726-a5be-fca90e38aaf5","expiresOn":"2030-01-01"}]"#).unwrap();

        // Test inspection
        let meta = inspect_world_metadata(&world_dir).unwrap();
        assert_eq!(meta.level_name, "AlphaWorld");
        assert_eq!(meta.game_type, "Creative");
        assert_eq!(meta.difficulty, "Hard");
        assert_eq!(meta.spawn_x, 42);
        assert_eq!(meta.seed, Some(999888777));

        // Test player listing
        let players = list_world_player_data(&world_dir, &server_dir).unwrap();
        assert_eq!(players.len(), 1);
        assert_eq!(players[0].name, "Notch");
        assert_eq!(players[0].health, 18.5);
        assert_eq!(players[0].xp_level, 30);
        assert_eq!(players[0].pos, (100.0, 70.0, -200.0));
    }

    #[test]
    fn test_cached_map_lifecycle() {
        use std::io::Write;
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;

        let tmp = tempfile::tempdir().unwrap();
        let cache_dir = tmp.path().join("cache");
        let server_dir = tmp.path().join("server");
        fs::create_dir_all(&server_dir).unwrap();

        let store = craft_core::CacheStore::new(cache_dir, 50 * 1024 * 1024).unwrap();

        // 1. Build a dummy world archive zip in memory
        let mut zip_bytes = Vec::new();
        {
            let mut zip = ZipWriter::new(std::io::Cursor::new(&mut zip_bytes));
            let options =
                SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zip.start_file("MyMap/level.dat", options).unwrap();
            zip.write_all(b"fake level dat bytes").unwrap();
            zip.finish().unwrap();
        }

        // 2. Put compressed map into cache store
        store
            .put_artifact_compressed(
                "worlds/test_map.zip",
                &zip_bytes,
                Some("Test Sky Island"),
                Some("map"),
                None,
            )
            .unwrap();

        // 3. List cached maps
        let cached = list_cached_maps_with_store(&store);
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].display_title(), "Test Sky Island");
        assert!(cached[0].is_compressed);

        // 4. Install from cache
        let (installed_path, final_name) = install_cached_map_with_store(
            &server_dir,
            &store,
            "worlds/test_map.zip",
            Some("SkyWorld"),
        )
        .unwrap();

        assert_eq!(final_name, "SkyWorld");
        assert!(installed_path.join("level.dat").exists());
        assert_eq!(
            fs::read(installed_path.join("level.dat")).unwrap(),
            b"fake level dat bytes"
        );
    }
}
