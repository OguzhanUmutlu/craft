use std::path::{Path, PathBuf};
use craft_core::{CraftError, Result};

pub mod modrinth;
pub mod hangar;
pub mod poggit;

pub use modrinth::{ModrinthClient, ModrinthHit, ModrinthFile};
pub use hangar::{HangarClient, HangarProject};
pub use poggit::{PoggitClient, PoggitPlugin};

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
}

impl PluginManager {
    pub fn new() -> Self {
        Self {
            modrinth: ModrinthClient::new(),
            hangar: HangarClient::new(),
            poggit: PoggitClient::new(),
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

    pub async fn install_from_modrinth(&self, server_path: &Path, project_id: &str) -> Result<PathBuf> {
        let file_info = self.modrinth.get_latest_file(project_id).await?;
        let plugins_dir = server_path.join("plugins");
        std::fs::create_dir_all(&plugins_dir)?;

        let dest = plugins_dir.join(&file_info.filename);
        let resp = reqwest::get(&file_info.url).await
            .map_err(|e| CraftError::Download(format!("Download failed: {}", e)))?;

        let bytes = resp.bytes().await
            .map_err(|e| CraftError::Download(format!("Failed to read plugin bytes: {}", e)))?;

        std::fs::write(&dest, &bytes)?;
        Ok(dest)
    }
}
