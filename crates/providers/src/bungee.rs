use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use craft_core::Result;
use crate::traits::{AssetDownload, ServerEdition, ServerSoftware};

pub struct BungeeProvider;

impl Default for BungeeProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl BungeeProvider {
    pub fn new() -> Self {
        Self
    }
}

impl ServerSoftware for BungeeProvider {
    fn id(&self) -> &'static str {
        "bungeecord"
    }

    fn name(&self) -> &'static str {
        "BungeeCord"
    }

    fn edition(&self) -> ServerEdition {
        ServerEdition::Proxy
    }

    fn description(&self) -> &'static str {
        "Classic multi-server network proxy"
    }

    fn default_server_file(&self) -> &'static str {
        "BungeeCord.jar"
    }

    fn bundled_versions(&self) -> Vec<String> {
        vec![
            "latest".into(),
            "1.21".into(),
            "1.20".into(),
            "1.19".into(),
            "1.18".into(),
        ]
    }

    fn fetch_versions<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + 'a>> {
        Box::pin(async move {
            Ok(self.bundled_versions())
        })
    }

    fn get_assets(&self, _version: &str) -> Result<Vec<AssetDownload>> {
        Ok(vec![AssetDownload {
            filename: "BungeeCord.jar".to_string(),
            url: "https://ci.md-5.net/job/BungeeCord/lastSuccessfulBuild/artifact/bootstrap/target/BungeeCord.jar".to_string(),
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
