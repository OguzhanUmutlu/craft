use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use std::process::Command;
use craft_core::{CraftError, Result};
use crate::traits::{AssetDownload, ServerEdition, ServerSoftware};

pub struct NeoForgeProvider;

impl Default for NeoForgeProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl NeoForgeProvider {
    pub fn new() -> Self {
        Self
    }

    async fn fetch_live_versions() -> Result<Vec<String>> {
        let client = reqwest::Client::builder()
            .user_agent("Craft-CLI/1.0 (https://github.com/OguzhanUmutlu/craft)")
            .build()
            .map_err(|e| CraftError::Download(format!("Failed to build HTTP client: {}", e)))?;

        let url = "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml";
        let resp = client.get(url).send().await
            .map_err(|e| CraftError::Download(format!("Failed to fetch NeoForge metadata: {}", e)))?;
        let text = resp.text().await
            .map_err(|e| CraftError::Download(format!("Failed to read NeoForge metadata: {}", e)))?;

        let mut versions = Vec::new();
        for line in text.lines() {
            let trimmed = line.trim();
            if trimmed.starts_with("<version>") && trimmed.ends_with("</version>") {
                let v = &trimmed[9..trimmed.len() - 10];
                versions.push(v.to_string());
            }
        }

        versions.reverse();
        Ok(versions)
    }
}

impl ServerSoftware for NeoForgeProvider {
    fn id(&self) -> &'static str {
        "neoforge"
    }

    fn name(&self) -> &'static str {
        "NeoForge"
    }

    fn edition(&self) -> ServerEdition {
        ServerEdition::Java
    }

    fn description(&self) -> &'static str {
        "Modern Forge-compatible modded server"
    }

    fn default_server_file(&self) -> &'static str {
        "run.sh"
    }

    fn bundled_versions(&self) -> Vec<String> {
        vec![
            "26.2.0.87".into(),
            "26.1.2.109".into(),
            "21.1.137".into(),
            "21.0.167".into(),
            "20.4.237".into(),
        ]
    }

    fn fetch_versions<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + 'a>> {
        Box::pin(async move {
            match Self::fetch_live_versions().await {
                Ok(v) if !v.is_empty() => Ok(v),
                _ => Ok(self.bundled_versions()),
            }
        })
    }

    fn get_assets(&self, version: &str) -> Result<Vec<AssetDownload>> {
        let url = format!(
            "https://maven.neoforged.net/releases/net/neoforged/neoforge/{}/neoforge-{}-installer.jar",
            version, version
        );

        Ok(vec![AssetDownload {
            filename: "neoforge-installer.jar".to_string(),
            url,
            sha256: None,
            is_archive: false,
        }])
    }

    fn post_download<'a>(
        &'a self,
        server_path: &'a Path,
        _version: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        let path = server_path.to_path_buf();
        Box::pin(async move {
            let installer = path.join("neoforge-installer.jar");
            if installer.exists() {
                println!("Running NeoForge installer to setup server files...");
                let status = Command::new("java")
                    .current_dir(&path)
                    .args(["-jar", "neoforge-installer.jar", "--installServer"])
                    .status()
                    .map_err(|e| CraftError::Process(format!("Failed to run NeoForge installer: {}", e)))?;

                if !status.success() {
                    return Err(CraftError::Process("NeoForge installer exited with non-zero code".to_string()));
                }

                // Remove the installer to keep directory clean
                let _ = std::fs::remove_file(&installer);

                #[cfg(not(target_os = "windows"))]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let run_sh = path.join("run.sh");
                    if run_sh.exists() {
                        let _ = std::fs::set_permissions(&run_sh, std::fs::Permissions::from_mode(0o755));
                    }
                }
            }
            Ok(())
        })
    }

    fn generate_start_script_with_flags(
        &self,
        server_path: &Path,
        _version: &str,
        _java_path: Option<&Path>,
        _memory: &str,
        _jvm_flags: Option<&[String]>,
    ) -> Result<()> {
        #[cfg(target_os = "windows")]
        {
            let cmd_content = "@echo off\r\ncall run.bat %*\r\n";
            std::fs::write(server_path.join("start.cmd"), cmd_content)?;
        }

        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;
            let sh_content = "#!/bin/sh\nexec ./run.sh \"$@\"\n";
            let sh_path = server_path.join("start.sh");
            std::fs::write(&sh_path, sh_content)?;
            let _ = std::fs::set_permissions(&sh_path, std::fs::Permissions::from_mode(0o755));
        }

        Ok(())
    }
}
