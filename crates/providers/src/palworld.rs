use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use craft_core::Result;
use crate::traits::{AssetDownload, ServerEdition, ServerSoftware};

pub struct PalworldProvider;

impl Default for PalworldProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl PalworldProvider {
    pub fn new() -> Self {
        Self
    }
}

impl ServerSoftware for PalworldProvider {
    fn id(&self) -> &'static str {
        "palserver"
    }

    fn name(&self) -> &'static str {
        "Palworld Dedicated Server"
    }

    fn edition(&self) -> ServerEdition {
        ServerEdition::Native
    }

    fn game_id(&self) -> &'static str {
        "palworld"
    }

    fn default_ports(&self) -> (u16, Option<u16>) {
        (8211, Some(27015))
    }

    fn description(&self) -> &'static str {
        "Palworld Dedicated Server (UE5)"
    }

    fn default_server_file(&self) -> &'static str {
        if cfg!(windows) {
            "PalServer.exe"
        } else {
            "PalServer.sh"
        }
    }

    fn bundled_versions(&self) -> Vec<String> {
        vec!["latest".to_string(), "v0.3.0".to_string()]
    }

    fn fetch_versions<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + 'a>> {
        Box::pin(async move {
            Ok(self.bundled_versions())
        })
    }

    fn get_assets(&self, _version: &str) -> Result<Vec<AssetDownload>> {
        Ok(vec![])
    }

    fn post_download<'a>(
        &'a self,
        server_path: &'a Path,
        _version: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            #[cfg(not(target_os = "windows"))]
            {
                use std::os::unix::fs::PermissionsExt;
                let bin = server_path.join("PalServer.sh");
                if bin.exists() {
                    let mut perms = std::fs::metadata(&bin)?.permissions();
                    perms.set_mode(0o755);
                    let _ = std::fs::set_permissions(&bin, perms);
                }
                let ue_bin = server_path.join("Pal/Binaries/Linux/PalServer-Linux-Test");
                if ue_bin.exists() {
                    let mut perms = std::fs::metadata(&ue_bin)?.permissions();
                    perms.set_mode(0o755);
                    let _ = std::fs::set_permissions(&ue_bin, perms);
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
            let cmd_content = "@echo off\r\nPalServer.exe -port=8211 -players=32 -useperfthreads -NoAsyncLoadingThread -UseMultithreadForDS\r\n";
            std::fs::write(server_path.join("start.cmd"), cmd_content)?;
        }

        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;
            let sh_content = "#!/bin/sh\nexec ./PalServer.sh -port=8211 -players=32 -useperfthreads -NoAsyncLoadingThread -UseMultithreadForDS\n";
            let sh_path = server_path.join("start.sh");
            std::fs::write(&sh_path, sh_content)?;
            let mut perms = std::fs::metadata(&sh_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&sh_path, perms)?;
        }

        Ok(())
    }
}
