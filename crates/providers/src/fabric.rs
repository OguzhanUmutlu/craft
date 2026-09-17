use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use serde::Deserialize;
use craft_core::Result;
use crate::traits::{AssetDownload, ServerEdition, ServerSoftware};

#[derive(Deserialize)]
struct FabricGameVersion {
    version: String,
    stable: bool,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct FabricLoaderVersion {
    version: String,
    stable: bool,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct FabricInstallerVersion {
    version: String,
    stable: bool,
}

pub struct FabricProvider;

impl Default for FabricProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl FabricProvider {
    pub fn new() -> Self {
        Self
    }
}

impl ServerSoftware for FabricProvider {
    fn id(&self) -> &'static str {
        "fabric"
    }

    fn name(&self) -> &'static str {
        "Fabric"
    }

    fn edition(&self) -> ServerEdition {
        ServerEdition::Java
    }

    fn supports_plugins(&self) -> bool {
        false
    }

    fn supports_mods(&self) -> bool {
        true
    }

    fn supports_datapacks(&self) -> bool {
        true
    }

    fn description(&self) -> &'static str {
        "Lightweight modular modded server"
    }

    fn bundled_versions(&self) -> Vec<String> {
        vec![
            "1.21.4".into(), "1.21.3".into(), "1.21.1".into(), "1.21".into(),
            "1.20.6".into(), "1.20.4".into(), "1.20.2".into(), "1.20.1".into(),
            "1.19.4".into(), "1.19.2".into(), "1.18.2".into(), "1.16.5".into(),
        ]
    }

    fn fetch_versions<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + 'a>> {
        Box::pin(async move {
            let url = "https://meta.fabricmc.net/v2/versions/game";
            if let Ok(cache) = crate::cache::CacheManager::from_default_paths() {
                if let Ok(list) = cache.get_cached_json::<Vec<FabricGameVersion>>("fabric_game_versions", url, std::time::Duration::from_secs(6 * 3600)).await {
                    let mut versions: Vec<String> = list.into_iter()
                        .filter(|g| g.stable)
                        .map(|g| g.version)
                        .collect();
                    if !versions.is_empty() {
                        craft_core::sort_versions_descending(&mut versions);
                        return Ok(versions);
                    }
                }
            }
            let client = reqwest::Client::new();
            if let Ok(resp) = client.get(url).send().await {
                if let Ok(list) = resp.json::<Vec<FabricGameVersion>>().await {
                    let mut versions: Vec<String> = list.into_iter()
                        .filter(|g| g.stable)
                        .map(|g| g.version)
                        .collect();
                    if !versions.is_empty() {
                        craft_core::sort_versions_descending(&mut versions);
                        return Ok(versions);
                    }
                }
            }
            Ok(self.bundled_versions())
        })
    }

    fn get_assets(&self, version: &str) -> Result<Vec<AssetDownload>> {
        // Use Fabric Meta's server/jar endpoint with latest loader & installer
        // https://meta.fabricmc.net/v2/versions/loader/{game_version}/0.16.14/1.1.0/server/jar
        let url = format!(
            "https://meta.fabricmc.net/v2/versions/loader/{}/0.16.14/1.1.0/server/jar",
            version
        );

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
