use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use craft_core::Result;
use crate::traits::{AssetDownload, ServerEdition, ServerSoftware};

pub struct FactorioProvider;

impl Default for FactorioProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl FactorioProvider {
    pub fn new() -> Self {
        Self
    }
}

impl ServerSoftware for FactorioProvider {
    fn id(&self) -> &'static str {
        "factorio"
    }

    fn name(&self) -> &'static str {
        "Factorio Headless Server"
    }

    fn edition(&self) -> ServerEdition {
        ServerEdition::Native
    }

    fn game_id(&self) -> &'static str {
        "factorio"
    }

    fn default_ports(&self) -> (u16, Option<u16>) {
        (34197, None)
    }

    fn description(&self) -> &'static str {
        "Factorio Headless Dedicated Server"
    }

    fn default_server_file(&self) -> &'static str {
        if cfg!(windows) {
            "bin/x64/factorio.exe"
        } else {
            "bin/x64/factorio"
        }
    }

    fn bundled_versions(&self) -> Vec<String> {
        vec!["2.0.28".to_string(), "1.1.109".to_string()]
    }

    fn fetch_versions<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + 'a>> {
        Box::pin(async move {
            Ok(self.bundled_versions())
        })
    }

    fn get_assets(&self, version: &str) -> Result<Vec<AssetDownload>> {
        let os_target = if cfg!(windows) { "win64" } else { "linux64" };
        let url = format!(
            "https://www.factorio.com/get-download/{}/headless/{}",
            version, os_target
        );
        Ok(vec![AssetDownload {
            filename: "factorio-headless.tar.xz".to_string(),
            url,
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
            let settings_path = server_path.join("server-settings.json");
            if !settings_path.exists() {
                let default_settings = serde_json::json!({
                    "name": "Craft Factorio Server",
                    "description": "Hosted with Craft Multi-Game Manager",
                    "tags": ["craft", "multiplayer"],
                    "max_players": 16,
                    "visibility": {
                        "public": true,
                        "lan": true
                    },
                    "username": "",
                    "password": "",
                    "token": "",
                    "game_password": "",
                    "require_user_verification": false,
                    "auto_pause": true
                });
                let _ = std::fs::write(&settings_path, serde_json::to_string_pretty(&default_settings).unwrap_or_default());
            }

            #[cfg(not(target_os = "windows"))]
            {
                use std::os::unix::fs::PermissionsExt;
                let bin = server_path.join("bin/x64/factorio");
                if bin.exists() {
                    let mut perms = std::fs::metadata(&bin)?.permissions();
                    perms.set_mode(0o755);
                    let _ = std::fs::set_permissions(&bin, perms);
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
            let cmd_content = "@echo off\r\nbin\\x64\\factorio.exe --start-server-load-latest --server-settings server-settings.json --port 34197\r\n";
            std::fs::write(server_path.join("start.cmd"), cmd_content)?;
        }

        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;
            let sh_content = "#!/bin/sh\nexec ./bin/x64/factorio --start-server-load-latest --server-settings server-settings.json --port 34197\n";
            let sh_path = server_path.join("start.sh");
            std::fs::write(&sh_path, sh_content)?;
            let mut perms = std::fs::metadata(&sh_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&sh_path, perms)?;
        }

        Ok(())
    }
}
