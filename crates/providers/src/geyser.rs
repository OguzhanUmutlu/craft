use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use craft_core::Result;
use crate::traits::{AssetDownload, ServerEdition, ServerSoftware};

pub struct GeyserProvider;

impl Default for GeyserProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl GeyserProvider {
    pub fn new() -> Self {
        Self
    }
}

impl ServerSoftware for GeyserProvider {
    fn id(&self) -> &'static str {
        "geyser"
    }

    fn name(&self) -> &'static str {
        "Geyser"
    }

    fn edition(&self) -> ServerEdition {
        ServerEdition::Proxy
    }

    fn description(&self) -> &'static str {
        "Cross-play bridge for Bedrock clients"
    }

    fn default_server_file(&self) -> &'static str {
        "Geyser.jar"
    }

    fn bundled_versions(&self) -> Vec<String> {
        vec![
            "latest".into(),
            "2.4.2".into(),
            "2.4.0".into(),
            "2.3.0".into(),
        ]
    }

    fn fetch_versions<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + 'a>> {
        Box::pin(async move {
            Ok(self.bundled_versions())
        })
    }

    fn get_assets(&self, version: &str) -> Result<Vec<AssetDownload>> {
        let url = if version == "latest" {
            "https://download.geysermc.org/v2/projects/geyser/versions/latest/builds/latest/downloads/standalone".to_string()
        } else {
            format!(
                "https://download.geysermc.org/v2/projects/geyser/versions/{}/builds/latest/downloads/standalone",
                version
            )
        };

        Ok(vec![AssetDownload {
            filename: "Geyser.jar".to_string(),
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
