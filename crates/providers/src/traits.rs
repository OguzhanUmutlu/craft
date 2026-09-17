use craft_core::Result;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerEdition {
    Java,
    Bedrock,
    Proxy,
    Native,
}

#[derive(Debug, Clone)]
pub struct AssetDownload {
    pub filename: String,
    pub url: String,
    pub sha256: Option<String>,
    pub is_archive: bool,
}

pub trait ServerSoftware: Send + Sync {
    fn id(&self) -> &'static str;
    fn name(&self) -> &'static str;
    fn display_name(&self) -> &'static str {
        self.name()
    }
    fn edition(&self) -> ServerEdition;
    fn game_id(&self) -> &'static str {
        "minecraft"
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
    fn content_capabilities(&self) -> craft_core::ContentCapabilities {
        craft_core::ContentCapabilities {
            plugins: self.supports_plugins(),
            mods: self.supports_mods(),
            datapacks: self.supports_datapacks(),
        }
    }
    fn runtime_kind(&self) -> craft_core::RuntimeKind {
        match self.edition() {
            ServerEdition::Java | ServerEdition::Proxy => craft_core::RuntimeKind::Java {
                default_jar: self.default_server_file().to_string(),
            },
            ServerEdition::Bedrock | ServerEdition::Native => {
                craft_core::RuntimeKind::NativeBinary {
                    default_executable: self.default_server_file().to_string(),
                }
            }
        }
    }
    fn default_ports(&self) -> (u16, Option<u16>) {
        match self.edition() {
            ServerEdition::Bedrock => (19132, None),
            ServerEdition::Proxy => (25577, None),
            _ => (25565, None),
        }
    }
    fn pre_start_check(&self, server_path: &Path) -> Result<()> {
        if self.game_id() == "minecraft" && self.edition() == ServerEdition::Java {
            let eula_path = server_path.join("eula.txt");
            if eula_path.exists() {
                if let Ok(content) = std::fs::read_to_string(&eula_path) {
                    if content.contains("eula=false") {
                        return Err(craft_core::CraftError::Other(
                            "EULA has not been accepted for this server yet. Run 'craft run --here' to view and agree to the EULA.".to_string()
                        ));
                    }
                }
            }
        } else if self.game_id() == "minecraft" && self.edition() == ServerEdition::Bedrock {
            #[cfg(not(target_os = "windows"))]
            {
                use std::os::unix::fs::PermissionsExt;
                let bin = server_path.join("bedrock_server");
                if bin.exists() {
                    if let Ok(meta) = std::fs::metadata(&bin) {
                        let mut perms = meta.permissions();
                        if perms.mode() & 0o111 == 0 {
                            perms.set_mode(0o755);
                            let _ = std::fs::set_permissions(&bin, perms);
                        }
                    }
                }
            }
        }
        Ok(())
    }
    fn description(&self) -> &'static str {
        "Dedicated server software"
    }
    fn default_server_file(&self) -> &'static str {
        "server.jar"
    }

    /// Default or fallback versions bundled with the tool
    fn bundled_versions(&self) -> Vec<String>;

    /// Returns the recommended / default stable release version for this software
    fn recommended_version(&self) -> String {
        let mut v = self.bundled_versions();
        craft_core::sort_versions_descending(&mut v);
        v.into_iter()
            .find(|ver| craft_core::is_stable_version(ver))
            .unwrap_or_else(|| "latest".to_string())
    }

    /// Fetch latest live versions from the upstream API
    fn fetch_versions<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + 'a>>;

    /// Resolve download assets for a specific version
    fn get_assets(&self, version: &str) -> Result<Vec<AssetDownload>>;

    /// Lifecycle hook executed immediately after assets are placed in the server directory
    fn post_download<'a>(
        &'a self,
        server_path: &'a Path,
        version: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>>;

    /// Generates appropriate start scripts (start.sh and/or start.cmd)
    fn generate_start_script(
        &self,
        server_path: &Path,
        version: &str,
        java_path: Option<&Path>,
        memory: &str,
    ) -> Result<()> {
        self.generate_start_script_with_flags(server_path, version, java_path, memory, None)
    }

    /// Generates start scripts with custom JVM flags and GC options
    fn generate_start_script_with_flags(
        &self,
        server_path: &Path,
        _version: &str,
        java_path: Option<&Path>,
        memory: &str,
        jvm_flags: Option<&[String]>,
    ) -> Result<()> {
        let server_file = self.default_server_file();
        let java_cmd = if let Some(j) = java_path {
            let path_str = j.to_string_lossy();
            if cfg!(windows) && path_str.contains(' ') {
                format!("\"{}\"", path_str)
            } else {
                path_str.replace(' ', "\\ ")
            }
        } else {
            "java".to_string()
        };

        let flags = match jvm_flags {
            Some(f) if !f.is_empty() => f.join(" "),
            _ => "-XX:+UseG1GC".to_string(),
        };

        #[cfg(target_os = "windows")]
        {
            let cmd_content = format!(
                "@echo off\r\n{} -Xms{} -Xmx{} {} -jar {} nogui\r\n",
                java_cmd, memory, memory, flags, server_file
            );
            std::fs::write(server_path.join("start.cmd"), cmd_content)?;
        }

        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;
            let sh_content = format!(
                "#!/bin/sh\nexec {} -Xms{} -Xmx{} {} -jar {} nogui\n",
                java_cmd, memory, memory, flags, server_file
            );
            let sh_path = server_path.join("start.sh");
            std::fs::write(&sh_path, sh_content)?;
            let mut perms = std::fs::metadata(&sh_path)?.permissions();
            perms.set_mode(0o755);
            std::fs::set_permissions(&sh_path, perms)?;
        }

        Ok(())
    }
}
