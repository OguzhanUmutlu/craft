use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use craft_core::{get_default_world, CraftError, Result};
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
            description: "Multi-genre adventure map featuring Escape, Trivia, Survival, Puzzle, and Boss.",
            download_url: "https://media.forgecdn.net/files/2908/333/Diversity_3.zip",
            default_folder: "Diversity3",
        },
        CuratedMap {
            name: "Medieval Village Spawn",
            category: "Build / Spawn",
            description: "High-detail starter village with castle, market, and houses for server lobby.",
            download_url: "https://media.forgecdn.net/files/3321/102/MedievalVillage.zip",
            default_folder: "MedievalVillage",
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
    pub size_bytes: u64,
}

pub fn list_installed_worlds(server_path: &Path) -> Vec<InstalledWorldItem> {
    let mut worlds = Vec::new();
    let default_world = get_default_world(server_path);

    if let Ok(entries) = fs::read_dir(server_path) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() && path.join("level.dat").exists() {
                let name = entry.file_name().to_string_lossy().to_string();
                let is_default = name.eq_ignore_ascii_case(&default_world);
                let size_bytes = dir_size(&path).unwrap_or(0);
                worlds.push(InstalledWorldItem {
                    name,
                    path,
                    is_default,
                    size_bytes,
                });
            }
        }
    }

    worlds.sort_by(|a, b| {
        // Active default world first, then alphabetically
        b.is_default.cmp(&a.is_default).then_with(|| a.name.cmp(&b.name))
    });
    worlds
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
        let mut entry = archive.by_index(i)
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
    let store = craft_core::CraftPaths::new()
        .ok()
        .and_then(|paths| {
            let settings = craft_core::GlobalSettings::load(&paths).unwrap_or_default();
            craft_core::CacheStore::new(paths.cache_dir, settings.cache_max_bytes).ok()
        });

    let (zip_path, _temp_dir) = if let Some(store) = store {
        let hash = {
            use sha2::{Digest, Sha256};
            let mut hasher = Sha256::new();
            hasher.update(url.as_bytes());
            hex::encode(hasher.finalize())
        };
        let rel_subpath = format!("worlds/{}.zip", &hash[..24]);

        let path = match store.get_artifact(&rel_subpath) {
            Some(p) => p,
            None => {
                let resp = reqwest::get(url).await
                    .map_err(|e| CraftError::Download(format!("Failed to download world archive: {}", e)))?;
                let bytes = resp.bytes().await
                    .map_err(|e| CraftError::Download(format!("Failed to read world bytes: {}", e)))?;
                let (p, _) = store.put_artifact(&rel_subpath, &bytes, None)?;
                p
            }
        };
        (path, None)
    } else {
        let resp = reqwest::get(url).await
            .map_err(|e| CraftError::Download(format!("Failed to download world archive: {}", e)))?;
        let bytes = resp.bytes().await
            .map_err(|e| CraftError::Download(format!("Failed to read world bytes: {}", e)))?;
        let temp_dir = tempfile::tempdir()?;
        let temp_zip = temp_dir.path().join("world_archive.zip");
        fs::write(&temp_zip, &bytes)?;
        (temp_zip, Some(temp_dir))
    };

    install_world_from_zip(server_path, &zip_path, world_name)
}

pub fn install_world_from_zip(
    server_path: &Path,
    zip_path: &Path,
    world_name: Option<&str>,
) -> Result<(PathBuf, String)> {
    let temp_extract = tempfile::tempdir()?;
    extract_zip(zip_path, temp_extract.path())?;

    let world_root = find_world_root(temp_extract.path())
        .ok_or_else(|| CraftError::Other("Invalid Minecraft world archive: level.dat was not found.".to_string()))?;

    let target_name = match world_name {
        Some(name) if !name.trim().is_empty() => sanitize_world_name(name),
        _ => {
            let folder_name = world_root
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
                .unwrap_or_else(|| "custom_world".to_string());
            if folder_name.is_empty() || folder_name == temp_extract.path().file_name().unwrap().to_string_lossy() {
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
        .map(|c| if c.is_ascii_alphanumeric() || c == '_' || c == '-' { c } else { '_' })
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
        use zip::write::SimpleFileOptions;
        use zip::ZipWriter;
        use std::io::Write;

        let tmp = tempfile::tempdir().unwrap();
        let zip_path = tmp.path().join("test_map.zip");

        // Create a zip with nested world structure: TestWorld/level.dat
        {
            let file = File::create(&zip_path).unwrap();
            let mut zip = ZipWriter::new(file);
            let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Stored);
            zip.start_file("TestWorld/level.dat", options).unwrap();
            zip.write_all(b"dummy level data").unwrap();
            zip.start_file("TestWorld/region/r.0.0.mca", options).unwrap();
            zip.write_all(b"dummy chunk data").unwrap();
            zip.finish().unwrap();
        }

        let server_dir = tmp.path().join("server");
        fs::create_dir_all(&server_dir).unwrap();

        let (installed_path, world_name) = install_world_from_zip(&server_dir, &zip_path, Some("MyCoolMap")).unwrap();
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

        let (installed_path, world_name) = install_world_from_folder(&server_dir, &src_world, Some("Lobby World")).unwrap();
        assert_eq!(world_name, "Lobby_World");
        assert!(installed_path.join("level.dat").exists());
    }
}
