use craft_core::{CraftError, Result};
use std::path::{Path, PathBuf};

pub mod hangar;
pub mod map_resolver;
pub mod modrinth;
pub mod poggit;
pub mod saves;
pub mod world;

pub use hangar::{HangarClient, HangarProject};
pub use map_resolver::resolve_map_download_url;
pub use modrinth::{ModrinthClient, ModrinthFile, ModrinthHit};
pub use poggit::{PoggitClient, PoggitPlugin};
pub use saves::{list_saves_for_server, SaveItem};
pub use world::{
    get_curated_maps, inspect_world_metadata, install_cached_map, install_world_from_url,
    install_world_from_zip, list_cached_maps, list_installed_worlds, list_world_advancements,
    list_world_data_storages, list_world_player_data, resolve_usercache_name, search_curated_maps,
    AdvancementEntry, CuratedMap, DataStorageEntry, InstalledWorldItem, PlayerDataSummary,
    WorldMetadataSummary,
};

#[derive(Debug, Clone)]
pub struct UnifiedPluginHit {
    pub name: String,
    pub description: String,
    pub source: &'static str,
    pub id_or_slug: String,
}

pub struct PluginManager {
    modrinth: ModrinthClient,
    hangar: HangarClient,
    poggit: PoggitClient,
    store: Option<craft_core::CacheStore>,
}

impl Default for PluginManager {
    fn default() -> Self {
        Self::new()
    }
}

impl PluginManager {
    pub fn new() -> Self {
        let store = craft_core::CraftPaths::new().ok().and_then(|paths| {
            let settings = craft_core::GlobalSettings::load(&paths).unwrap_or_default();
            craft_core::CacheStore::new(paths.cache_dir, settings.cache_max_bytes).ok()
        });

        Self {
            modrinth: ModrinthClient::new(),
            hangar: HangarClient::new(),
            poggit: PoggitClient::new(),
            store,
        }
    }

    pub fn with_store(store: craft_core::CacheStore) -> Self {
        Self {
            modrinth: ModrinthClient::new(),
            hangar: HangarClient::new(),
            poggit: PoggitClient::new(),
            store: Some(store),
        }
    }

    pub async fn search(&self, query: &str) -> Vec<UnifiedPluginHit> {
        let mut results = Vec::new();

        // 1. Search Modrinth
        if let Ok(hits) = self.modrinth.search(query, Some("plugin")).await {
            for hit in hits {
                results.push(UnifiedPluginHit {
                    name: hit.title,
                    description: hit.description,
                    source: "Modrinth",
                    id_or_slug: hit.project_id,
                });
            }
        }

        // 2. Search Hangar
        if let Ok(projects) = self.hangar.search(query).await {
            for p in projects {
                results.push(UnifiedPluginHit {
                    name: p.name,
                    description: p.description.unwrap_or_default(),
                    source: "Hangar",
                    id_or_slug: format!("{}/{}", p.namespace.owner, p.namespace.slug),
                });
            }
        }

        // 3. Search Poggit
        if let Ok(plugins) = self.poggit.search(query).await {
            for p in plugins {
                results.push(UnifiedPluginHit {
                    name: p.name.clone(),
                    description: p.tagline.unwrap_or_default(),
                    source: "Poggit",
                    id_or_slug: p.name,
                });
            }
        }

        results
    }

    pub async fn search_mods(&self, query: &str) -> Vec<UnifiedPluginHit> {
        let mut results = Vec::new();
        if let Ok(hits) = self.modrinth.search(query, Some("mod")).await {
            for hit in hits {
                results.push(UnifiedPluginHit {
                    name: hit.title,
                    description: hit.description,
                    source: "Modrinth",
                    id_or_slug: hit.project_id,
                });
            }
        }
        results
    }

    pub async fn search_datapacks(&self, query: &str) -> Vec<UnifiedPluginHit> {
        let mut results = Vec::new();
        if let Ok(hits) = self.modrinth.search(query, Some("datapack")).await {
            for hit in hits {
                results.push(UnifiedPluginHit {
                    name: hit.title,
                    description: hit.description,
                    source: "Modrinth",
                    id_or_slug: hit.project_id,
                });
            }
        }
        results
    }

    async fn install_artifact_cached(
        &self,
        category: &str,
        filename: &str,
        url: &str,
        destination_dir: &Path,
    ) -> Result<PathBuf> {
        std::fs::create_dir_all(destination_dir)?;
        let dest = destination_dir.join(filename);

        if let Some(ref store) = self.store {
            let rel_subpath = format!("{}/{}", category, filename);
            if !store.has_artifact(&rel_subpath) {
                let resp = reqwest::get(url)
                    .await
                    .map_err(|e| CraftError::Download(format!("Download failed: {}", e)))?;
                if !resp.status().is_success() {
                    return Err(CraftError::Download(format!(
                        "Download failed with status {}: {}",
                        resp.status(),
                        url
                    )));
                }
                let bytes = resp
                    .bytes()
                    .await
                    .map_err(|e| CraftError::Download(format!("Failed to read bytes: {}", e)))?;
                store.put_artifact_compressed(
                    &rel_subpath,
                    &bytes,
                    Some(filename),
                    Some(category),
                    None,
                )?;
            }
            store.extract_artifact_to(&rel_subpath, &dest)?;
            Ok(dest)
        } else {
            let resp = reqwest::get(url)
                .await
                .map_err(|e| CraftError::Download(format!("Download failed: {}", e)))?;
            if !resp.status().is_success() {
                return Err(CraftError::Download(format!(
                    "Download failed with status {}: {}",
                    resp.status(),
                    url
                )));
            }
            let bytes = resp
                .bytes()
                .await
                .map_err(|e| CraftError::Download(format!("Failed to read bytes: {}", e)))?;
            std::fs::write(&dest, &bytes)?;
            Ok(dest)
        }
    }

    pub fn list_cached(&self) -> Vec<craft_core::CacheEntryMeta> {
        if let Some(ref store) = self.store {
            store.list_cached_artifacts(Some("plugin"))
        } else {
            Vec::new()
        }
    }

    pub fn install_cached(&self, rel_subpath: &str, destination_dir: &Path) -> Result<PathBuf> {
        if let Some(ref store) = self.store {
            std::fs::create_dir_all(destination_dir)?;
            let raw_name = Path::new(rel_subpath)
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| {
                    CraftError::Other(format!("Invalid plugin subpath: {}", rel_subpath))
                })?;
            let filename = raw_name.strip_suffix(".zst").unwrap_or(raw_name);
            let dest = destination_dir.join(filename);
            store.extract_artifact_to(rel_subpath, &dest)?;
            Ok(dest)
        } else {
            Err(CraftError::Other(
                "Cache store is not configured".to_string(),
            ))
        }
    }

    pub async fn install_from_modrinth(
        &self,
        server_path: &Path,
        project_id: &str,
    ) -> Result<PathBuf> {
        let file_info = self.modrinth.get_latest_file(project_id).await?;
        let plugins_dir = server_path.join("plugins");
        self.install_artifact_cached("plugins", &file_info.filename, &file_info.url, &plugins_dir)
            .await
    }

    pub async fn install_mod_from_modrinth(
        &self,
        server_path: &Path,
        project_id: &str,
    ) -> Result<PathBuf> {
        let file_info = self.modrinth.get_latest_file(project_id).await?;
        let mods_dir = server_path.join("mods");
        self.install_artifact_cached("mods", &file_info.filename, &file_info.url, &mods_dir)
            .await
    }

    pub async fn install_datapack_from_modrinth(
        &self,
        server_path: &Path,
        project_id: &str,
        target_world: &str,
    ) -> Result<PathBuf> {
        let file_info = self.modrinth.get_latest_file(project_id).await?;
        let datapacks_dir = server_path.join(target_world).join("datapacks");
        self.install_artifact_cached(
            "datapacks",
            &file_info.filename,
            &file_info.url,
            &datapacks_dir,
        )
        .await
    }
}

pub fn list_cached_plugins() -> Vec<craft_core::CacheEntryMeta> {
    PluginManager::new().list_cached()
}

pub fn install_cached_plugin(rel_subpath: &str, destination_dir: &Path) -> Result<PathBuf> {
    PluginManager::new().install_cached(rel_subpath, destination_dir)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_plugin_manager_cached_installation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let server_dir = temp_dir.path().join("server");
        let store = craft_core::CacheStore::new(cache_dir, 10 * 1024 * 1024).unwrap();
        let pm = PluginManager::with_store(store);

        // Pre-populate cache with a mock plugin
        let fake_plugin_data = b"dummy-plugin-bytes";
        pm.store
            .as_ref()
            .unwrap()
            .put_artifact("plugins/TestPlugin.jar", fake_plugin_data, None)
            .unwrap();

        // Calling install_artifact_cached directly should hit cache without network
        let plugins_dir = server_dir.join("plugins");
        let installed = pm
            .install_artifact_cached(
                "plugins",
                "TestPlugin.jar",
                "http://invalid.url.that.should.not.be.called",
                &plugins_dir,
            )
            .await
            .unwrap();

        assert!(installed.exists());
        assert_eq!(std::fs::read(installed).unwrap(), fake_plugin_data);
    }

    #[tokio::test]
    async fn test_plugin_manager_compressed_cached_installation() {
        let temp_dir = tempfile::tempdir().unwrap();
        let cache_dir = temp_dir.path().join("cache");
        let server_dir = temp_dir.path().join("server");
        let store = craft_core::CacheStore::new(cache_dir, 10 * 1024 * 1024).unwrap();
        let pm = PluginManager::with_store(store);

        let fake_plugin_data = b"compressed-dummy-plugin-bytes-which-should-be-compressed";
        pm.store
            .as_ref()
            .unwrap()
            .put_artifact_compressed(
                "plugins/EssentialsX.jar",
                fake_plugin_data,
                Some("EssentialsX.jar"),
                Some("plugin"),
                None,
            )
            .unwrap();

        let cached = pm.list_cached();
        assert_eq!(cached.len(), 1);
        assert_eq!(cached[0].display_title(), "EssentialsX.jar");
        assert!(cached[0].is_compressed);

        let plugins_dir = server_dir.join("plugins");
        let installed = pm
            .install_cached("plugins/EssentialsX.jar", &plugins_dir)
            .unwrap();

        assert!(installed.exists());
        assert_eq!(std::fs::read(installed).unwrap(), fake_plugin_data);
    }
}
