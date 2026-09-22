use crate::manifest::inspect_jar_manifest;
use crate::modrinth::ModrinthClient;
use craft_core::{CraftError, CraftPaths, Result, TrashManager};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha512};
use std::fs::{self, File};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PluginUpdateCandidate {
    pub file_name: String,
    pub file_path: PathBuf,
    pub installed_version: String,
    pub latest_version: String,
    pub project_name: String,
    pub project_id: String,
    pub download_url: String,
    pub sha512: String,
    pub has_update: bool,
}

/// Computes the SHA-512 cryptographic digest of a file for Modrinth version file matching
pub fn compute_file_sha512(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha512::new();
    let mut buffer = [0u8; 64 * 1024];

    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }

    Ok(hex::encode(hasher.finalize()))
}

/// Scans installed plugins and mods and checks Modrinth for available updates
pub async fn check_server_updates(
    server_path: &Path,
    server_version: Option<&str>,
    loader: Option<&str>,
) -> Result<Vec<PluginUpdateCandidate>> {
    let mut jar_files = Vec::new();

    for sub in &["plugins", "mods"] {
        let dir = server_path.join(sub);
        if dir.exists() {
            if let Ok(entries) = fs::read_dir(&dir) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("jar") {
                        jar_files.push(p);
                    }
                }
            }
        }
    }

    if jar_files.is_empty() {
        return Ok(Vec::new());
    }

    let mut hashes = Vec::new();
    let mut file_info = Vec::new();

    for jar in jar_files {
        let manifest = inspect_jar_manifest(&jar).ok();
        let fname = jar
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("unknown.jar")
            .to_string();

        let inst_ver = manifest
            .as_ref()
            .map(|m| m.version.clone())
            .unwrap_or_else(|| "unknown".to_string());

        let proj_name = manifest
            .as_ref()
            .and_then(|m| m.display_name.clone())
            .unwrap_or_else(|| fname.clone());

        if let Ok(hash) = compute_file_sha512(&jar) {
            hashes.push(hash.clone());
            file_info.push((jar, fname, inst_ver, proj_name, hash));
        }
    }

    let client = ModrinthClient::new();
    let loaders: Vec<String> = loader.map(|l| vec![l.to_string()]).unwrap_or_default();
    let gvs: Vec<String> = server_version.map(|v| vec![v.to_string()]).unwrap_or_default();

    let update_map = client
        .check_updates_by_hashes(&hashes, &loaders, &gvs)
        .await
        .unwrap_or_default();

    let mut candidates = Vec::new();

    for (path, fname, inst_ver, proj_name, hash) in file_info {
        if let Some(upstream_ver) = update_map.get(&hash) {
            let latest_version_number = upstream_ver.version_number.clone();
            let primary_file = upstream_ver.files.iter().find(|f| f.primary).or_else(|| upstream_ver.files.first());

            if let Some(file) = primary_file {
                let has_update = inst_ver != latest_version_number && inst_ver != "unknown";
                candidates.push(PluginUpdateCandidate {
                    file_name: fname,
                    file_path: path,
                    installed_version: inst_ver,
                    latest_version: latest_version_number,
                    project_name: proj_name,
                    project_id: upstream_ver.id.clone(),
                    download_url: file.url.clone(),
                    sha512: hash,
                    has_update,
                });
            }
        }
    }

    Ok(candidates)
}

/// Applies an update atomically to a plugin or mod file with automatic rollback protection
pub async fn apply_atomic_update(
    candidate: &PluginUpdateCandidate,
    paths: &CraftPaths,
) -> Result<PathBuf> {
    if !candidate.file_path.exists() {
        return Err(CraftError::InvalidPath(format!(
            "Target plugin file '{}' does not exist.",
            candidate.file_path.display()
        )));
    }

    // 1. Stage backup rollback file
    let backup_path = candidate.file_path.with_extension("jar.upgrade_bak");
    fs::copy(&candidate.file_path, &backup_path).map_err(|e| {
        CraftError::Other(format!(
            "Failed to create rollback backup for '{}': {}",
            candidate.file_name, e
        ))
    })?;

    // 2. Download new version to temporary directory
    let temp_dir = tempfile::tempdir().map_err(|e| {
        CraftError::Other(format!("Failed to create temporary download directory: {}", e))
    })?;
    let downloaded_temp = temp_dir.path().join(&candidate.file_name);

    let resp = reqwest::get(&candidate.download_url).await.map_err(|e| {
        let _ = fs::remove_file(&backup_path);
        CraftError::Download(format!(
            "Failed to download update for '{}': {}",
            candidate.project_name, e
        ))
    })?;

    if !resp.status().is_success() {
        let _ = fs::remove_file(&backup_path);
        return Err(CraftError::Download(format!(
            "Update download failed with status {}: {}",
            resp.status(),
            candidate.download_url
        )));
    }

    let bytes = resp.bytes().await.map_err(|e| {
        let _ = fs::remove_file(&backup_path);
        CraftError::Download(format!("Failed to read downloaded bytes: {}", e))
    })?;

    if let Err(e) = fs::write(&downloaded_temp, &bytes) {
        let _ = fs::remove_file(&backup_path);
        return Err(CraftError::Io(e));
    }

    // 3. Atomically overwrite
    if let Err(_e) = fs::rename(&downloaded_temp, &candidate.file_path) {
        // If rename across filesystems fails, attempt copy
        if let Err(copy_err) = fs::copy(&downloaded_temp, &candidate.file_path) {
            // Restore from backup
            let _ = fs::copy(&backup_path, &candidate.file_path);
            let _ = fs::remove_file(&backup_path);
            return Err(CraftError::Other(format!(
                "Failed to replace plugin file: {}. Restored from backup.",
                copy_err
            )));
        }
    }

    // 4. Archive old backup into non-destructive trash bin
    let trash_mgr = TrashManager::new(paths);
    let _ = trash_mgr.trash_path(&backup_path, Some("plugin_upgrade_backup"));
    let _ = fs::remove_file(&backup_path);

    Ok(candidate.file_path.clone())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_compute_file_sha512() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.jar");
        fs::write(&file_path, b"test content for sha512").unwrap();

        let hash = compute_file_sha512(&file_path).unwrap();
        assert_eq!(hash.len(), 128); // SHA-512 hex is 128 characters
    }
}
