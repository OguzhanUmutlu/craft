use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use serde::Deserialize;
use craft_core::{CraftError, Result};
use crate::bundled::{parse_bundled_manifest, BUNDLED_FOLIA, BUNDLED_PAPER};
use crate::traits::{AssetDownload, ServerEdition, ServerSoftware};

#[derive(Deserialize)]
struct PaperV3ProjectResponse {
    versions: HashMap<String, Vec<String>>,
}

pub struct PaperProvider {
    project: &'static str,
    name: &'static str,
    edition: ServerEdition,
    bundled: HashMap<String, String>,
    default_versions: Vec<String>,
}

impl PaperProvider {
    pub fn new_paper() -> Self {
        let (_, bundled) = parse_bundled_manifest(BUNDLED_PAPER);
        Self {
            project: "paper",
            name: "Paper",
            edition: ServerEdition::Java,
            bundled,
            default_versions: vec![
                "1.21.4".into(), "1.21.3".into(), "1.21.1".into(), "1.21".into(),
                "1.20.6".into(), "1.20.4".into(), "1.20.2".into(), "1.20.1".into(),
                "1.19.4".into(), "1.18.2".into(), "1.16.5".into(),
            ],
        }
    }

    pub fn new_folia() -> Self {
        let (_, bundled) = parse_bundled_manifest(BUNDLED_FOLIA);
        Self {
            project: "folia",
            name: "Folia",
            edition: ServerEdition::Java,
            bundled,
            default_versions: vec![
                "1.21.4".into(), "1.21.3".into(), "1.21.1".into(), "1.20.6".into(), "1.20.4".into(),
            ],
        }
    }

    pub fn new_velocity() -> Self {
        Self {
            project: "velocity",
            name: "Velocity",
            edition: ServerEdition::Proxy,
            bundled: HashMap::new(),
            default_versions: vec![
                "3.4.0".into(), "3.3.0-SNAPSHOT".into(), "3.2.0-SNAPSHOT".into(), "3.1.2-SNAPSHOT".into(),
            ],
        }
    }

    pub fn new_waterfall() -> Self {
        Self {
            project: "waterfall",
            name: "Waterfall",
            edition: ServerEdition::Proxy,
            bundled: HashMap::new(),
            default_versions: vec![
                "1.21".into(), "1.20".into(), "1.19".into(), "1.18".into(), "1.17".into(), "1.16".into(),
            ],
        }
    }

    async fn fetch_live_versions(&self) -> Result<Vec<String>> {
        let url = format!("https://fill.papermc.io/v3/projects/{}", self.project);
        let cache_key = format!("papermc_project_{}", self.project);

        let data: PaperV3ProjectResponse = if let Ok(cache) = crate::cache::CacheManager::from_default_paths() {
            match cache.get_cached_json(&cache_key, &url, std::time::Duration::from_secs(6 * 3600)).await {
                Ok(data) => data,
                Err(_) => {
                    let client = reqwest::Client::builder()
                        .user_agent("Craft-CLI/1.0 (https://github.com/OguzhanUmutlu/craft)")
                        .build()
                        .map_err(|e| CraftError::Download(format!("Failed to build HTTP client: {}", e)))?;
                    let resp = client.get(&url).send().await
                        .map_err(|e| CraftError::Download(format!("Failed to fetch PaperMC project info: {}", e)))?;
                    resp.json().await
                        .map_err(|e| CraftError::Download(format!("Invalid PaperMC API response: {}", e)))?
                }
            }
        } else {
            let client = reqwest::Client::builder()
                .user_agent("Craft-CLI/1.0 (https://github.com/OguzhanUmutlu/craft)")
                .build()
                .map_err(|e| CraftError::Download(format!("Failed to build HTTP client: {}", e)))?;
            let resp = client.get(&url).send().await
                .map_err(|e| CraftError::Download(format!("Failed to fetch PaperMC project info: {}", e)))?;
            resp.json().await
                .map_err(|e| CraftError::Download(format!("Invalid PaperMC API response: {}", e)))?
        };

        let mut all_versions = Vec::new();
        for (_, vers) in data.versions {
            all_versions.extend(vers);
        }

        all_versions.sort();
        all_versions.reverse();
        all_versions.dedup();
        Ok(all_versions)
    }

    fn resolve_live_build(&self, version: &str) -> Option<AssetDownload> {
        let handle = tokio::runtime::Handle::try_current().ok()?;
        let project = self.project;
        let version_str = version.to_string();

        tokio::task::block_in_place(|| {
            handle.block_on(async move {
                let url = format!("https://fill.papermc.io/v3/projects/{}/versions/{}/builds", project, version_str);
                let cache_key = format!("papermc_builds_{}_{}", project, version_str);

                let builds: Vec<serde_json::Value> = if let Ok(cache) = crate::cache::CacheManager::from_default_paths() {
                    match cache.get_cached_json(&cache_key, &url, std::time::Duration::from_secs(3600)).await {
                        Ok(b) => b,
                        Err(_) => {
                            let client = reqwest::Client::builder()
                                .user_agent("Craft-CLI/1.0 (https://github.com/OguzhanUmutlu/craft)")
                                .build().ok()?;
                            let resp = client.get(&url).send().await.ok()?;
                            resp.json().await.ok()?
                        }
                    }
                } else {
                    let client = reqwest::Client::builder()
                        .user_agent("Craft-CLI/1.0 (https://github.com/OguzhanUmutlu/craft)")
                        .build().ok()?;
                    let resp = client.get(&url).send().await.ok()?;
                    resp.json().await.ok()?
                };

                let chosen = builds
                    .iter()
                    .find(|b| b.get("channel").and_then(|c| c.as_str()) == Some("STABLE"))
                    .or_else(|| builds.first());

                if let Some(b) = chosen {
                    let downloads = b.get("downloads")?.as_object()?;
                    let dl = downloads.get("server:default")
                        .or_else(|| downloads.values().next())?;
                    let file_url = dl.get("url")?.as_str()?.to_string();
                    let sha256 = dl.get("checksums").and_then(|c| c.get("sha256")).and_then(|s| s.as_str()).map(|s| s.to_string());
                    let filename = "server.jar".to_string();

                    Some(AssetDownload {
                        filename,
                        url: file_url,
                        sha256,
                        is_archive: false,
                    })
                } else {
                    None
                }
            })
        })
    }
}

impl ServerSoftware for PaperProvider {
    fn id(&self) -> &'static str {
        self.project
    }

    fn name(&self) -> &'static str {
        self.name
    }

    fn edition(&self) -> ServerEdition {
        self.edition
    }

    fn description(&self) -> &'static str {
        match self.project {
            "paper" => "High-performance standard Java server (Rec.)",
            "folia" => "Multi-threaded regional ticking server",
            "velocity" => "Next-generation ultra-fast proxy (Rec.)",
            "waterfall" => "Optimized BungeeCord proxy fork",
            _ => "PaperMC platform",
        }
    }

    fn bundled_versions(&self) -> Vec<String> {
        if !self.bundled.is_empty() {
            let mut v: Vec<String> = self.bundled.keys().cloned().collect();
            v.sort();
            v.reverse();
            v
        } else {
            self.default_versions.clone()
        }
    }

    fn fetch_versions<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + 'a>> {
        Box::pin(async move {
            match self.fetch_live_versions().await {
                Ok(v) if !v.is_empty() => Ok(v),
                _ => Ok(self.bundled_versions()),
            }
        })
    }

    fn get_assets(&self, version: &str) -> Result<Vec<AssetDownload>> {
        // 1. Try live resolution via fill.papermc.io
        if let Some(asset) = self.resolve_live_build(version) {
            return Ok(vec![asset]);
        }

        // 2. Try bundled map
        if let Some(url) = self.bundled.get(version) {
            return Ok(vec![AssetDownload {
                filename: "server.jar".to_string(),
                url: url.clone(),
                sha256: None,
                is_archive: false,
            }]);
        }

        // 3. Fallback direct build format
        Ok(vec![AssetDownload {
            filename: "server.jar".to_string(),
            url: format!(
                "https://fill.papermc.io/v3/projects/{}/versions/{}/builds/latest",
                self.project, version
            ),
            sha256: None,
            is_archive: false,
        }])
    }

    fn post_download<'a>(
        &'a self,
        _server_path: &'a Path,
        _version: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }
}
