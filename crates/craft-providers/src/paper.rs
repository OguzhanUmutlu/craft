use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use serde::Deserialize;
use craft_core::{CraftError, Result};
use crate::bundled::{parse_bundled_manifest, BUNDLED_FOLIA, BUNDLED_PAPER};
use crate::traits::{AssetDownload, ServerEdition, ServerSoftware};

#[derive(Deserialize)]
struct PaperProjectResponse {
    versions: Vec<String>,
}

#[allow(dead_code)]
#[derive(Deserialize)]
struct PaperVersionResponse {
    builds: Vec<u32>,
}

pub struct PaperProvider {
    project: &'static str,
    name: &'static str,
    bundled: HashMap<String, String>,
}

impl PaperProvider {
    pub fn new_paper() -> Self {
        let (_, bundled) = parse_bundled_manifest(BUNDLED_PAPER);
        Self {
            project: "paper",
            name: "Paper",
            bundled,
        }
    }

    pub fn new_folia() -> Self {
        let (_, bundled) = parse_bundled_manifest(BUNDLED_FOLIA);
        Self {
            project: "folia",
            name: "Folia",
            bundled,
        }
    }

    async fn fetch_live_versions(&self) -> Result<Vec<String>> {
        let client = reqwest::Client::new();
        let url = format!("https://api.papermc.io/v2/projects/{}", self.project);
        let resp = client.get(&url).send().await
            .map_err(|e| CraftError::Download(format!("Failed to fetch PaperMC versions: {}", e)))?;
        let data: PaperProjectResponse = resp.json().await
            .map_err(|e| CraftError::Download(format!("Invalid PaperMC response: {}", e)))?;

        let mut versions = data.versions;
        versions.reverse();
        Ok(versions)
    }

    #[allow(dead_code)]
    async fn resolve_live_build(&self, version: &str) -> Result<String> {
        let client = reqwest::Client::new();
        let url = format!("https://api.papermc.io/v2/projects/{}/versions/{}", self.project, version);
        let resp = client.get(&url).send().await
            .map_err(|e| CraftError::Download(format!("Failed to fetch PaperMC builds for {}: {}", version, e)))?;
        let data: PaperVersionResponse = resp.json().await
            .map_err(|e| CraftError::Download(format!("Invalid builds response: {}", e)))?;

        let latest_build = data.builds.last().copied()
            .ok_or_else(|| CraftError::UnknownVersion {
                software: self.name.to_string(),
                version: version.to_string(),
            })?;

        Ok(format!(
            "https://api.papermc.io/v2/projects/{}/versions/{}/builds/{}/downloads/{}-{}-{}.jar",
            self.project, version, latest_build, self.project, version, latest_build
        ))
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
        ServerEdition::Java
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
            match self.fetch_live_versions().await {
                Ok(v) if !v.is_empty() => Ok(v),
                _ => Ok(self.bundled_versions()),
            }
        })
    }

    fn get_assets(&self, version: &str) -> Result<Vec<AssetDownload>> {
        // If present in bundled map, use it immediately; otherwise construct via live API or fallback
        if let Some(url) = self.bundled.get(version) {
            return Ok(vec![AssetDownload {
                filename: "server.jar".to_string(),
                url: url.clone(),
                sha256: None,
                is_archive: false,
            }]);
        }

        // Fallback or dynamic
        Ok(vec![AssetDownload {
            filename: "server.jar".to_string(),
            url: format!(
                "https://api.papermc.io/v2/projects/{}/versions/{}/builds/latest/downloads/{}-{}-latest.jar",
                self.project, version, self.project, version
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
