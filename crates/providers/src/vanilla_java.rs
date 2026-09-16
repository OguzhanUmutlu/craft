use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use serde::Deserialize;
use craft_core::{CraftError, Result};
use crate::bundled::{parse_bundled_manifest, BUNDLED_VANILLA_JAVA};
use crate::traits::{AssetDownload, ServerEdition, ServerSoftware};

#[derive(Deserialize)]
struct MojangManifest {
    versions: Vec<MojangVersionEntry>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct MojangVersionEntry {
    id: String,
    #[serde(rename = "type")]
    release_type: String,
    url: String,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct MojangPackage {
    downloads: Option<MojangDownloads>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct MojangDownloads {
    server: Option<MojangDownloadFile>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct MojangDownloadFile {
    sha1: String,
    url: String,
}

pub struct VanillaJavaProvider {
    bundled: HashMap<String, String>,
}

impl Default for VanillaJavaProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl VanillaJavaProvider {
    pub fn new() -> Self {
        let (_, bundled) = parse_bundled_manifest(BUNDLED_VANILLA_JAVA);
        Self { bundled }
    }

    async fn fetch_mojang_manifest(&self) -> Result<MojangManifest> {
        let url = "https://launchermeta.mojang.com/mc/game/version_manifest_v2.json";
        if let Ok(cache) = crate::cache::CacheManager::from_default_paths() {
            if let Ok(manifest) = cache.get_cached_json::<MojangManifest>(
                "mojang_version_manifest_v2",
                url,
                std::time::Duration::from_secs(6 * 3600),
            ).await {
                return Ok(manifest);
            }
        }
        let client = reqwest::Client::new();
        let resp = client.get(url)
            .send().await
            .map_err(|e| CraftError::Download(format!("Mojang manifest fetch error: {}", e)))?;
        resp.json().await
            .map_err(|e| CraftError::Download(format!("Invalid Mojang manifest: {}", e)))
    }
}

impl ServerSoftware for VanillaJavaProvider {
    fn id(&self) -> &'static str {
        "vanilla_java"
    }

    fn name(&self) -> &'static str {
        "Vanilla (Java)"
    }

    fn edition(&self) -> ServerEdition {
        ServerEdition::Java
    }

    fn supports_plugins(&self) -> bool {
        false
    }

    fn supports_mods(&self) -> bool {
        false
    }

    fn supports_datapacks(&self) -> bool {
        true
    }

    fn description(&self) -> &'static str {
        "Official Mojang Java dedicated server"
    }

    fn bundled_versions(&self) -> Vec<String> {
        let mut v: Vec<String> = self.bundled.keys().cloned().collect();
        v.sort();
        v.reverse();
        v
    }

    fn fetch_versions<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + 'a>> {
        Box::pin(async move {
            if let Ok(manifest) = self.fetch_mojang_manifest().await {
                let versions: Vec<String> = manifest.versions
                    .into_iter()
                    .filter(|v| v.release_type == "release")
                    .map(|v| v.id)
                    .collect();
                if !versions.is_empty() {
                    return Ok(versions);
                }
            }
            Ok(self.bundled_versions())
        })
    }

    fn get_assets(&self, version: &str) -> Result<Vec<AssetDownload>> {
        if let Some(url) = self.bundled.get(version) {
            return Ok(vec![AssetDownload {
                filename: "server.jar".to_string(),
                url: url.clone(),
                sha256: None,
                is_archive: false,
            }]);
        }

        Err(CraftError::UnknownVersion {
            software: self.name().to_string(),
            version: version.to_string(),
        })
    }

    fn post_download<'a>(
        &'a self,
        _server_path: &'a Path,
        _version: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move { Ok(()) })
    }
}
