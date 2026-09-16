use std::path::{Path, PathBuf};
use craft_core::{CraftError, Result};

pub mod modrinth;
pub mod hangar;
pub mod poggit;
pub mod world;

pub use modrinth::{ModrinthClient, ModrinthHit, ModrinthFile};
pub use hangar::{HangarClient, HangarProject};
pub use poggit::{PoggitClient, PoggitPlugin};
pub use world::{
    CuratedMap, InstalledWorldItem, WorldMetadataSummary, PlayerDataSummary,
    AdvancementEntry, DataStorageEntry, get_curated_maps, search_curated_maps,
    list_installed_worlds, install_world_from_url, install_world_from_zip,
    inspect_world_metadata, list_world_player_data, list_world_advancements,
    list_world_data_storages, resolve_usercache_name,
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
        let store = craft_core::CraftPaths::new()
            .ok()
            .and_then(|paths| {
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
            let cached_path = match store.get_artifact(&rel_subpath) {
                Some(path) => path,
                None => {
                    let resp = reqwest::get(url).await
                        .map_err(|e| CraftError::Download(format!("Download failed: {}", e)))?;
                    let bytes = resp.bytes().await
                        .map_err(|e| CraftError::Download(format!("Failed to read bytes: {}", e)))?;
                    let (path, _) = store.put_artifact(&rel_subpath, &bytes, None)?;
                    path
                }
            };
            store.link_or_copy(&cached_path, &dest)?;
            Ok(dest)
        } else {
            let resp = reqwest::get(url).await
                .map_err(|e| CraftError::Download(format!("Download failed: {}", e)))?;
            let bytes = resp.bytes().await
                .map_err(|e| CraftError::Download(format!("Failed to read bytes: {}", e)))?;
            std::fs::write(&dest, &bytes)?;
            Ok(dest)
        }
    }

    pub async fn install_from_modrinth(&self, server_path: &Path, project_id: &str) -> Result<PathBuf> {
        let file_info = self.modrinth.get_latest_file(project_id).await?;
        let plugins_dir = server_path.join("plugins");
        self.install_artifact_cached("plugins", &file_info.filename, &file_info.url, &plugins_dir).await
    }

    pub async fn install_mod_from_modrinth(&self, server_path: &Path, project_id: &str) -> Result<PathBuf> {
        let file_info = self.modrinth.get_latest_file(project_id).await?;
        let mods_dir = server_path.join("mods");
        self.install_artifact_cached("mods", &file_info.filename, &file_info.url, &mods_dir).await
    }

    pub async fn install_datapack_from_modrinth(&self, server_path: &Path, project_id: &str, target_world: &str) -> Result<PathBuf> {
        let file_info = self.modrinth.get_latest_file(project_id).await?;
        let datapacks_dir = server_path.join(target_world).join("datapacks");
        self.install_artifact_cached("datapacks", &file_info.filename, &file_info.url, &datapacks_dir).await
    }
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
        pm.store.as_ref().unwrap().put_artifact("plugins/TestPlugin.jar", fake_plugin_data, None).unwrap();

        // Calling install_artifact_cached directly should hit cache without network
        let plugins_dir = server_dir.join("plugins");
        let installed = pm.install_artifact_cached(
            "plugins",
            "TestPlugin.jar",
            "http://invalid.url.that.should.not.be.called",
            &plugins_dir,
        ).await.unwrap();

        assert!(installed.exists());
        assert_eq!(std::fs::read(installed).unwrap(), fake_plugin_data);
    }
}

