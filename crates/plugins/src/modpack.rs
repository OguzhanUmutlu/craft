use craft_core::{CacheStore, CraftError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{BufReader, Read};
use std::path::Path;
use tracing::{debug, info};
use zip::ZipArchive;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ModpackKind {
    Modrinth,
    CurseForge,
    Unknown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModpackEnv {
    pub client: Option<String>,
    pub server: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthHashes {
    pub sha1: Option<String>,
    pub sha512: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthFileEntry {
    pub path: String,
    pub hashes: ModrinthHashes,
    pub env: Option<ModpackEnv>,
    pub downloads: Vec<String>,
    #[serde(rename = "fileSize")]
    pub file_size: Option<u64>,
}

impl ModrinthFileEntry {
    pub fn is_server_eligible(&self) -> bool {
        if let Some(ref env) = self.env {
            if let Some(ref srv) = env.server {
                if srv.to_lowercase() == "unsupported" {
                    return false;
                }
            }
        }
        true
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModrinthIndex {
    #[serde(rename = "formatVersion")]
    pub format_version: u32,
    pub game: String,
    #[serde(rename = "versionId")]
    pub version_id: String,
    pub name: String,
    pub summary: Option<String>,
    pub files: Vec<ModrinthFileEntry>,
    pub dependencies: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurseForgeModLoader {
    pub id: String,
    pub primary: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurseForgeMinecraft {
    pub version: String,
    #[serde(rename = "modLoaders", default)]
    pub mod_loaders: Vec<CurseForgeModLoader>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurseForgeFileEntry {
    #[serde(rename = "projectID")]
    pub project_id: u64,
    #[serde(rename = "fileID")]
    pub file_id: u64,
    pub required: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CurseForgeManifest {
    #[serde(rename = "manifestType")]
    pub manifest_type: String,
    #[serde(rename = "manifestVersion")]
    pub manifest_version: u32,
    pub name: String,
    pub version: String,
    pub author: Option<String>,
    pub minecraft: CurseForgeMinecraft,
    pub files: Vec<CurseForgeFileEntry>,
    pub overrides: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModpackInspectSummary {
    pub kind: ModpackKind,
    pub name: String,
    pub version: String,
    pub game_version: Option<String>,
    pub loader: Option<String>,
    pub total_files: usize,
    pub server_eligible_files: usize,
    pub client_only_files: usize,
    pub has_overrides: bool,
    pub has_server_overrides: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModpackInstallSummary {
    pub name: String,
    pub version: String,
    pub files_downloaded: usize,
    pub files_cached: usize,
    pub overrides_applied: usize,
    pub total_bytes: u64,
}

pub fn inspect_modpack_archive(archive_path: &Path) -> Result<ModpackInspectSummary> {
    let file = File::open(archive_path).map_err(|e| {
        CraftError::Other(format!(
            "Failed to open modpack archive {}: {}",
            archive_path.display(),
            e
        ))
    })?;

    let mut zip = ZipArchive::new(BufReader::new(file)).map_err(|e| {
        CraftError::Other(format!("Failed to read modpack zip archive: {}", e))
    })?;

    let mut has_overrides = false;
    let mut has_server_overrides = false;
    for i in 0..zip.len() {
        if let Ok(entry) = zip.by_index(i) {
            let name = entry.name();
            if name.starts_with("overrides/") {
                has_overrides = true;
            }
            if name.starts_with("server-overrides/") {
                has_server_overrides = true;
            }
        }
    }

    // Try Modrinth mrpack (modrinth.index.json)
    if let Ok(mut index_file) = zip.by_name("modrinth.index.json") {
        let mut content = String::new();
        index_file
            .read_to_string(&mut content)
            .map_err(|e| CraftError::Other(format!("Failed to read modrinth.index.json: {}", e)))?;

        let index: ModrinthIndex = serde_json::from_str(&content)
            .map_err(|e| CraftError::Other(format!("Failed to parse modrinth.index.json: {}", e)))?;

        let game_version = index.dependencies.get("minecraft").cloned();
        let loader = index
            .dependencies
            .iter()
            .find(|(k, _)| k.contains("loader") || k.as_str() == "forge" || k.as_str() == "fabric" || k.as_str() == "quilt" || k.as_str() == "neoforge")
            .map(|(k, v)| format!("{}: {}", k, v));

        let total_files = index.files.len();
        let server_eligible = index.files.iter().filter(|f| f.is_server_eligible()).count();
        let client_only = total_files.saturating_sub(server_eligible);

        return Ok(ModpackInspectSummary {
            kind: ModpackKind::Modrinth,
            name: index.name,
            version: index.version_id,
            game_version,
            loader,
            total_files,
            server_eligible_files: server_eligible,
            client_only_files: client_only,
            has_overrides,
            has_server_overrides,
        });
    }

    // Try CurseForge manifest.json
    if let Ok(mut manifest_file) = zip.by_name("manifest.json") {
        let mut content = String::new();
        manifest_file
            .read_to_string(&mut content)
            .map_err(|e| CraftError::Other(format!("Failed to read manifest.json: {}", e)))?;

        let manifest: CurseForgeManifest = serde_json::from_str(&content)
            .map_err(|e| CraftError::Other(format!("Failed to parse manifest.json: {}", e)))?;

        let game_version = Some(manifest.minecraft.version.clone());
        let loader = manifest
            .minecraft
            .mod_loaders
            .iter()
            .find(|l| l.primary)
            .or_else(|| manifest.minecraft.mod_loaders.first())
            .map(|l| l.id.clone());

        let total_files = manifest.files.len();

        return Ok(ModpackInspectSummary {
            kind: ModpackKind::CurseForge,
            name: manifest.name,
            version: manifest.version,
            game_version,
            loader,
            total_files,
            server_eligible_files: total_files,
            client_only_files: 0,
            has_overrides,
            has_server_overrides,
        });
    }

    Err(CraftError::Other(
        "Archive is not a valid Modrinth (.mrpack) or CurseForge modpack (missing modrinth.index.json or manifest.json)".to_string(),
    ))
}

pub async fn install_modpack_archive(
    archive_path: &Path,
    target_dir: &Path,
    cache_store: Option<&CacheStore>,
) -> Result<ModpackInstallSummary> {
    fs::create_dir_all(target_dir)?;

    let file = File::open(archive_path).map_err(|e| {
        CraftError::Other(format!(
            "Failed to open modpack archive {}: {}",
            archive_path.display(),
            e
        ))
    })?;

    let mut zip = ZipArchive::new(BufReader::new(file)).map_err(|e| {
        CraftError::Other(format!("Failed to read modpack zip archive: {}", e))
    })?;

    // Check if Modrinth
    let modrinth_index = if let Ok(mut index_file) = zip.by_name("modrinth.index.json") {
        let mut content = String::new();
        index_file.read_to_string(&mut content)?;
        let index: ModrinthIndex = serde_json::from_str(&content)?;
        Some(index)
    } else {
        None
    };

    let mut files_downloaded = 0usize;
    let mut files_cached = 0usize;
    let mut total_bytes = 0u64;

    if let Some(index) = modrinth_index {
        info!(
            pack = %index.name,
            version = %index.version_id,
            files = index.files.len(),
            "Installing Modrinth modpack"
        );

        let client = reqwest::Client::builder()
            .user_agent("craft-modpack-engine/1.0")
            .build()
            .map_err(|e| CraftError::Other(format!("Failed to build HTTP client: {}", e)))?;

        for file_entry in &index.files {
            if !file_entry.is_server_eligible() {
                debug!(path = %file_entry.path, "Skipping client-only mod in server modpack");
                continue;
            }

            let dest_path = target_dir.join(&file_entry.path);
            if let Some(parent) = dest_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let cache_subpath = format!("modpack_files/{}", file_entry.path);
            let mut got_from_cache = false;

            if let Some(store) = cache_store {
                if store.has_artifact(&cache_subpath) {
                    if let Ok(()) = store.extract_artifact_to(&cache_subpath, &dest_path) {
                        files_cached += 1;
                        got_from_cache = true;
                    }
                }
            }

            if !got_from_cache {
                let download_url = file_entry.downloads.first().ok_or_else(|| {
                    CraftError::Other(format!("No download URLs for file {}", file_entry.path))
                })?;

                debug!(path = %file_entry.path, url = %download_url, "Downloading modpack file");
                let resp = client
                    .get(download_url)
                    .send()
                    .await
                    .map_err(|e| CraftError::Download(format!("Download failed for {}: {}", download_url, e)))?;

                if !resp.status().is_success() {
                    return Err(CraftError::Download(format!(
                        "Download failed with status {} for {}",
                        resp.status(),
                        download_url
                    )));
                }

                let bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| CraftError::Download(format!("Failed to read file bytes: {}", e)))?;

                // Verify SHA-512 if present
                if let Some(ref expected_sha512) = file_entry.hashes.sha512 {
                    let mut hasher = Sha512::new();
                    hasher.update(&bytes);
                    let computed_sha512 = hex::encode(hasher.finalize());
                    if !computed_sha512.eq_ignore_ascii_case(expected_sha512) {
                        return Err(CraftError::Other(format!(
                            "SHA-512 checksum mismatch for {}: expected {}, computed {}",
                            file_entry.path, expected_sha512, computed_sha512
                        )));
                    }
                }

                total_bytes += bytes.len() as u64;
                fs::write(&dest_path, &bytes)?;
                files_downloaded += 1;

                if let Some(store) = cache_store {
                    let _ = store.put_artifact_compressed(
                        &cache_subpath,
                        &bytes,
                        Some(&file_entry.path),
                        Some("modpack_file"),
                        None,
                    );
                }
            }
        }

        // Apply overrides/ and server-overrides/
        let overrides_applied = apply_overrides_from_zip(&mut zip, target_dir)?;

        return Ok(ModpackInstallSummary {
            name: index.name,
            version: index.version_id,
            files_downloaded,
            files_cached,
            overrides_applied,
            total_bytes,
        });
    }

    // CurseForge manifest
    let manifest: CurseForgeManifest = {
        let mut manifest_file = zip.by_name("manifest.json").map_err(|_| {
            CraftError::Other("Not a valid Modrinth or CurseForge modpack".to_string())
        })?;
        let mut content = String::new();
        manifest_file.read_to_string(&mut content)?;
        serde_json::from_str(&content)?
    };

    info!(
        pack = %manifest.name,
        version = %manifest.version,
        "Applying CurseForge overrides and files"
    );

    let overrides_folder = manifest.overrides.unwrap_or_else(|| "overrides".to_string());
    let mut overrides_applied = 0usize;

    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| CraftError::Other(e.to_string()))?;
        let entry_name = entry.name().to_string();

        let rel_dest = if entry_name.starts_with(&format!("{}/", overrides_folder)) {
            Some(entry_name.trim_start_matches(&format!("{}/", overrides_folder)).to_string())
        } else if entry_name.starts_with("server-overrides/") {
            Some(entry_name.trim_start_matches("server-overrides/").to_string())
        } else {
            None
        };

        if let Some(dest_subpath) = rel_dest {
            if dest_subpath.is_empty() || dest_subpath.ends_with('/') {
                continue;
            }
            let target_file = target_dir.join(dest_subpath);
            if let Some(parent) = target_file.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut out = File::create(&target_file)?;
            std::io::copy(&mut entry, &mut out)?;
            overrides_applied += 1;
        }
    }

    Ok(ModpackInstallSummary {
        name: manifest.name,
        version: manifest.version,
        files_downloaded: 0,
        files_cached: 0,
        overrides_applied,
        total_bytes: 0,
    })
}

fn apply_overrides_from_zip(
    zip: &mut ZipArchive<BufReader<File>>,
    target_dir: &Path,
) -> Result<usize> {
    let mut overrides_count = 0usize;

    // First pass: extract overrides/
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| CraftError::Other(e.to_string()))?;
        let name = entry.name().to_string();
        if name.starts_with("overrides/") && !name.ends_with('/') {
            let rel = name.trim_start_matches("overrides/");
            let dest = target_dir.join(rel);
            if let Some(p) = dest.parent() {
                fs::create_dir_all(p)?;
            }
            let mut out = File::create(&dest)?;
            std::io::copy(&mut entry, &mut out)?;
            overrides_count += 1;
        }
    }

    // Second pass: extract server-overrides/ (taking priority over overrides/)
    for i in 0..zip.len() {
        let mut entry = zip.by_index(i).map_err(|e| CraftError::Other(e.to_string()))?;
        let name = entry.name().to_string();
        if name.starts_with("server-overrides/") && !name.ends_with('/') {
            let rel = name.trim_start_matches("server-overrides/");
            let dest = target_dir.join(rel);
            if let Some(p) = dest.parent() {
                fs::create_dir_all(p)?;
            }
            let mut out = File::create(&dest)?;
            std::io::copy(&mut entry, &mut out)?;
            overrides_count += 1;
        }
    }

    Ok(overrides_count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use zip::write::SimpleFileOptions;
    use zip::ZipWriter;

    #[test]
    fn test_modrinth_eligibility() {
        let client_only = ModrinthFileEntry {
            path: "mods/appleskin.jar".to_string(),
            hashes: ModrinthHashes {
                sha1: None,
                sha512: None,
            },
            env: Some(ModpackEnv {
                client: Some("required".to_string()),
                server: Some("unsupported".to_string()),
            }),
            downloads: vec!["https://example.com/appleskin.jar".to_string()],
            file_size: Some(1024),
        };
        assert!(!client_only.is_server_eligible());

        let server_eligible = ModrinthFileEntry {
            path: "mods/fabric-api.jar".to_string(),
            hashes: ModrinthHashes {
                sha1: None,
                sha512: None,
            },
            env: Some(ModpackEnv {
                client: Some("optional".to_string()),
                server: Some("required".to_string()),
            }),
            downloads: vec!["https://example.com/fabric-api.jar".to_string()],
            file_size: Some(2048),
        };
        assert!(server_eligible.is_server_eligible());

        let unmentioned = ModrinthFileEntry {
            path: "mods/common.jar".to_string(),
            hashes: ModrinthHashes {
                sha1: None,
                sha512: None,
            },
            env: None,
            downloads: vec![],
            file_size: None,
        };
        assert!(unmentioned.is_server_eligible());
    }

    #[test]
    fn test_inspect_modrinth_pack() {
        let temp_dir = tempfile::tempdir().unwrap();
        let archive_path = temp_dir.path().join("testpack.mrpack");

        {
            let file = File::create(&archive_path).unwrap();
            let mut zip = ZipWriter::new(file);
            let options = SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);

            let index_json = serde_json::json!({
                "formatVersion": 1,
                "game": "minecraft",
                "versionId": "1.0.0",
                "name": "Super Pack",
                "summary": "Awesome pack",
                "files": [
                    {
                        "path": "mods/server_mod.jar",
                        "hashes": {},
                        "env": { "server": "required" },
                        "downloads": ["http://example.com/server.jar"]
                    },
                    {
                        "path": "mods/client_mod.jar",
                        "hashes": {},
                        "env": { "server": "unsupported" },
                        "downloads": ["http://example.com/client.jar"]
                    }
                ],
                "dependencies": {
                    "minecraft": "1.20.1",
                    "fabric-loader": "0.15.7"
                }
            });

            zip.start_file("modrinth.index.json", options).unwrap();
            use std::io::Write;
            zip.write_all(index_json.to_string().as_bytes()).unwrap();

            zip.start_file("overrides/config/common.cfg", options).unwrap();
            zip.write_all(b"test=true").unwrap();

            zip.finish().unwrap();
        }

        let summary = inspect_modpack_archive(&archive_path).unwrap();
        assert_eq!(summary.kind, ModpackKind::Modrinth);
        assert_eq!(summary.name, "Super Pack");
        assert_eq!(summary.version, "1.0.0");
        assert_eq!(summary.game_version.as_deref(), Some("1.20.1"));
        assert!(summary.loader.unwrap().contains("fabric-loader"));
        assert_eq!(summary.total_files, 2);
        assert_eq!(summary.server_eligible_files, 1);
        assert_eq!(summary.client_only_files, 1);
        assert!(summary.has_overrides);
        assert!(!summary.has_server_overrides);
    }

    #[test]
    fn test_inspect_curseforge_pack() {
        let temp_dir = tempfile::tempdir().unwrap();
        let archive_path = temp_dir.path().join("cursepack.zip");

        {
            let file = File::create(&archive_path).unwrap();
            let mut zip = ZipWriter::new(file);
            let options = SimpleFileOptions::default()
                .compression_method(zip::CompressionMethod::Stored);

            let manifest_json = serde_json::json!({
                "manifestType": "minecraftModpack",
                "manifestVersion": 1,
                "name": "Forge Pack",
                "version": "2.5",
                "author": "Modder",
                "minecraft": {
                    "version": "1.20.1",
                    "modLoaders": [
                        { "id": "forge-47.2.0", "primary": true }
                    ]
                },
                "files": [
                    { "projectID": 1234, "fileID": 5678, "required": true }
                ],
                "overrides": "overrides"
            });

            zip.start_file("manifest.json", options).unwrap();
            use std::io::Write;
            zip.write_all(manifest_json.to_string().as_bytes()).unwrap();
            zip.finish().unwrap();
        }

        let summary = inspect_modpack_archive(&archive_path).unwrap();
        assert_eq!(summary.kind, ModpackKind::CurseForge);
        assert_eq!(summary.name, "Forge Pack");
        assert_eq!(summary.version, "2.5");
        assert_eq!(summary.game_version.as_deref(), Some("1.20.1"));
        assert_eq!(summary.loader.as_deref(), Some("forge-47.2.0"));
        assert_eq!(summary.total_files, 1);
    }
}
