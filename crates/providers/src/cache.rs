use std::fs::{self, File};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use serde::de::DeserializeOwned;
use futures_util::StreamExt;
use craft_core::{CraftError, CraftPaths, Result, CacheStore, CacheStats, GlobalSettings};

pub struct CacheManager {
    client: Client,
    cache_dir: PathBuf,
    store: CacheStore,
}

impl CacheManager {
    pub fn new(paths: &CraftPaths) -> Self {
        let client = Client::builder()
            .user_agent("Craft/1.0 (Minecraft Server Manager)")
            .build()
            .unwrap_or_default();

        let settings = GlobalSettings::load(paths).unwrap_or_default();
        let store = CacheStore::new(paths.cache_dir.clone(), settings.cache_max_bytes)
            .unwrap_or_else(|_| CacheStore::new(paths.cache_dir.clone(), 2 * 1024 * 1024 * 1024).expect("cache store init"));

        Self {
            client,
            cache_dir: paths.cache_dir.clone(),
            store,
        }
    }

    pub fn from_default_paths() -> Result<Self> {
        let paths = CraftPaths::new()?;
        Ok(Self::new(&paths))
    }

    pub fn store(&self) -> &CacheStore {
        &self.store
    }

    pub fn store_mut(&mut self) -> &mut CacheStore {
        &mut self.store
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub fn cache_path(&self, software_id: &str, version: &str, filename: &str) -> PathBuf {
        let safe_filename = format!("{}-{}-{}", software_id, version.replace('/', "_"), filename);
        self.store.artifacts_dir().join(safe_filename)
    }

    /// Downloads the asset if not already cached, and hardlinks/copies it to destination
    pub async fn fetch_and_install(
        &self,
        software_id: &str,
        version: &str,
        filename: &str,
        url: &str,
        destination: &Path,
        expected_sha256: Option<&str>,
    ) -> Result<PathBuf> {
        let rel_subpath = format!("{}-{}-{}", software_id, version.replace('/', "_"), filename);

        let cached_file = match self.store.get_artifact(&rel_subpath) {
            Some(path) => path,
            None => {
                let temp_dir = tempfile::tempdir()?;
                let temp_file = temp_dir.path().join(filename);
                self.download_file(url, &temp_file, filename).await?;

                let (path, _) = self.store.put_artifact_file(&rel_subpath, &temp_file, expected_sha256)?;
                path
            }
        };

        let target_file = destination.join(filename);
        self.store.link_or_copy(&cached_file, &target_file)?;

        Ok(target_file)
    }

    /// Fetches a JSON endpoint with zstd-compressed caching on disk and specified TTL
    pub async fn get_cached_json<T: DeserializeOwned>(
        &self,
        key: &str,
        url: &str,
        ttl: Duration,
    ) -> Result<T> {
        // 1. Check metadata cache
        if let Ok(Some(cached_bytes)) = self.store.get_metadata(key) {
            if let Ok(data) = serde_json::from_slice::<T>(&cached_bytes) {
                return Ok(data);
            }
        }

        // 2. Fetch from network
        let resp = self.client.get(url).send().await
            .map_err(|e| CraftError::Download(format!("HTTP request failed for {}: {}", url, e)))?;

        if !resp.status().is_success() {
            return Err(CraftError::Download(format!(
                "HTTP {} while downloading from {}",
                resp.status(),
                url
            )));
        }

        let bytes = resp.bytes().await
            .map_err(|e| CraftError::Download(format!("Failed to read response body: {}", e)))?;

        // 3. Deserialize JSON
        let parsed: T = serde_json::from_slice(&bytes)
            .map_err(|e| CraftError::Download(format!("Failed to parse JSON from {}: {}", url, e)))?;

        // 4. Cache compressed with Zstandard level 3
        let _ = self.store.put_metadata(key, &bytes, Some(ttl));

        Ok(parsed)
    }

    async fn download_file(&self, url: &str, target_path: &Path, display_name: &str) -> Result<()> {
        let response = self.client.get(url).send().await
            .map_err(|e| CraftError::Download(format!("HTTP request failed for {}: {}", url, e)))?;

        if !response.status().is_success() {
            return Err(CraftError::Download(format!(
                "HTTP {} while downloading from {}",
                response.status(),
                url
            )));
        }

        let total_size = response.content_length().unwrap_or(0);
        let pb = if total_size > 0 {
            let pb = ProgressBar::new(total_size);
            pb.set_style(
                ProgressStyle::default_bar()
                    .template("{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, ETA: {eta})")
                    .unwrap()
                    .progress_chars("#>-"),
            );
            pb.set_message(format!("Downloading {}", display_name));
            Some(pb)
        } else {
            println!("Downloading {} (indeterminate size)...", display_name);
            None
        };

        let temp_path = target_path.with_extension("download.tmp");
        let mut file = File::create(&temp_path)?;
        let mut stream = response.bytes_stream();

        while let Some(chunk_result) = stream.next().await {
            let chunk = chunk_result
                .map_err(|e| CraftError::Download(format!("Error reading stream: {}", e)))?;
            file.write_all(&chunk)?;
            if let Some(ref pb) = pb {
                pb.inc(chunk.len() as u64);
            }
        }

        if let Some(pb) = pb {
            pb.finish_with_message(format!("Downloaded {}", display_name));
        }

        fs::rename(&temp_path, target_path)?;
        Ok(())
    }

    pub fn get_cache_size(&self) -> u64 {
        self.store.get_stats().total_bytes
    }

    pub fn get_stats(&self) -> CacheStats {
        self.store.get_stats()
    }

    pub fn prune(&self) -> Result<u64> {
        self.store.prune_to_watermark()
    }

    pub fn clean_expired(&self) -> Result<u64> {
        self.store.clean_expired()
    }

    pub fn clean_cache(&self) -> Result<u64> {
        self.store.clean_all()
    }

    /// Utility: Extracts a ZIP archive into a destination folder
    pub fn extract_zip(zip_path: &Path, destination: &Path) -> Result<()> {
        let file = File::open(zip_path)?;
        let mut archive = zip::ZipArchive::new(file)
            .map_err(|e| CraftError::Other(format!("Failed to open zip: {}", e)))?;

        for i in 0..archive.len() {
            let mut file = archive.by_index(i)
                .map_err(|e| CraftError::Other(format!("Zip read error: {}", e)))?;
            let outpath = match file.enclosed_name() {
                Some(path) => destination.join(path),
                None => continue,
            };

            if file.name().ends_with('/') {
                fs::create_dir_all(&outpath)?;
            } else {
                if let Some(p) = outpath.parent() {
                    if !p.exists() {
                        fs::create_dir_all(p)?;
                    }
                }
                let mut outfile = File::create(&outpath)?;
                std::io::copy(&mut file, &mut outfile)?;
            }
        }
        Ok(())
    }

    /// Utility: Extracts a tar.gz archive into a destination folder
    pub fn extract_tar_gz(tar_gz_path: &Path, destination: &Path) -> Result<()> {
        let file = File::open(tar_gz_path)?;
        let gz = flate2::read::GzDecoder::new(file);
        let mut archive = tar::Archive::new(gz);
        archive.unpack(destination)
            .map_err(|e| CraftError::Other(format!("Failed to unpack tar.gz: {}", e)))?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_hardlink_and_fallback() {
        let temp_dir = std::env::temp_dir().join(format!("craft_test_cache_{}", std::process::id()));
        let _ = fs::remove_dir_all(&temp_dir);
        let cache_dir = temp_dir.join("cache");
        let server_dir = temp_dir.join("server");
        fs::create_dir_all(&cache_dir).unwrap();
        fs::create_dir_all(&server_dir).unwrap();

        let cached_file = cache_dir.join("paper-1.21.4.jar");
        fs::write(&cached_file, b"test-minecraft-jar-content").unwrap();

        let target_file = server_dir.join("server.jar");
        if target_file.exists() {
            let _ = fs::remove_file(&target_file);
        }

        if fs::hard_link(&cached_file, &target_file).is_err() {
            fs::copy(&cached_file, &target_file).unwrap();
        }

        assert!(target_file.exists());
        assert_eq!(fs::read(&target_file).unwrap(), b"test-minecraft-jar-content");

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let meta1 = fs::metadata(&cached_file).unwrap();
            let meta2 = fs::metadata(&target_file).unwrap();
            if meta1.dev() == meta2.dev() {
                assert_eq!(meta1.ino(), meta2.ino());
            }
        }

        let _ = fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_cache_manager_integration() {
        let temp_dir = tempfile::tempdir().unwrap();
        let paths = CraftPaths::from_base(temp_dir.path().to_path_buf());
        let cache = CacheManager::new(&paths);

        assert_eq!(cache.get_cache_size(), 0);
        let stats = cache.get_stats();
        assert_eq!(stats.artifacts_count, 0);
        assert_eq!(stats.metadata_count, 0);

        // Put an artifact directly through store
        let (_, meta) = cache.store().put_artifact("test-server.jar", b"jar content", None).unwrap();
        assert_eq!(meta.size_bytes, 11);
        assert_eq!(cache.get_cache_size(), 11);

        // Clean cache
        let cleaned = cache.clean_cache().unwrap();
        assert_eq!(cleaned, 11);
        assert_eq!(cache.get_cache_size(), 0);
    }
}

