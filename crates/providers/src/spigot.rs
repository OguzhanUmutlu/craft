use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use craft_core::{CraftError, Result};
use crate::bundled::{parse_bundled_manifest, BUNDLED_SPIGOT};
use crate::traits::{AssetDownload, ServerEdition, ServerSoftware};

pub struct SpigotProvider {
    bundled: HashMap<String, String>,
}

impl Default for SpigotProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl SpigotProvider {
    pub fn new() -> Self {
        let (_, bundled) = parse_bundled_manifest(BUNDLED_SPIGOT);
        Self { bundled }
    }
}

impl ServerSoftware for SpigotProvider {
    fn id(&self) -> &'static str {
        "spigot"
    }

    fn name(&self) -> &'static str {
        "Spigot"
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
        "Classic Bukkit / Spigot plugin server"
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
