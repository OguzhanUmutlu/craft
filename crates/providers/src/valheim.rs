use crate::traits::{AssetDownload, ServerEdition, ServerSoftware};
use craft_core::Result;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

pub struct ValheimProvider;

impl Default for ValheimProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl ValheimProvider {
    pub fn new() -> Self {
        Self
    }
}

impl ServerSoftware for ValheimProvider {
    fn id(&self) -> &'static str {
        "valheim"
    }

    fn name(&self) -> &'static str {
        "Valheim"
    }

    fn edition(&self) -> ServerEdition {
        ServerEdition::Native
    }

    fn game_id(&self) -> &'static str {
        "valheim"
    }

    fn supports_plugins(&self) -> bool {
        false
    }

    fn supports_mods(&self) -> bool {
        false
    }

    fn supports_datapacks(&self) -> bool {
        false
    }

    fn default_ports(&self) -> (u16, Option<u16>) {
        (2456, Some(2457))
    }

    fn description(&self) -> &'static str {
        "Valheim (Unity)"
    }

    fn default_server_file(&self) -> &'static str {
        if cfg!(windows) {
            "valheim_server.exe"
        } else {
            "valheim_server.x86_64"
        }
    }

    fn bundled_versions(&self) -> Vec<String> {
        vec!["latest".to_string()]
    }

    fn fetch_versions<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + 'a>> {
        Box::pin(async move { Ok(self.bundled_versions()) })
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
                let bin = server_path.join("valheim_server.x86_64");
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
            let cmd_content = "@echo off\r\nset SteamAppId=892970\r\nvalheim_server.exe -nographics -batchmode -name \"Craft Server\" -port 2456 -world \"Dedicated\" -password \"secret\"\r\n";
            std::fs::write(server_path.join("start.cmd"), cmd_content)?;
        }

        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;
            let sh_content = "#!/bin/sh\nexport templdpath=$LD_LIBRARY_PATH\nexport LD_LIBRARY_PATH=./linux64:$LD_LIBRARY_PATH\nexport SteamAppId=892970\nexec ./valheim_server.x86_64 -nographics -batchmode -name \"Craft Server\" -port 2456 -world \"Dedicated\" -password \"secret\"\n";
            let sh_path = server_path.join("start.sh");
            std::fs::write(&sh_path, sh_content)?;
            let mut perms = std::fs::metadata(&sh_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&sh_path, perms)?;
        }

        Ok(())
    }
}
