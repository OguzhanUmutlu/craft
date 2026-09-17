use crate::traits::{AssetDownload, ServerEdition, ServerSoftware};
use craft_core::Result;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

pub struct NukkitProvider;

impl Default for NukkitProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl NukkitProvider {
    pub fn new() -> Self {
        Self
    }

    fn resolve_snapshot_url() -> Option<String> {
        let handle = tokio::runtime::Handle::try_current().ok()?;
        tokio::task::block_in_place(|| {
            handle.block_on(async move {
                let client = reqwest::Client::builder()
                    .user_agent("Craft-CLI/1.0 (https://github.com/OguzhanUmutlu/craft)")
                    .build().ok()?;
                let url = "https://repo.opencollab.dev/maven-snapshots/cn/nukkit/nukkit/1.0-SNAPSHOT/maven-metadata.xml";
                let resp = client.get(url).send().await.ok()?;
                let text = resp.text().await.ok()?;

                // Find <snapshotVersion> with <extension>jar</extension> and <value>
                let mut in_jar_snapshot = false;
                for line in text.lines() {
                    let trimmed = line.trim();
                    if trimmed == "<snapshotVersion>" {
                        in_jar_snapshot = false;
                    } else if trimmed == "<extension>jar</extension>" {
                        in_jar_snapshot = true;
                    } else if in_jar_snapshot && trimmed.starts_with("<value>") && trimmed.ends_with("</value>") {
                        let val = &trimmed[7..trimmed.len() - 8];
                        return Some(format!(
                            "https://repo.opencollab.dev/maven-snapshots/cn/nukkit/nukkit/1.0-SNAPSHOT/nukkit-{}.jar",
                            val
                        ));
                    }
                }
                None
            })
        })
    }
}

impl ServerSoftware for NukkitProvider {
    fn id(&self) -> &'static str {
        "nukkit"
    }

    fn name(&self) -> &'static str {
        "NukkitX"
    }

    fn edition(&self) -> ServerEdition {
        ServerEdition::Bedrock
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
        "Java-based multi-threaded Bedrock server"
    }

    fn default_server_file(&self) -> &'static str {
        "nukkit.jar"
    }

    fn bundled_versions(&self) -> Vec<String> {
        vec!["latest".into(), "1.0".into()]
    }

    fn fetch_versions<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + 'a>> {
        Box::pin(async move { Ok(self.bundled_versions()) })
    }

    fn get_assets(&self, _version: &str) -> Result<Vec<AssetDownload>> {
        let url = Self::resolve_snapshot_url().unwrap_or_else(|| {
            "https://repo.opencollab.dev/maven-snapshots/cn/nukkit/nukkit/1.0-SNAPSHOT/nukkit-1.0-20260907.104332-1247.jar".to_string()
        });

        Ok(vec![AssetDownload {
            filename: "nukkit.jar".to_string(),
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
