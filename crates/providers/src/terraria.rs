use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use craft_core::Result;
use crate::traits::{AssetDownload, ServerEdition, ServerSoftware};

pub struct TerrariaProvider;

impl Default for TerrariaProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl TerrariaProvider {
    pub fn new() -> Self {
        Self
    }
}

impl ServerSoftware for TerrariaProvider {
    fn id(&self) -> &'static str {
        "tshock"
    }

    fn name(&self) -> &'static str {
        "TShock (Terraria)"
    }

    fn edition(&self) -> ServerEdition {
        ServerEdition::Native
    }

    fn game_id(&self) -> &'static str {
        "terraria"
    }

    fn default_ports(&self) -> (u16, Option<u16>) {
        (7777, None)
    }

    fn description(&self) -> &'static str {
        "TShock dedicated server for Terraria"
    }

    fn default_server_file(&self) -> &'static str {
        if cfg!(windows) {
            "TerrariaServer.exe"
        } else {
            "TerrariaServer.bin.x86_64"
        }
    }

    fn bundled_versions(&self) -> Vec<String> {
        vec!["v5.2.0".to_string(), "v5.1.3".to_string()]
    }

    fn fetch_versions<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + 'a>> {
        Box::pin(async move {
            Ok(self.bundled_versions())
        })
    }

    fn get_assets(&self, version: &str) -> Result<Vec<AssetDownload>> {
        let tag = if version.starts_with('v') {
            version.to_string()
        } else {
            format!("v{}", version)
        };
        let filename = format!("TShock-{}-Server.zip", tag);
        let url = format!(
            "https://github.com/Pryaxis/TShock/releases/download/{}/{}",
            tag, filename
        );
        Ok(vec![AssetDownload {
            filename,
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
            let config_path = server_path.join("serverconfig.txt");
            if !config_path.exists() {
                let default_cfg = "\
# Terraria Server Configuration
port=7777
maxplayers=16
worldpath=./Worlds
worldname=World
autocreate=2
difficulty=1
motd=Welcome to our Terraria Server!
";
                let _ = std::fs::write(&config_path, default_cfg);
            }

            #[cfg(not(target_os = "windows"))]
            {
                use std::os::unix::fs::PermissionsExt;
                let bin = server_path.join("TerrariaServer.bin.x86_64");
                if bin.exists() {
                    let mut perms = std::fs::metadata(&bin)?.permissions();
                    perms.set_mode(0o755);
                    let _ = std::fs::set_permissions(&bin, perms);
                }
                let tshock_bin = server_path.join("TShock.Server");
                if tshock_bin.exists() {
                    let mut perms = std::fs::metadata(&tshock_bin)?.permissions();
                    perms.set_mode(0o755);
                    let _ = std::fs::set_permissions(&tshock_bin, perms);
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
            let cmd_content = "@echo off\r\nTerrariaServer.exe -config serverconfig.txt\r\n";
            std::fs::write(server_path.join("start.cmd"), cmd_content)?;
        }

        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;
            let sh_content = "#!/bin/sh\nexec ./TerrariaServer.bin.x86_64 -config serverconfig.txt\n";
            let sh_path = server_path.join("start.sh");
            std::fs::write(&sh_path, sh_content)?;
            let mut perms = std::fs::metadata(&sh_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&sh_path, perms)?;
        }

        Ok(())
    }
}
