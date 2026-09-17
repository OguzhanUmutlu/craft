use crate::bundled::{parse_bundled_manifest, BUNDLED_PURPUR};
use crate::traits::{AssetDownload, ServerEdition, ServerSoftware};
use craft_core::Result;
use serde::Deserialize;
use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

#[derive(Deserialize)]
struct PurpurResponse {
    versions: Vec<String>,
}

pub struct PurpurProvider {
    bundled: HashMap<String, String>,
}

impl Default for PurpurProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl PurpurProvider {
    pub fn new() -> Self {
        let (_, bundled) = parse_bundled_manifest(BUNDLED_PURPUR);
        Self { bundled }
    }
}

impl ServerSoftware for PurpurProvider {
    fn id(&self) -> &'static str {
        "purpur"
    }

    fn name(&self) -> &'static str {
        "Purpur"
    }

    fn edition(&self) -> ServerEdition {
        ServerEdition::Java
    }

    fn supports_plugins(&self) -> bool {
        true
    }

    fn supports_mods(&self) -> bool {
        false
    }

    fn supports_datapacks(&self) -> bool {
        true
    }

    fn description(&self) -> &'static str {
        "Paper fork with extensive gameplay tweaks"
    }

    fn bundled_versions(&self) -> Vec<String> {
        let mut v: Vec<String> = self.bundled.keys().cloned().collect();
        craft_core::sort_versions_descending(&mut v);
        v
    }

    fn fetch_versions<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + 'a>> {
        Box::pin(async move {
            let url = "https://api.purpurmc.org/v2/purpur";
            if let Ok(cache) = crate::cache::CacheManager::from_default_paths() {
                if let Ok(data) = cache
                    .get_cached_json::<PurpurResponse>(
                        "purpur_versions",
                        url,
                        std::time::Duration::from_secs(6 * 3600),
                    )
                    .await
                {
                    let mut versions = data.versions;
                    craft_core::sort_versions_descending(&mut versions);
                    return Ok(versions);
                }
            }
            let client = reqwest::Client::new();
            if let Ok(resp) = client.get(url).send().await {
                if let Ok(data) = resp.json::<PurpurResponse>().await {
                    let mut versions = data.versions;
                    craft_core::sort_versions_descending(&mut versions);
                    return Ok(versions);
                }
            }
            Ok(self.bundled_versions())
        })
    }

    fn get_assets(&self, version: &str) -> Result<Vec<AssetDownload>> {
        let url = if let Some(u) = self.bundled.get(version) {
            u.clone()
        } else {
            format!(
                "https://api.purpurmc.org/v2/purpur/{}/latest/download",
                version
            )
        };

        Ok(vec![AssetDownload {
            filename: "server.jar".to_string(),
            url,
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
