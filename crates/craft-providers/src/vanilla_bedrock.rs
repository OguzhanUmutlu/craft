use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use craft_core::{CraftError, Result};
use crate::bundled::{parse_bundled_manifest, BUNDLED_BEDROCK_LINUX, BUNDLED_BEDROCK_WIN};
use crate::cache::CacheManager;
use crate::traits::{AssetDownload, ServerEdition, ServerSoftware};

pub struct VanillaBedrockProvider {
    win_versions: HashMap<String, String>,
    linux_versions: HashMap<String, String>,
}

impl VanillaBedrockProvider {
    pub fn new() -> Self {
        let (_, win_versions) = parse_bundled_manifest(BUNDLED_BEDROCK_WIN);
        let (_, linux_versions) = parse_bundled_manifest(BUNDLED_BEDROCK_LINUX);
        Self {
            win_versions,
            linux_versions,
        }
    }
}

impl ServerSoftware for VanillaBedrockProvider {
    fn id(&self) -> &'static str {
        "vanilla_bedrock"
    }

    fn name(&self) -> &'static str {
        "Vanilla (Bedrock)"
    }

    fn edition(&self) -> ServerEdition {
        ServerEdition::Bedrock
    }

    fn default_server_file(&self) -> &'static str {
        if cfg!(windows) {
            "bedrock_server.exe"
        } else {
            "bedrock_server"
        }
    }

    fn bundled_versions(&self) -> Vec<String> {
        let mut versions: Vec<String> = if cfg!(windows) {
            self.win_versions.keys().cloned().collect()
        } else {
            self.linux_versions.keys().cloned().collect()
        };
        versions.sort();
        versions.reverse();
        versions
    }

    fn fetch_versions<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + 'a>> {
        Box::pin(async move {
            Ok(self.bundled_versions())
        })
    }

    fn get_assets(&self, version: &str) -> Result<Vec<AssetDownload>> {
        let url = if cfg!(windows) {
            self.win_versions.get(version)
        } else {
            self.linux_versions.get(version)
        }.ok_or_else(|| CraftError::UnknownVersion {
            software: self.name().to_string(),
            version: version.to_string(),
        })?;

        Ok(vec![AssetDownload {
            filename: "server.zip".to_string(),
            url: url.clone(),
            sha256: None,
            is_archive: true,
        }])
    }

    fn post_download<'a>(
        &'a self,
        server_path: &'a Path,
        _version: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let zip_file = server_path.join("server.zip");
            if zip_file.exists() {
                CacheManager::extract_zip(&zip_file, server_path)?;
                let _ = std::fs::remove_file(zip_file);
            }

            #[cfg(not(target_os = "windows"))]
            {
                use std::os::unix::fs::PermissionsExt;
                let bin = server_path.join("bedrock_server");
                if bin.exists() {
                    let mut perms = std::fs::metadata(&bin)?.permissions();
                    perms.set_mode(0o755);
                    std::fs::set_permissions(&bin, perms)?;
                }
            }

            Ok(())
        })
    }

    fn generate_start_script(
        &self,
        server_path: &Path,
        _version: &str,
        _java_path: Option<&Path>,
        _memory: &str,
    ) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            let cmd_content = "@echo off\r\nbedrock_server.exe\r\npause\r\n";
            std::fs::write(server_path.join("start.cmd"), cmd_content)?;
        }

        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;
            let sh_content = "#!/bin/sh\nexport LD_LIBRARY_PATH=.\nexec ./bedrock_server\n";
            let sh_path = server_path.join("start.sh");
            std::fs::write(&sh_path, sh_content)?;
            let mut perms = std::fs::metadata(&sh_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&sh_path, perms)?;
        }

        Ok(())
    }
}
