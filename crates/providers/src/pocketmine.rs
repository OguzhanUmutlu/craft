use std::collections::HashMap;
use std::future::Future;
use std::path::Path;
use std::pin::Pin;
use serde_json::Value;
use craft_core::{CraftError, Result};
use crate::bundled::BUNDLED_POCKETMINE;
use crate::cache::CacheManager;
use crate::traits::{AssetDownload, ServerEdition, ServerSoftware};

#[derive(Clone)]
struct PocketmineEntry {
    pm_version: String,
    phar_url: String,
    sh_url: String,
    #[allow(dead_code)]
    cmd_url: String,
}

pub struct PocketmineProvider {
    versions: HashMap<String, PocketmineEntry>,
}

impl Default for PocketmineProvider {
    fn default() -> Self {
        Self::new()
    }
}

impl PocketmineProvider {
    pub fn new() -> Self {
        let parsed: Value = serde_json::from_str(BUNDLED_POCKETMINE).unwrap_or(Value::Null);
        let mut versions = HashMap::new();

        if let Some(obj) = parsed.as_object() {
            for (k, v) in obj {
                if k == "latest" {
                    continue;
                }
                if let Some(arr) = v.as_array() {
                    if arr.len() >= 4 {
                        versions.insert(
                            k.clone(),
                            PocketmineEntry {
                                pm_version: arr[0].as_str().unwrap_or_default().to_string(),
                                phar_url: arr[1].as_str().unwrap_or_default().to_string(),
                                sh_url: arr[2].as_str().unwrap_or_default().to_string(),
                                cmd_url: arr[3].as_str().unwrap_or_default().to_string(),
                            },
                        );
                    }
                }
            }
        }

        Self { versions }
    }

    fn resolve_php_binary_url(&self, pm_version: &str) -> Result<(&'static str, bool)> {
        // Returns (url, is_tar_gz)
        let major = pm_version.chars().next().unwrap_or('5');
        match major {
            '5' => {
                #[cfg(target_os = "windows")]
                return Ok(("https://github.com/pmmp/PHP-Binaries/releases/download/pm5-latest/PHP-8.2-Windows-x64-PM5.zip", false));
                #[cfg(target_os = "linux")]
                return Ok(("https://github.com/pmmp/PHP-Binaries/releases/download/pm5-latest/PHP-8.2-Linux-x86_64-PM5.tar.gz", true));
                #[cfg(target_os = "macos")]
                {
                    #[cfg(target_arch = "aarch64")]
                    return Ok(("https://github.com/pmmp/PHP-Binaries/releases/download/pm5-latest/PHP-8.2-MacOS-arm64-PM5.tar.gz", true));
                    #[cfg(not(target_arch = "aarch64"))]
                    return Ok(("https://github.com/pmmp/PHP-Binaries/releases/download/pm5-latest/PHP-8.2-MacOS-x86_64-PM5.tar.gz", true));
                }
            }
            '4' => {
                #[cfg(target_os = "windows")]
                return Ok(("https://github.com/pmmp/PHP-Binaries/releases/download/pm4-php-8.0-latest/PHP-8.0-Windows-x64-PM4.zip", false));
                #[cfg(target_os = "linux")]
                return Ok(("https://github.com/pmmp/PHP-Binaries/releases/download/pm4-php-8.0-latest/PHP-8.0-Linux-x86_64-PM4.tar.gz", true));
                #[cfg(target_os = "macos")]
                return Ok(("https://github.com/pmmp/PHP-Binaries/releases/download/pm4-php-8.0-latest/PHP-8.0-MacOS-x86_64-PM4.tar.gz", true));
            }
            _ => {
                #[cfg(target_os = "windows")]
                return Ok(("https://pmmp-php.github.io/files/api3/php-7.4.21-Windows.zip", false));
                #[cfg(target_os = "linux")]
                return Ok(("https://pmmp-php.github.io/files/api3/php-7.4.21-Linux.zip", false));
                #[cfg(target_os = "macos")]
                return Ok(("https://pmmp-php.github.io/files/api3/php-7.4.21-Mac.zip", false));
            }
        }
    }
}

impl ServerSoftware for PocketmineProvider {
    fn id(&self) -> &'static str {
        "pocketmine"
    }

    fn name(&self) -> &'static str {
        "PocketMine-MP"
    }

    fn edition(&self) -> ServerEdition {
        ServerEdition::Bedrock
    }

    fn description(&self) -> &'static str {
        "High-performance C++ / PHP Bedrock server"
    }

    fn default_server_file(&self) -> &'static str {
        "PocketMine-MP.phar"
    }

    fn bundled_versions(&self) -> Vec<String> {
        let mut v: Vec<String> = self.versions.keys().cloned().collect();
        v.sort();
        v.reverse();
        v
    }

    fn fetch_versions<'a>(
        &'a self,
    ) -> Pin<Box<dyn Future<Output = Result<Vec<String>>> + Send + 'a>> {
        Box::pin(async move {
            Ok(self.bundled_versions())
        })
    }

    fn get_assets(&self, version: &str) -> Result<Vec<AssetDownload>> {
        let entry = self.versions.get(version).ok_or_else(|| CraftError::UnknownVersion {
            software: self.name().to_string(),
            version: version.to_string(),
        })?;

        let (php_url, is_tar_gz) = self.resolve_php_binary_url(&entry.pm_version)?;

        let mut assets = vec![
            AssetDownload {
                filename: "PocketMine-MP.phar".to_string(),
                url: entry.phar_url.clone(),
                sha256: None,
                is_archive: false,
            },
            AssetDownload {
                filename: if is_tar_gz { "libs.tar.gz".to_string() } else { "libs.zip".to_string() },
                url: php_url.to_string(),
                sha256: None,
                is_archive: true,
            },
        ];

        #[cfg(target_os = "windows")]
        assets.push(AssetDownload {
            filename: "start.cmd".to_string(),
            url: entry.cmd_url.clone(),
            sha256: None,
            is_archive: false,
        });

        #[cfg(not(target_os = "windows"))]
        assets.push(AssetDownload {
            filename: "start.sh".to_string(),
            url: entry.sh_url.clone(),
            sha256: None,
            is_archive: false,
        });

        Ok(assets)
    }

    fn post_download<'a>(
        &'a self,
        server_path: &'a Path,
        _version: &'a str,
    ) -> Pin<Box<dyn Future<Output = Result<()>> + Send + 'a>> {
        Box::pin(async move {
            let zip_libs = server_path.join("libs.zip");
            let targz_libs = server_path.join("libs.tar.gz");

            if zip_libs.exists() {
                CacheManager::extract_zip(&zip_libs, server_path)?;
                let _ = std::fs::remove_file(zip_libs);
            } else if targz_libs.exists() {
                CacheManager::extract_tar_gz(&targz_libs, server_path)?;
                let _ = std::fs::remove_file(targz_libs);
            }

            #[cfg(not(target_os = "windows"))]
            {
                use std::os::unix::fs::PermissionsExt;
                let sh = server_path.join("start.sh");
                if sh.exists() {
                    let mut perms = std::fs::metadata(&sh)?.permissions();
                    perms.set_mode(0o755);
                    let _ = std::fs::set_permissions(&sh, perms);
                }

                let bin_php = server_path.join("bin").join("php7").join("bin").join("php");
                if bin_php.exists() {
                    let mut perms = std::fs::metadata(&bin_php)?.permissions();
                    perms.set_mode(0o755);
                    let _ = std::fs::set_permissions(&bin_php, perms);
                }
            }

            Ok(())
        })
    }

    fn generate_start_script(
        &self,
        _server_path: &Path,
        _version: &str,
        _java_path: Option<&Path>,
        _memory: &str,
    ) -> Result<()> {
        // PocketMine start script is downloaded directly from the official PMMP repository
        Ok(())
    }
}
