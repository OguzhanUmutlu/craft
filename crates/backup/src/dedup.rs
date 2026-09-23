use chrono::{DateTime, Utc};
use craft_core::{ChaCha20Poly1305, CraftError, CraftPaths, Result};
use hex;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

pub const MIN_CHUNK_SIZE: usize = 16 * 1024; // 16 KB
pub const TARGET_CHUNK_SIZE: usize = 64 * 1024; // 64 KB
pub const MAX_CHUNK_SIZE: usize = 256 * 1024; // 256 KB
pub const CHUNK_MASK: u32 = 0x0000ffff; // 16 bits -> avg 64 KB

// Pre-computed lookup table for Gear rolling hashing
const GEAR_TABLE: [u32; 256] = [
    0x3198a2e0, 0x517cc1b7, 0x889df28a, 0x47e305e7, 0xa1cf8716, 0x4fa6e3d5, 0x89ad0729, 0xb7976e10,
    0x296a84f5, 0x2280d96f, 0x76b6d516, 0xb1b643a6, 0xa927c4fb, 0xa435eb44, 0x7b5e408d, 0x58249822,
    0x4fb819c9, 0x93309a63, 0x2c0022f4, 0x3d02cf11, 0x486b72a6, 0xa79bb8e5, 0x2052c286, 0x81c7e997,
    0xd95ee72d, 0xee562d2a, 0x6e9a65d5, 0x4d8a16fb, 0xb844dc92, 0xa6607e4d, 0xd0f57564, 0xf6d6ca1f,
    0xc708c353, 0xd47bc085, 0x77c244c0, 0xa3bf5269, 0x29a0f0d2, 0xd888adcf, 0xe4631245, 0xd65ee919,
    0x7179093a, 0x705d8f61, 0xb1758117, 0x6f69c57d, 0x633887d1, 0x83e20e36, 0xa97be4f5, 0x25a07c3f,
    0x8e2b8344, 0x74601174, 0x5291b5c4, 0x7b9c9769, 0x4d53a921, 0xd6b746c5, 0x497cb321, 0x9b114d59,
    0x8798e4f1, 0x21c0ad19, 0x46927d14, 0x487cd521, 0x3e17b819, 0xd0546928, 0x7c248b11, 0xa2c67e91,
    0x6191cb72, 0x19a27c49, 0xd8291074, 0x4a771961, 0x7e290074, 0x66c72951, 0x93d08e55, 0xb68b2091,
    0xa436578b, 0x3d077c12, 0x59902264, 0x9e1276a5, 0x3788a101, 0x6d289053, 0x221147e4, 0x8809c733,
    0x718a2205, 0x93b70861, 0x550186c3, 0x77332210, 0x14f08a99, 0x99cc4421, 0x52e00812, 0x91880054,
    0x1927cb09, 0x33e88701, 0x77c20556, 0xa8b16024, 0xd67098e2, 0x38b00517, 0x71720894, 0x296a84f5,
    0x551109a2, 0x70889104, 0x3d548011, 0x26900224, 0x91776510, 0x88924056, 0xa927c4fb, 0x667709a3,
    0x3108c4a1, 0x87990025, 0x4d53a921, 0x705d8f61, 0x8e2b8344, 0x6e9a65d5, 0xee562d2a, 0x4d8a16fb,
    0x3d02cf11, 0x486b72a6, 0xa79bb8e5, 0x2052c286, 0x81c7e997, 0xd95ee72d, 0x4fb819c9, 0x93309a63,
    0x296a84f5, 0x2280d96f, 0x76b6d516, 0xb1b643a6, 0x517cc1b7, 0x889df28a, 0x47e305e7, 0xa1cf8716,
    0xd95ee72d, 0xee562d2a, 0x6e9a65d5, 0x4d8a16fb, 0xb844dc92, 0xa6607e4d, 0xd0f57564, 0xf6d6ca1f,
    0x4fb819c9, 0x93309a63, 0x2c0022f4, 0x3d02cf11, 0x486b72a6, 0xa79bb8e5, 0x2052c286, 0x81c7e997,
    0x7179093a, 0x705d8f61, 0xb1758117, 0x6f69c57d, 0x633887d1, 0x83e20e36, 0xa97be4f5, 0x25a07c3f,
    0x8e2b8344, 0x74601174, 0x5291b5c4, 0x7b9c9769, 0x4d53a921, 0xd6b746c5, 0x497cb321, 0x9b114d59,
    0x6191cb72, 0x19a27c49, 0xd8291074, 0x4a771961, 0x7e290074, 0x66c72951, 0x93d08e55, 0xb68b2091,
    0xa436578b, 0x3d077c12, 0x59902264, 0x9e1276a5, 0x3788a101, 0x6d289053, 0x221147e4, 0x8809c733,
    0x718a2205, 0x93b70861, 0x550186c3, 0x77332210, 0x14f08a99, 0x99cc4421, 0x52e00812, 0x91880054,
    0x1927cb09, 0x33e88701, 0x77c20556, 0xa8b16024, 0xd67098e2, 0x38b00517, 0x71720894, 0x296a84f5,
    0x551109a2, 0x70889104, 0x3d548011, 0x26900224, 0x91776510, 0x88924056, 0xa927c4fb, 0x667709a3,
    0x3108c4a1, 0x87990025, 0x4d53a921, 0x705d8f61, 0x8e2b8344, 0x6e9a65d5, 0xee562d2a, 0x4d8a16fb,
    0x3d02cf11, 0x486b72a6, 0xa79bb8e5, 0x2052c286, 0x81c7e997, 0xd95ee72d, 0x4fb819c9, 0x93309a63,
    0x296a84f5, 0x2280d96f, 0x76b6d516, 0xb1b643a6, 0x517cc1b7, 0x889df28a, 0x47e305e7, 0xa1cf8716,
    0xc708c353, 0xd47bc085, 0x77c244c0, 0xa3bf5269, 0x29a0f0d2, 0xd888adcf, 0xe4631245, 0xd65ee919,
    0x3198a2e0, 0x517cc1b7, 0x889df28a, 0x47e305e7, 0xa1cf8716, 0x4fa6e3d5, 0x89ad0729, 0xb7976e10,
    0x296a84f5, 0x2280d96f, 0x76b6d516, 0xb1b643a6, 0xa927c4fb, 0xa435eb44, 0x7b5e408d, 0x58249822,
    0x4fb819c9, 0x93309a63, 0x2c0022f4, 0x3d02cf11, 0x486b72a6, 0xa79bb8e5, 0x2052c286, 0x81c7e997,
];

pub struct FastCDC;

impl FastCDC {
    /// Splits an input byte slice into contiguous variable-sized chunk boundaries
    pub fn chunk(data: &[u8]) -> Vec<(usize, usize)> {
        if data.is_empty() {
            return Vec::new();
        }

        let mut chunks = Vec::new();
        let mut start = 0;

        while start < data.len() {
            let remaining = data.len() - start;
            if remaining <= MIN_CHUNK_SIZE {
                chunks.push((start, remaining));
                break;
            }

            let max_len = remaining.min(MAX_CHUNK_SIZE);
            let mut fingerprint: u32 = 0;
            let mut cut_len = max_len;

            for i in MIN_CHUNK_SIZE..max_len {
                let byte = data[start + i];
                fingerprint = (fingerprint << 1).wrapping_add(GEAR_TABLE[byte as usize]);
                if (fingerprint & CHUNK_MASK) == 0 {
                    cut_len = i + 1;
                    break;
                }
            }

            chunks.push((start, cut_len));
            start += cut_len;
        }

        chunks
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChunkRef {
    pub hash: String,
    pub offset: u64,
    pub length: usize,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManifestFileEntry {
    pub relative_path: String,
    pub size_bytes: u64,
    pub file_hash: String,
    pub chunks: Vec<ChunkRef>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct BackupManifest {
    pub manifest_id: String,
    pub server_name: String,
    pub created_at: DateTime<Utc>,
    pub total_raw_bytes: u64,
    pub total_dedup_bytes: u64,
    pub dedup_ratio_pct: f64,
    pub merkle_root: String,
    pub files: Vec<ManifestFileEntry>,
}

pub struct ChunkStore {
    base_dir: PathBuf,
}

impl ChunkStore {
    pub fn new(paths: &CraftPaths) -> Self {
        Self {
            base_dir: paths.chunks_dir.clone(),
        }
    }

    pub fn with_dir(base_dir: PathBuf) -> Self {
        Self { base_dir }
    }

    fn chunk_path(&self, hash: &str) -> PathBuf {
        let prefix = if hash.len() >= 2 { &hash[..2] } else { "00" };
        self.base_dir.join(prefix).join(format!("{}.chunk.zst", hash))
    }

    pub fn has_chunk(&self, hash: &str) -> bool {
        self.chunk_path(hash).exists()
    }

    pub fn store_chunk(&self, raw_data: &[u8], key: Option<&[u8; 32]>) -> Result<String> {
        let mut hasher = Sha256::new();
        hasher.update(raw_data);
        let hash = hex::encode(hasher.finalize());

        let target_file = self.chunk_path(&hash);
        if target_file.exists() {
            return Ok(hash);
        }

        if let Some(parent) = target_file.parent() {
            fs::create_dir_all(parent)?;
        }

        // Compress with zstd
        let compressed = zstd::encode_all(raw_data, 3)
            .map_err(|e| CraftError::Other(format!("Failed to compress chunk with zstd: {}", e)))?;

        // Encrypt if key provided
        let payload = if let Some(k) = key {
            let mut nonce = [0u8; 12];
            let hash_bytes = hex::decode(&hash).unwrap_or_default();
            for (idx, b) in hash_bytes.iter().take(12).enumerate() {
                nonce[idx] = *b;
            }
            let (ciphertext, tag) = ChaCha20Poly1305::encrypt(k, &nonce, b"craft-chunk", &compressed);
            let mut enc_payload = tag.to_vec();
            enc_payload.extend_from_slice(&ciphertext);
            enc_payload
        } else {
            compressed
        };

        let temp_file = target_file.with_extension("tmp");
        fs::write(&temp_file, &payload)?;
        fs::rename(&temp_file, &target_file)?;

        Ok(hash)
    }

    pub fn load_chunk(&self, hash: &str, key: Option<&[u8; 32]>) -> Result<Vec<u8>> {
        let path = self.chunk_path(hash);
        if !path.exists() {
            return Err(CraftError::Other(format!("Chunk '{}' not found in store", hash)));
        }

        let payload = fs::read(&path)?;

        let compressed = if let Some(k) = key {
            if payload.len() < 16 {
                return Err(CraftError::Other("Encrypted chunk corrupt: insufficient tag".to_string()));
            }
            let mut tag = [0u8; 16];
            tag.copy_from_slice(&payload[..16]);
            let ciphertext = &payload[16..];
            let mut nonce = [0u8; 12];
            let hash_bytes = hex::decode(hash).unwrap_or_default();
            for (idx, b) in hash_bytes.iter().take(12).enumerate() {
                nonce[idx] = *b;
            }
            ChaCha20Poly1305::decrypt(k, &nonce, b"craft-chunk", ciphertext, &tag)?
        } else {
            payload
        };

        let decompressed = zstd::decode_all(&compressed[..])
            .map_err(|e| CraftError::Other(format!("Failed to decompress chunk: {}", e)))?;

        Ok(decompressed)
    }
}

pub struct DeduplicationEngine {
    paths: CraftPaths,
    chunk_store: ChunkStore,
}

impl DeduplicationEngine {
    pub fn new(paths: &CraftPaths) -> Self {
        Self {
            paths: paths.clone(),
            chunk_store: ChunkStore::new(paths),
        }
    }

    pub fn with_store(paths: &CraftPaths, chunk_store: ChunkStore) -> Self {
        Self {
            paths: paths.clone(),
            chunk_store,
        }
    }

    pub fn create_snapshot(
        &self,
        server_name: &str,
        server_path: &Path,
        key: Option<&[u8; 32]>,
    ) -> Result<BackupManifest> {
        if !server_path.exists() {
            return Err(CraftError::InvalidPath(server_path.to_string_lossy().to_string()));
        }

        let mut files = Vec::new();
        let mut total_raw_bytes = 0u64;
        let mut chunk_hashes_set = std::collections::HashSet::new();
        let mut chunk_hashes_ordered = Vec::new();

        self.scan_and_chunk(
            server_path,
            server_path,
            key,
            &mut files,
            &mut total_raw_bytes,
            &mut chunk_hashes_set,
            &mut chunk_hashes_ordered,
        )?;

        let mut total_dedup_bytes = 0u64;
        for hash in &chunk_hashes_set {
            let path = self.chunk_store.chunk_path(hash);
            if let Ok(meta) = fs::metadata(path) {
                total_dedup_bytes += meta.len();
            }
        }

        let dedup_ratio_pct = if total_raw_bytes > 0 {
            let saved = total_raw_bytes.saturating_sub(total_dedup_bytes);
            (saved as f64 / total_raw_bytes as f64) * 100.0
        } else {
            0.0
        };

        chunk_hashes_ordered.sort();
        let merkle_root = crate::merkle::compute_merkle_root(&chunk_hashes_ordered);

        let manifest = BackupManifest {
            manifest_id: format!("{}-{}", server_name, Utc::now().format("%Y%m%d_%H%M%S")),
            server_name: server_name.to_string(),
            created_at: Utc::now(),
            total_raw_bytes,
            total_dedup_bytes,
            dedup_ratio_pct,
            merkle_root,
            files,
        };

        // Save manifest in server backup directory
        let backup_dir = self.paths.backups_dir.join(server_name);
        fs::create_dir_all(&backup_dir)?;
        let manifest_path = backup_dir.join(format!("{}.manifest.json", manifest.manifest_id));
        let serialized = serde_json::to_string_pretty(&manifest)
            .map_err(|e| CraftError::Other(format!("Failed to serialize manifest: {}", e)))?;
        fs::write(&manifest_path, serialized)?;

        Ok(manifest)
    }

    fn scan_and_chunk(
        &self,
        root: &Path,
        current: &Path,
        key: Option<&[u8; 32]>,
        files: &mut Vec<ManifestFileEntry>,
        total_raw_bytes: &mut u64,
        chunk_hashes_set: &mut std::collections::HashSet<String>,
        chunk_hashes_ordered: &mut Vec<String>,
    ) -> Result<()> {
        if current.is_file() {
            let rel_path = current
                .strip_prefix(root)
                .map_err(|_| CraftError::Other("Path strip failure".to_string()))?
                .to_string_lossy()
                .replace('\\', "/");

            // Ignore temporary locks, sockets, and logs
            if rel_path.ends_with(".lock")
                || rel_path.ends_with(".pid")
                || rel_path.ends_with(".sock")
                || rel_path.starts_with("cache/")
            {
                return Ok(());
            }

            let mut file_bytes = Vec::new();
            let mut file = File::open(current)?;
            file.read_to_end(&mut file_bytes)?;

            let size_bytes = file_bytes.len() as u64;
            *total_raw_bytes += size_bytes;

            let mut file_hasher = Sha256::new();
            file_hasher.update(&file_bytes);
            let file_hash = hex::encode(file_hasher.finalize());

            let chunk_slices = FastCDC::chunk(&file_bytes);
            let mut chunks = Vec::new();

            for (offset, length) in chunk_slices {
                let slice = &file_bytes[offset..offset + length];
                let chunk_hash = self.chunk_store.store_chunk(slice, key)?;

                if chunk_hashes_set.insert(chunk_hash.clone()) {
                    chunk_hashes_ordered.push(chunk_hash.clone());
                }

                chunks.push(ChunkRef {
                    hash: chunk_hash,
                    offset: offset as u64,
                    length,
                });
            }

            files.push(ManifestFileEntry {
                relative_path: rel_path,
                size_bytes,
                file_hash,
                chunks,
            });
            return Ok(());
        }

        if current.is_dir() {
            for entry in fs::read_dir(current)? {
                let entry = entry?;
                self.scan_and_chunk(
                    root,
                    &entry.path(),
                    key,
                    files,
                    total_raw_bytes,
                    chunk_hashes_set,
                    chunk_hashes_ordered,
                )?;
            }
        }

        Ok(())
    }

    pub fn reconstitute(
        &self,
        manifest: &BackupManifest,
        target_dir: &Path,
        key: Option<&[u8; 32]>,
    ) -> Result<()> {
        fs::create_dir_all(target_dir)?;

        for file_entry in &manifest.files {
            let out_path = target_dir.join(&file_entry.relative_path);
            if let Some(parent) = out_path.parent() {
                fs::create_dir_all(parent)?;
            }

            let mut reconstructed = Vec::with_capacity(file_entry.size_bytes as usize);

            for chunk_ref in &file_entry.chunks {
                let chunk_bytes = self.chunk_store.load_chunk(&chunk_ref.hash, key)?;
                reconstructed.extend_from_slice(&chunk_bytes);
            }

            // Verify reconstructed file SHA-256
            let mut hasher = Sha256::new();
            hasher.update(&reconstructed);
            let computed_hash = hex::encode(hasher.finalize());

            if computed_hash != file_entry.file_hash {
                return Err(CraftError::Other(format!(
                    "File integrity check failed for '{}': expected {}, got {}",
                    file_entry.relative_path, file_entry.file_hash, computed_hash
                )));
            }

            let mut out_file = OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&out_path)?;

            out_file.write_all(&reconstructed)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_fastcdc_chunking_deterministic() {
        let mut buffer = Vec::new();
        for i in 0..200_000 {
            buffer.push((i % 256) as u8);
        }

        let chunks1 = FastCDC::chunk(&buffer);
        let chunks2 = FastCDC::chunk(&buffer);

        assert_eq!(chunks1, chunks2);
        assert!(!chunks1.is_empty());

        let mut total_len = 0;
        for (_, len) in chunks1 {
            assert!(len >= MIN_CHUNK_SIZE || len == buffer.len());
            total_len += len;
        }
        assert_eq!(total_len, buffer.len());
    }

    #[test]
    fn test_dedup_snapshot_and_reconstitution_roundtrip() {
        let temp_src = tempdir().unwrap();
        let temp_restore = tempdir().unwrap();
        let temp_base = tempdir().unwrap();

        let paths = CraftPaths::from_base(temp_base.path().to_path_buf());
        let engine = DeduplicationEngine::new(&paths);

        // Populate mock server files
        let test_file1 = temp_src.path().join("server.properties");
        fs::write(&test_file1, "motd=Craft Server\nserver-port=25565\n").unwrap();

        let test_file2 = temp_src.path().join("world").join("region").join("r.0.0.mca");
        fs::create_dir_all(test_file2.parent().unwrap()).unwrap();
        let mut world_bytes = vec![0xAB; 120_000];
        world_bytes[500] = 0xCD;
        fs::write(&test_file2, &world_bytes).unwrap();

        // Create initial snapshot
        let manifest = engine
            .create_snapshot("test-server", temp_src.path(), None)
            .unwrap();

        assert_eq!(manifest.files.len(), 2);
        assert!(manifest.total_raw_bytes > 120_000);

        // Reconstitute into target
        engine
            .reconstitute(&manifest, temp_restore.path(), None)
            .unwrap();

        // Verify content matches exactly
        let restored_prop = fs::read_to_string(temp_restore.path().join("server.properties")).unwrap();
        assert_eq!(restored_prop, "motd=Craft Server\nserver-port=25565\n");

        let restored_mca = fs::read(temp_restore.path().join("world/region/r.0.0.mca")).unwrap();
        assert_eq!(restored_mca, world_bytes);
    }

    #[test]
    fn test_encrypted_chunk_store() {
        let temp = tempdir().unwrap();
        let store = ChunkStore::with_dir(temp.path().to_path_buf());
        let key = [0x77u8; 32];
        let data = b"World chunk with terrain data and player coordinates";

        let hash = store.store_chunk(data, Some(&key)).unwrap();
        assert!(store.has_chunk(&hash));

        let loaded = store.load_chunk(&hash, Some(&key)).unwrap();
        assert_eq!(loaded, data);

        // Decrypt with wrong key should fail
        let wrong_key = [0x88u8; 32];
        let err = store.load_chunk(&hash, Some(&wrong_key));
        assert!(err.is_err());
    }
}
