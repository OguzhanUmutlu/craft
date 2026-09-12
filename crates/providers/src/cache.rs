use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use indicatif::{ProgressBar, ProgressStyle};
use reqwest::Client;
use sha2::{Digest, Sha256};
use futures_util::StreamExt;
use craft_core::{CraftError, CraftPaths, Result};

pub struct CacheManager {
    client: Client,
    cache_dir: PathBuf,
}

impl CacheManager {
    pub fn new(paths: &CraftPaths) -> Self {
        let client = Client::builder()
            .user_agent("Craft/1.0 (Minecraft Server Manager)")
            .build()
            .unwrap_or_default();

        Self {
            client,
            cache_dir: paths.cache_dir.clone(),
        }
    }

    pub fn cache_path(&self, software_id: &str, version: &str, filename: &str) -> PathBuf {
        let safe_filename = format!("{}-{}-{}", software_id, version.replace('/', "_"), filename);
        self.cache_dir.join(safe_filename)
    }

    /// Downloads the asset if not already cached, and copies it to destination
    pub async fn fetch_and_install(
        &self,
        software_id: &str,
        version: &str,
        filename: &str,
        url: &str,
        destination: &Path,
        expected_sha256: Option<&str>,
    ) -> Result<PathBuf> {
        let cached = self.cache_path(software_id, version, filename);

        if !cached.exists() {
            self.download_file(url, &cached, filename).await?;

            // Verify checksum if supplied
            if let Some(expected) = expected_sha256 {
                let actual = compute_sha256(&cached)?;
                if !actual.eq_ignore_ascii_case(expected) {
                    let _ = fs::remove_file(&cached);
                    return Err(CraftError::ChecksumMismatch {
                        file: filename.to_string(),
                        expected: expected.to_string(),
                        actual,
                    });
                }
            }
        }

        let target_file = destination.join(filename);
        if let Some(parent) = target_file.parent() {
            fs::create_dir_all(parent)?;
        }
        if target_file.exists() {
            let _ = fs::remove_file(&target_file);
        }

        // Try hardlink first to share disk blocks and save storage across servers
        if fs::hard_link(&cached, &target_file).is_err() {
            // Fallback to copy if cross-filesystem (EXDEV) or unsupported
            fs::copy(&cached, &target_file)?;
        }

        Ok(target_file)
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
        let mut total = 0;
        if let Ok(entries) = fs::read_dir(&self.cache_dir) {
            for entry in entries.flatten() {
                if let Ok(meta) = entry.metadata() {
                    if meta.is_file() {
                        total += meta.len();
                    }
                }
            }
        }
        total
    }

    pub fn clean_cache(&self) -> Result<u64> {
        let total = self.get_cache_size();
        if let Ok(entries) = fs::read_dir(&self.cache_dir) {
            for entry in entries.flatten() {
                let _ = fs::remove_file(entry.path());
            }
        }
        Ok(total)
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

fn compute_sha256(path: &Path) -> Result<String> {
    let mut file = File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 8192];
    loop {
        let count = file.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        hasher.update(&buffer[..count]);
    }
    Ok(hex::encode(hasher.finalize()))
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
}

