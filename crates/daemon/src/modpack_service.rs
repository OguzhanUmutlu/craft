use craft_core::{
    CraftError, CraftPaths, DeltaPatchManifest, ModSide, ModpackBuildManifest, ModpackRegistry,
    Result,
};
use craft_plugins::{BinaryDeltaEngine, ModpackBuilder};
use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io::{Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};

/// Chunk response payload for HTTP range queries
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModpackChunkResponse {
    pub status_code: u16,
    pub content_range: Option<String>,
    pub total_size: u64,
    pub data: Vec<u8>,
}

/// Parses an RFC 7233 HTTP Range header: bytes=START-END
pub fn parse_http_range(range_header: &str, total_len: u64) -> Option<(u64, u64)> {
    if total_len == 0 {
        return None;
    }
    let trimmed = range_header.trim();
    if !trimmed.starts_with("bytes=") {
        return None;
    }
    let range_part = &trimmed["bytes=".len()..];
    let parts: Vec<&str> = range_part.split('-').collect();
    if parts.len() != 2 {
        return None;
    }

    if parts[0].is_empty() {
        // Suffix range: bytes=-500 (last 500 bytes)
        let suffix_len: u64 = parts[1].parse().ok()?;
        let start = total_len.saturating_sub(suffix_len);
        let end = total_len - 1;
        Some((start, end))
    } else if parts[1].is_empty() {
        // Open range: bytes=500- (from 500 to end)
        let start: u64 = parts[0].parse().ok()?;
        if start >= total_len {
            return None;
        }
        let end = total_len - 1;
        Some((start, end))
    } else {
        // Explicit range: bytes=0-499
        let start: u64 = parts[0].parse().ok()?;
        let end: u64 = parts[1].parse().ok()?;
        if start > end || start >= total_len {
            return None;
        }
        let end = end.min(total_len - 1);
        Some((start, end))
    }
}

/// Daemon service managing continuous modpack builds, binary deltas, and chunk streaming
#[derive(Clone)]
pub struct ModpackDistributionService {
    paths: CraftPaths,
}

impl ModpackDistributionService {
    pub fn new(paths: &CraftPaths) -> Self {
        Self {
            paths: paths.clone(),
        }
    }

    /// Builds a new modpack version, segregating client and server components and creating bundles
    pub fn build_modpack(
        &self,
        name: &str,
        version: &str,
        loader: &str,
        minecraft_version: &str,
        base_dir: &Path,
    ) -> Result<ModpackBuildManifest> {
        let mut manifest = ModpackBuilder::build_manifest(
            name,
            version,
            loader,
            minecraft_version,
            base_dir,
        )?;

        fs::create_dir_all(&self.paths.modpack_ci_dir)?;

        // Export server bundle
        let server_tar = self
            .paths
            .modpack_ci_dir
            .join(format!("{}-{}-server.tar.zst", name, version));
        let server_hash = ModpackBuilder::export_bundle(
            &manifest,
            base_dir,
            Some(ModSide::ServerOnly),
            &server_tar,
        )?;
        manifest.server_archive_hash = Some(server_hash);

        // Export client bundle
        let client_tar = self
            .paths
            .modpack_ci_dir
            .join(format!("{}-{}-client.tar.zst", name, version));
        let client_hash = ModpackBuilder::export_bundle(
            &manifest,
            base_dir,
            Some(ModSide::ClientOnly),
            &client_tar,
        )?;
        manifest.client_archive_hash = Some(client_hash);

        // Update ModpackRegistry under advisory lock
        let mut registry = ModpackRegistry::load(&self.paths)?;
        registry.register_build(manifest.clone());
        registry.save(&self.paths)?;

        Ok(manifest)
    }

    /// Computes and registers a binary delta patch between two modpack versions
    pub fn generate_delta(
        &self,
        pack_name: &str,
        source_version: &str,
        target_version: &str,
    ) -> Result<DeltaPatchManifest> {
        fs::create_dir_all(&self.paths.delta_cache_dir)?;

        let src_tar = self
            .paths
            .modpack_ci_dir
            .join(format!("{}-{}-server.tar.zst", pack_name, source_version));
        let tgt_tar = self
            .paths
            .modpack_ci_dir
            .join(format!("{}-{}-server.tar.zst", pack_name, target_version));

        if !src_tar.exists() {
            return Err(CraftError::InvalidPath(format!(
                "Source modpack archive not found: {}",
                src_tar.display()
            )));
        }
        if !tgt_tar.exists() {
            return Err(CraftError::InvalidPath(format!(
                "Target modpack archive not found: {}",
                tgt_tar.display()
            )));
        }

        let delta_filename = format!("{}_{}_to_{}.delta", pack_name, source_version, target_version);
        let delta_path = self.paths.delta_cache_dir.join(&delta_filename);

        let delta_manifest = BinaryDeltaEngine::compute_file_delta(
            pack_name,
            source_version,
            target_version,
            &src_tar,
            &tgt_tar,
            &delta_path,
            4096,
        )?;

        let mut registry = ModpackRegistry::load(&self.paths)?;
        registry.register_delta(delta_manifest.clone());
        registry.save(&self.paths)?;

        Ok(delta_manifest)
    }

    /// Retrieves all recorded releases and delta patches for a modpack
    pub fn get_status(
        &self,
        pack_name: &str,
    ) -> Result<(Vec<ModpackBuildManifest>, Vec<DeltaPatchManifest>)> {
        let registry = ModpackRegistry::load(&self.paths)?;
        if let Some(record) = registry.modpacks.get(pack_name) {
            Ok((record.versions.clone(), record.deltas.clone()))
        } else {
            Ok((Vec::new(), Vec::new()))
        }
    }

    /// Reads a file or specific byte range supporting RFC 7233 Range headers
    pub fn read_chunk(
        &self,
        file_path_str: &str,
        range_header: Option<&str>,
    ) -> Result<ModpackChunkResponse> {
        let path = PathBuf::from(file_path_str);
        if !path.exists() {
            return Err(CraftError::InvalidPath(format!(
                "Requested file does not exist: {}",
                file_path_str
            )));
        }

        let total_size = fs::metadata(&path)?.len();
        let mut file = File::open(&path)?;

        if let Some(range_str) = range_header {
            if let Some((start, end)) = parse_http_range(range_str, total_size) {
                let chunk_len = (end - start + 1) as usize;
                file.seek(SeekFrom::Start(start))?;
                let mut data = vec![0u8; chunk_len];
                file.read_exact(&mut data)?;

                return Ok(ModpackChunkResponse {
                    status_code: 206,
                    content_range: Some(format!("bytes {}-{}/{}", start, end, total_size)),
                    total_size,
                    data,
                });
            }
        }

        // Return full file if no range header or unparseable range
        let mut data = Vec::with_capacity(total_size as usize);
        file.read_to_end(&mut data)?;

        Ok(ModpackChunkResponse {
            status_code: 200,
            content_range: None,
            total_size,
            data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_parse_http_range() {
        assert_eq!(parse_http_range("bytes=0-499", 1000), Some((0, 499)));
        assert_eq!(parse_http_range("bytes=500-", 1000), Some((500, 999)));
        assert_eq!(parse_http_range("bytes=-200", 1000), Some((800, 999)));
        assert_eq!(parse_http_range("bytes=0-1500", 1000), Some((0, 999)));
        assert_eq!(parse_http_range("invalid", 1000), None);
        assert_eq!(parse_http_range("bytes=1200-1500", 1000), None);
    }

    #[test]
    fn test_chunk_read_ranges() {
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("payload.bin");
        let content: Vec<u8> = (0..255).collect();
        File::create(&file_path).unwrap().write_all(&content).unwrap();

        let paths = CraftPaths::from_base(dir.path().to_path_buf());
        let service = ModpackDistributionService::new(&paths);

        // Full read
        let full = service
            .read_chunk(&file_path.to_string_lossy(), None)
            .unwrap();
        assert_eq!(full.status_code, 200);
        assert_eq!(full.data.len(), 255);
        assert!(full.content_range.is_none());

        // Range read bytes=10-19
        let partial = service
            .read_chunk(&file_path.to_string_lossy(), Some("bytes=10-19"))
            .unwrap();
        assert_eq!(partial.status_code, 206);
        assert_eq!(partial.data.len(), 10);
        assert_eq!(partial.data[0], 10);
        assert_eq!(partial.data[9], 19);
        assert_eq!(
            partial.content_range,
            Some("bytes 10-19/255".to_string())
        );
    }
}
