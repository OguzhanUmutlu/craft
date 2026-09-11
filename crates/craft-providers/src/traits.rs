use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use craft_core::Result;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ServerEdition {
    Java,
    Bedrock,
    Proxy,
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
    fn edition(&self) -> ServerEdition;
    fn default_server_file(&self) -> &'static str {
        "server.jar"
    }

    /// Default or fallback versions bundled with the tool
    fn bundled_versions(&self) -> Vec<String>;

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
        _version: &str,
        java_path: Option<&Path>,
        memory: &str,
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

        #[cfg(target_os = "windows")]
        {
            let cmd_content = format!(
                "@echo off\r\n{} -Xms{} -Xmx{} -XX:+UseG1GC -jar {} nogui\r\npause\r\n",
                java_cmd, memory, memory, server_file
            );
            std::fs::write(server_path.join("start.cmd"), cmd_content)?;
        }

        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::fs::PermissionsExt;
            let sh_content = format!(
                "#!/bin/sh\nexec {} -Xms{} -Xmx{} -XX:+UseG1GC -jar {} nogui\n",
                java_cmd, memory, memory, server_file
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
