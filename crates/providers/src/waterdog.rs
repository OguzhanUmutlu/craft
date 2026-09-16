use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use craft_core::Result;
use crate::traits::{AssetDownload, ServerEdition, ServerSoftware};

pub struct WaterdogProvider;

impl Default for WaterdogProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl WaterdogProvider {
    pub fn new() -> Self {
        Self
    }
}

impl ServerSoftware for WaterdogProvider {
    fn id(&self) -> &'static str {
        "waterdog"
    }

    fn name(&self) -> &'static str {
        "WaterdogPE"
    }

    fn edition(&self) -> ServerEdition {
        ServerEdition::Proxy
    }

    fn supports_plugins(&self) -> bool {
        true
    }

    fn supports_mods(&self) -> bool {
        false
    }

    fn supports_datapacks(&self) -> bool {
        false
    }

    fn description(&self) -> &'static str {
        "Native Bedrock network proxy"
    }

    fn default_server_file(&self) -> &'static str {
        "Waterdog.jar"
    }

    fn bundled_versions(&self) -> Vec<String> {
        vec!["latest".into(), "2.0".into()]
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
            filename: "Waterdog.jar".to_string(),
            url: "https://github.com/WaterdogPE/WaterdogPE/releases/download/latest/Waterdog.jar".to_string(),
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
