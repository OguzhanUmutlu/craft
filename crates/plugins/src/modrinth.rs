use craft_core::{CraftError, Result};
use serde::Deserialize;

#[derive(Debug, Clone, Deserialize)]
pub struct ModrinthSearchResponse {
    pub hits: Vec<ModrinthHit>,
    pub total_hits: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModrinthHit {
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub client_side: Option<String>,
    pub server_side: Option<String>,
    pub project_type: String,
    pub downloads: u64,
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModrinthVersion {
    pub id: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub files: Vec<ModrinthFile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModrinthFile {
    pub url: String,
    pub filename: String,
    pub primary: bool,
    pub size: u64,
}

pub struct ModrinthClient {
    client: reqwest::Client,
}

impl Default for ModrinthClient {
    fn default() -> Self {
        Self::new()
    }
}

impl ModrinthClient {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("Craft/1.0 (Minecraft Server Manager)")
            .build()
            .unwrap_or_default();
        Self { client }
    }

    pub async fn search(
        &self,
        query: &str,
        project_type: Option<&str>,
    ) -> Result<Vec<ModrinthHit>> {
        let pt_str = project_type.unwrap_or("all");
        let cache_key = format!("modrinth_search_{}_{}", pt_str, query);

        // Check cache (30 min TTL)
        if let Ok(paths) = craft_core::CraftPaths::new() {
            let settings = craft_core::GlobalSettings::load(&paths).unwrap_or_default();
            if let Ok(store) =
                craft_core::CacheStore::new(paths.cache_dir, settings.cache_max_bytes)
            {
                if let Ok(Some(bytes)) = store.get_metadata(&cache_key) {
                    if let Ok(data) = serde_json::from_slice::<ModrinthSearchResponse>(&bytes) {
                        return Ok(data.hits);
                    }
                }
            }
        }

        let mut url = format!(
            "https://api.modrinth.com/v2/search?query={}&limit=15",
            urlencoding(query)
        );

        if let Some(pt) = project_type {
            let facet = format!("[[\"project_type:{}\"]]", pt);
            url.push_str(&format!("&facets={}", urlencoding(&facet)));
        }

        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| CraftError::Download(format!("Modrinth search error: {}", e)))?;

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| CraftError::Download(format!("Modrinth response read error: {}", e)))?;

        let data: ModrinthSearchResponse = serde_json::from_slice(&bytes)
            .map_err(|e| CraftError::Download(format!("Invalid Modrinth response: {}", e)))?;

        if let Ok(paths) = craft_core::CraftPaths::new() {
            let settings = craft_core::GlobalSettings::load(&paths).unwrap_or_default();
            if let Ok(store) =
                craft_core::CacheStore::new(paths.cache_dir, settings.cache_max_bytes)
            {
                let _ = store.put_metadata(
                    &cache_key,
                    &bytes,
                    Some(std::time::Duration::from_secs(30 * 60)),
                );
            }
        }

        Ok(data.hits)
    }

    pub async fn get_latest_file(&self, project_id: &str) -> Result<ModrinthFile> {
        let cache_key = format!("modrinth_version_{}", project_id);
        if let Ok(paths) = craft_core::CraftPaths::new() {
            let settings = craft_core::GlobalSettings::load(&paths).unwrap_or_default();
            if let Ok(store) =
                craft_core::CacheStore::new(paths.cache_dir, settings.cache_max_bytes)
            {
                if let Ok(Some(bytes)) = store.get_metadata(&cache_key) {
                    if let Ok(versions) = serde_json::from_slice::<Vec<ModrinthVersion>>(&bytes) {
                        if let Some(first) = versions.into_iter().next() {
                            if let Some(primary) = first.files.into_iter().find(|f| f.primary) {
                                return Ok(primary);
                            }
                        }
                    }
                }
            }
        }

        let url = format!("https://api.modrinth.com/v2/project/{}/version", project_id);
        let resp = self
            .client
            .get(&url)
            .send()
            .await
            .map_err(|e| CraftError::Download(format!("Modrinth versions error: {}", e)))?;

        let bytes = resp.bytes().await.map_err(|e| {
            CraftError::Download(format!("Failed to read versions response: {}", e))
        })?;

        let versions: Vec<ModrinthVersion> = serde_json::from_slice(&bytes)
            .map_err(|e| CraftError::Download(format!("Invalid versions response: {}", e)))?;

        if let Ok(paths) = craft_core::CraftPaths::new() {
            let settings = craft_core::GlobalSettings::load(&paths).unwrap_or_default();
            if let Ok(store) =
                craft_core::CacheStore::new(paths.cache_dir, settings.cache_max_bytes)
            {
                let _ = store.put_metadata(
                    &cache_key,
                    &bytes,
                    Some(std::time::Duration::from_secs(30 * 60)),
                );
            }
        }

        let first = versions
            .into_iter()
            .next()
            .ok_or_else(|| CraftError::Other("No releases found for this project".to_string()))?;

        let primary_file = first.files.into_iter().find(|f| f.primary).ok_or_else(|| {
            CraftError::Other("No downloadable files found in release".to_string())
        })?;

        Ok(primary_file)
    }
}

fn urlencoding(input: &str) -> String {
    let mut out = String::new();
    for b in input.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}
