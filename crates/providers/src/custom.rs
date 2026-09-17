use crate::traits::{AssetDownload, ServerEdition, ServerSoftware};
use craft_core::Result;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

pub struct CustomGameProvider;

impl Default for CustomGameProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl CustomGameProvider {
    pub fn new() -> Self {
        Self
    }
}

impl ServerSoftware for CustomGameProvider {
    fn id(&self) -> &'static str {
        "custom"
    }

    fn name(&self) -> &'static str {
        "Custom Game"
    }

    fn display_name(&self) -> &'static str {
        "Custom Game (Generic binary/script)"
    }

    fn edition(&self) -> ServerEdition {
        ServerEdition::Native
    }

    fn game_id(&self) -> &'static str {
        ""
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
        (8080, None)
    }

    fn description(&self) -> &'static str {
        "Custom binary or script"
    }

    fn default_server_file(&self) -> &'static str {
        if cfg!(windows) {
            "server.exe"
        } else {
            "server"
        }
    }

    fn bundled_versions(&self) -> Vec<String> {
        vec!["custom".to_string()]
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
            let config = match craft_scripting::CustomServerConfig::load_from_dir(server_path)? {
                Some(cfg) => cfg,
                None => {
                    let name = server_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("custom-server");
                    craft_scripting::CustomServerConfig::default_lua(name, 8080)
                }
            };
            craft_scripting::generate_all_starter_files(server_path, &config)?;

            #[cfg(not(target_os = "windows"))]
            {
                use std::os::unix::fs::PermissionsExt;
                let bin = server_path.join("server");
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
        let config = match craft_scripting::CustomServerConfig::load_from_dir(server_path)? {
            Some(cfg) => cfg,
            None => {
                let name = server_path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("custom-server");
                craft_scripting::CustomServerConfig::default_lua(name, 8080)
            }
        };

        craft_scripting::generate_all_starter_files(server_path, &config)?;
        Ok(())
    }

    fn generate_start_script_with_flags(
        &self,
        server_path: &Path,
        version: &str,
        java_path: Option<&Path>,
        memory: &str,
        _jvm_flags: Option<&[String]>,
    ) -> Result<()> {
        self.generate_start_script(server_path, version, java_path, memory)
    }
}
