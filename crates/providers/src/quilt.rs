use crate::traits::{AssetDownload, ServerEdition, ServerSoftware};
use craft_core::{CraftError, Result};
use serde::Deserialize;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::process::Command;

#[derive(Deserialize)]
struct QuiltGameVersion {
    version: String,
    stable: bool,
}

pub struct QuiltProvider;

impl Default for QuiltProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl QuiltProvider {
    pub fn new() -> Self {
        Self
    }
}

impl ServerSoftware for QuiltProvider {
    fn id(&self) -> &'static str {
        "quilt"
    }

    fn name(&self) -> &'static str {
        "Quilt"
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
        "Community-driven modular modded server"
    }

    fn default_server_file(&self) -> &'static str {
        "quilt-server-launch.jar"
    }

    fn bundled_versions(&self) -> Vec<String> {
        vec![
            "1.21.4".into(),
            "1.21.1".into(),
            "1.21".into(),
            "1.20.6".into(),
            "1.20.4".into(),
            "1.20.2".into(),
            "1.20.1".into(),
            "1.19.4".into(),
            "1.18.2".into(),
        ]
    }

    fn fetch_versions<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + 'a>> {
        Box::pin(async move {
            let url = "https://meta.quiltmc.org/v3/versions/game";
            if let Ok(cache) = crate::cache::CacheManager::from_default_paths() {
                if let Ok(list) = cache
                    .get_cached_json::<Vec<QuiltGameVersion>>(
                        "quilt_game_versions",
                        url,
                        std::time::Duration::from_secs(6 * 3600),
                    )
                    .await
                {
                    let mut versions: Vec<String> = list
                        .into_iter()
                        .filter(|g| g.stable)
                        .map(|g| g.version)
                        .collect();
                    if !versions.is_empty() {
                        craft_core::sort_versions_descending(&mut versions);
                        return Ok(versions);
                    }
                }
            }

            let client = reqwest::Client::builder()
                .user_agent("Craft-CLI/1.0 (https://github.com/OguzhanUmutlu/craft)")
                .build()
                .map_err(|e| CraftError::Download(format!("Failed to build HTTP client: {}", e)))?;

            if let Ok(resp) = client.get(url).send().await {
                if let Ok(list) = resp.json::<Vec<QuiltGameVersion>>().await {
                    let mut versions: Vec<String> = list
                        .into_iter()
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

    fn get_assets(&self, _version: &str) -> Result<Vec<AssetDownload>> {
        Ok(vec![AssetDownload {
            filename: "quilt-installer.jar".to_string(),
            url: "https://maven.quiltmc.org/repository/release/org/quiltmc/quilt-installer/0.15.1/quilt-installer-0.15.1.jar".to_string(),
            sha256: None,
            is_archive: false,
        }])
    }

    fn post_download<'a>(
        &'a self,
        server_path: &'a Path,
        version: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        let path = server_path.to_path_buf();
        let ver = version.to_string();

        Box::pin(async move {
            let installer = path.join("quilt-installer.jar");
            if installer.exists() {
                println!("Running Quilt installer for version {}...", ver);
                let status = Command::new("java")
                    .current_dir(&path)
                    .args([
                        "-jar",
                        "quilt-installer.jar",
                        "install",
                        "server",
                        &ver,
                        "--download-server",
                    ])
                    .status()
                    .map_err(|e| {
                        CraftError::Process(format!("Failed to run Quilt installer: {}", e))
                    })?;

                if !status.success() {
                    return Err(CraftError::Process(
                        "Quilt installer exited with non-zero exit code".to_string(),
                    ));
                }

                let _ = std::fs::remove_file(&installer);
            }
            Ok(())
        })
    }
}
