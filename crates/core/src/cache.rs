use chrono::Utc;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::{CraftError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CacheEntryMeta {
    pub key: String,
    pub rel_path: String,
    pub size_bytes: u64,
    pub is_compressed: bool,
    pub uncompressed_size: u64,
    pub created_at: i64,
    pub last_accessed_at: i64,
    pub access_count: u64,
    pub sha256: Option<String>,
    pub etag: Option<String>,
    pub expires_at: Option<i64>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
}

impl CacheEntryMeta {
    pub fn rel_subpath(&self) -> &str {
        self.key.strip_prefix("artifacts/").unwrap_or(&self.key)
    }

    pub fn display_title(&self) -> &str {
        if let Some(ref t) = self.title {
            if !t.is_empty() {
                return t.as_str();
            }
        }
        let sub = self.rel_subpath();
        Path::new(sub)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(sub)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct CacheIndex {
    #[serde(default)]
    pub version: u32,
    #[serde(default)]
    pub entries: HashMap<String, CacheEntryMeta>,
}

#[derive(Debug, Clone)]
pub struct CacheStats {
    pub total_bytes: u64,
    pub max_bytes: u64,
    pub low_watermark_bytes: u64,
    pub artifacts_count: usize,
    pub artifacts_bytes: u64,
    pub metadata_count: usize,
    pub metadata_compressed_bytes: u64,
    pub metadata_uncompressed_bytes: u64,
    pub savings_bytes: u64,
    pub savings_ratio_pct: f64,
}

pub struct CacheStore {
    cache_dir: PathBuf,
    artifacts_dir: PathBuf,
    meta_dir: PathBuf,
    index_file: PathBuf,
    lock_file: PathBuf,
    max_bytes: u64,
    low_watermark_bytes: u64,
}

impl CacheStore {
    pub fn new(cache_dir: PathBuf, max_bytes: u64) -> Result<Self> {
        let artifacts_dir = cache_dir.join("artifacts");
        let meta_dir = cache_dir.join("meta");
        let index_file = cache_dir.join("index.json");
        let lock_file = cache_dir.join("cache.lock");

        fs::create_dir_all(&artifacts_dir)?;
        fs::create_dir_all(&meta_dir)?;

        let low_watermark_bytes = (max_bytes * 3) / 4; // 75%

        let store = Self {
            cache_dir,
            artifacts_dir,
            meta_dir,
            index_file,
            lock_file,
            max_bytes,
            low_watermark_bytes,
        };

        // Ensure index exists
        if !store.index_file.exists() {
            let initial = CacheIndex {
                version: 1,
                entries: HashMap::new(),
            };
            store.save_index(&initial)?;
        }

        // Migrate legacy flat files if present
        if let Ok(entries) = fs::read_dir(&store.cache_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_file() {
                    let file_name = entry.file_name();
                    let name_str = file_name.to_string_lossy();
                    if name_str != "index.json"
                        && name_str != "cache.lock"
                        && !name_str.ends_with(".tmp")
                    {
                        let target = store.artifacts_dir.join(&*name_str);
                        if !target.exists() {
                            let _ = fs::rename(&path, &target);
                        }
                    }
                }
            }
        }

        Ok(store)
    }

    pub fn set_max_bytes(&mut self, max_bytes: u64) {
        self.max_bytes = max_bytes;
        self.low_watermark_bytes = (max_bytes * 3) / 4;
    }

    pub fn max_bytes(&self) -> u64 {
        self.max_bytes
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    pub fn artifacts_dir(&self) -> &Path {
        &self.artifacts_dir
    }

    pub fn meta_dir(&self) -> &Path {
        &self.meta_dir
    }

    fn lock(&self) -> Result<File> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&self.lock_file)?;
        file.lock_exclusive()?;
        Ok(file)
    }

    fn load_index(&self) -> CacheIndex {
        if self.index_file.exists() {
            if let Ok(data) = fs::read_to_string(&self.index_file) {
                if let Ok(idx) = serde_json::from_str::<CacheIndex>(&data) {
                    return idx;
                }
            }
        }
        CacheIndex {
            version: 1,
            entries: HashMap::new(),
        }
    }

    fn save_index(&self, index: &CacheIndex) -> Result<()> {
        let data = serde_json::to_string_pretty(index)
            .map_err(|e| CraftError::Other(format!("Failed to serialize cache index: {}", e)))?;
        let tmp = self.index_file.with_extension("tmp");
        fs::write(&tmp, data)?;
        fs::rename(&tmp, &self.index_file)?;
        Ok(())
    }

    pub fn put_artifact(
        &self,
        rel_subpath: &str,
        data: &[u8],
        expected_sha256: Option<&str>,
    ) -> Result<(PathBuf, CacheEntryMeta)> {
        let _lock = self.lock()?;

        // Verify hash if supplied
        let actual_hash = compute_sha256_bytes(data);
        if let Some(expected) = expected_sha256 {
            if !actual_hash.eq_ignore_ascii_case(expected) {
                return Err(CraftError::ChecksumMismatch {
                    file: rel_subpath.to_string(),
                    expected: expected.to_string(),
                    actual: actual_hash,
                });
            }
        }

        let full_path = self.artifacts_dir.join(rel_subpath);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp_path = full_path.with_extension("tmp");
        fs::write(&tmp_path, data)?;
        fs::rename(&tmp_path, &full_path)?;

        let size = data.len() as u64;
        let now = Utc::now().timestamp_millis();
        let key = format!("artifacts/{}", rel_subpath);
        let meta = CacheEntryMeta {
            key: key.clone(),
            rel_path: format!("artifacts/{}", rel_subpath),
            size_bytes: size,
            is_compressed: false,
            uncompressed_size: size,
            created_at: now,
            last_accessed_at: now,
            access_count: 1,
            sha256: Some(actual_hash),
            etag: None,
            expires_at: None,
            title: None,
            category: None,
        };

        let mut idx = self.load_index();
        idx.entries.insert(key, meta.clone());
        self.save_index(&idx)?;

        self.prune_if_needed_locked(&mut idx)?;

        Ok((full_path, meta))
    }

    pub fn put_artifact_file(
        &self,
        rel_subpath: &str,
        src_path: &Path,
        expected_sha256: Option<&str>,
    ) -> Result<(PathBuf, CacheEntryMeta)> {
        let _lock = self.lock()?;

        let actual_hash = compute_sha256_file(src_path)?;
        if let Some(expected) = expected_sha256 {
            if !actual_hash.eq_ignore_ascii_case(expected) {
                return Err(CraftError::ChecksumMismatch {
                    file: rel_subpath.to_string(),
                    expected: expected.to_string(),
                    actual: actual_hash,
                });
            }
        }

        let full_path = self.artifacts_dir.join(rel_subpath);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp_path = full_path.with_extension("tmp");
        fs::copy(src_path, &tmp_path)?;
        fs::rename(&tmp_path, &full_path)?;

        let size = fs::metadata(&full_path).map(|m| m.len()).unwrap_or(0);
        let now = Utc::now().timestamp_millis();
        let key = format!("artifacts/{}", rel_subpath);
        let meta = CacheEntryMeta {
            key: key.clone(),
            rel_path: format!("artifacts/{}", rel_subpath),
            size_bytes: size,
            is_compressed: false,
            uncompressed_size: size,
            created_at: now,
            last_accessed_at: now,
            access_count: 1,
            sha256: Some(actual_hash),
            etag: None,
            expires_at: None,
            title: None,
            category: None,
        };

        let mut idx = self.load_index();
        idx.entries.insert(key, meta.clone());
        self.save_index(&idx)?;

        self.prune_if_needed_locked(&mut idx)?;

        Ok((full_path, meta))
    }

    pub fn get_artifact(&self, rel_subpath: &str) -> Option<PathBuf> {
        let full_path = self.artifacts_dir.join(rel_subpath);
        if full_path.is_file() {
            let key = format!("artifacts/{}", rel_subpath);
            if let Ok(_lock) = self.lock() {
                let mut idx = self.load_index();
                if let Some(entry) = idx.entries.get_mut(&key) {
                    entry.last_accessed_at = Utc::now().timestamp_millis();
                    entry.access_count += 1;
                    let _ = self.save_index(&idx);
                }
            }
            Some(full_path)
        } else {
            None
        }
    }

    pub fn has_artifact(&self, rel_subpath: &str) -> bool {
        let clean = rel_subpath
            .strip_prefix("artifacts/")
            .unwrap_or(rel_subpath);
        let key = format!("artifacts/{}", clean);
        let idx = self.load_index();
        if let Some(entry) = idx.entries.get(&key) {
            if self.cache_dir.join(&entry.rel_path).is_file() {
                return true;
            }
        }
        self.artifacts_dir.join(format!("{}.zst", clean)).is_file()
            || self.artifacts_dir.join(clean).is_file()
    }

    pub fn link_or_copy(&self, cached_path: &Path, destination_file: &Path) -> Result<()> {
        if let Some(parent) = destination_file.parent() {
            fs::create_dir_all(parent)?;
        }
        if destination_file.exists() {
            let _ = fs::remove_file(destination_file);
        }

        // Attempt zero-copy hardlink first to share physical disk blocks across servers
        if fs::hard_link(cached_path, destination_file).is_err() {
            // Fallback to copy if cross-filesystem or unsupported
            fs::copy(cached_path, destination_file)?;
        }
        Ok(())
    }

    pub fn put_artifact_compressed(
        &self,
        rel_subpath: &str,
        data: &[u8],
        title: Option<&str>,
        category: Option<&str>,
        expected_sha256: Option<&str>,
    ) -> Result<(PathBuf, CacheEntryMeta)> {
        let _lock = self.lock()?;

        let actual_hash = compute_sha256_bytes(data);
        if let Some(expected) = expected_sha256 {
            if !actual_hash.eq_ignore_ascii_case(expected) {
                return Err(CraftError::ChecksumMismatch {
                    file: rel_subpath.to_string(),
                    expected: expected.to_string(),
                    actual: actual_hash,
                });
            }
        }

        let compressed = zstd::encode_all(data, 3)
            .map_err(|e| CraftError::Other(format!("Zstandard compression failed: {}", e)))?;

        let filename = if rel_subpath.ends_with(".zst") {
            rel_subpath.to_string()
        } else {
            format!("{}.zst", rel_subpath)
        };

        let full_path = self.artifacts_dir.join(&filename);
        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let tmp_path = full_path.with_extension("tmp");
        fs::write(&tmp_path, &compressed)?;
        fs::rename(&tmp_path, &full_path)?;

        let now = Utc::now().timestamp_millis();
        let key = format!("artifacts/{}", rel_subpath);
        let meta = CacheEntryMeta {
            key: key.clone(),
            rel_path: format!("artifacts/{}", filename),
            size_bytes: compressed.len() as u64,
            is_compressed: true,
            uncompressed_size: data.len() as u64,
            created_at: now,
            last_accessed_at: now,
            access_count: 1,
            sha256: Some(actual_hash),
            etag: None,
            expires_at: None,
            title: title.map(|t| t.to_string()),
            category: category.map(|c| c.to_string()),
        };

        let mut idx = self.load_index();
        idx.entries.insert(key, meta.clone());
        self.save_index(&idx)?;

        self.prune_if_needed_locked(&mut idx)?;

        Ok((full_path, meta))
    }

    pub fn get_artifact_data(&self, rel_subpath: &str) -> Result<Option<Vec<u8>>> {
        let clean = rel_subpath
            .strip_prefix("artifacts/")
            .unwrap_or(rel_subpath);
        let key = format!("artifacts/{}", clean);
        let _lock = self.lock()?;
        let mut idx = self.load_index();
        if let Some(entry) = idx.entries.get_mut(&key) {
            entry.last_accessed_at = Utc::now().timestamp_millis();
            entry.access_count += 1;
            let full_path = self.cache_dir.join(&entry.rel_path);
            let is_compressed = entry.is_compressed;
            let _ = self.save_index(&idx);

            if !full_path.is_file() {
                return Ok(None);
            }

            let raw_bytes = fs::read(&full_path)?;
            if is_compressed {
                let decompressed = zstd::decode_all(raw_bytes.as_slice()).map_err(|e| {
                    CraftError::Other(format!("Failed to decompress cached artifact: {}", e))
                })?;
                Ok(Some(decompressed))
            } else {
                Ok(Some(raw_bytes))
            }
        } else {
            let compressed_path = self.artifacts_dir.join(format!("{}.zst", clean));
            if compressed_path.is_file() {
                let raw_bytes = fs::read(&compressed_path)?;
                let decompressed = zstd::decode_all(raw_bytes.as_slice()).map_err(|e| {
                    CraftError::Other(format!("Failed to decompress cached artifact: {}", e))
                })?;
                return Ok(Some(decompressed));
            }
            let uncompressed_path = self.artifacts_dir.join(clean);
            if uncompressed_path.is_file() {
                let raw_bytes = fs::read(&uncompressed_path)?;
                return Ok(Some(raw_bytes));
            }
            Ok(None)
        }
    }

    pub fn extract_artifact_to(&self, rel_subpath: &str, destination_file: &Path) -> Result<()> {
        if let Some(parent) = destination_file.parent() {
            fs::create_dir_all(parent)?;
        }
        if destination_file.exists() {
            let _ = fs::remove_file(destination_file);
        }

        let clean = rel_subpath
            .strip_prefix("artifacts/")
            .unwrap_or(rel_subpath);
        let key = format!("artifacts/{}", clean);
        let (rel_path, is_compressed) = {
            let _lock = self.lock()?;
            let mut idx = self.load_index();
            if let Some(entry) = idx.entries.get_mut(&key) {
                entry.last_accessed_at = Utc::now().timestamp_millis();
                entry.access_count += 1;
                let rel = entry.rel_path.clone();
                let comp = entry.is_compressed;
                let _ = self.save_index(&idx);
                (Some(rel), comp)
            } else {
                (None, false)
            }
        };

        let source_path = if let Some(rel) = rel_path {
            self.cache_dir.join(rel)
        } else {
            let comp = self.artifacts_dir.join(format!("{}.zst", clean));
            if comp.is_file() {
                comp
            } else {
                self.artifacts_dir.join(clean)
            }
        };

        if !source_path.exists() {
            return Err(CraftError::Other(format!(
                "Artifact '{}' not found in cache",
                clean
            )));
        }

        let is_zstd = is_compressed || source_path.extension().map(|e| e == "zst").unwrap_or(false);
        if is_zstd {
            let file = File::open(&source_path)?;
            let mut decoder = zstd::Decoder::new(file)
                .map_err(|e| CraftError::Other(format!("Zstandard decoder error: {}", e)))?;
            let mut dest_file = OpenOptions::new()
                .write(true)
                .create(true)
                .truncate(true)
                .open(destination_file)?;
            std::io::copy(&mut decoder, &mut dest_file)?;
        } else {
            self.link_or_copy(&source_path, destination_file)?;
        }

        Ok(())
    }

    pub fn list_cached_artifacts(&self, category: Option<&str>) -> Vec<CacheEntryMeta> {
        let idx = self.load_index();
        let mut items: Vec<CacheEntryMeta> = idx
            .entries
            .into_values()
            .filter(|e| e.key.starts_with("artifacts/"))
            .filter(|e| match category {
                Some("plugin") | Some("plugins") => {
                    e.category.as_deref() == Some("plugin")
                        || e.category.as_deref() == Some("plugins")
                        || e.key.starts_with("artifacts/plugins/")
                }
                Some("map") | Some("maps") => {
                    e.category.as_deref() == Some("map")
                        || e.category.as_deref() == Some("maps")
                        || e.key.starts_with("artifacts/worlds/")
                        || e.key.starts_with("artifacts/maps/")
                }
                Some(cat) => e.category.as_deref() == Some(cat),
                None => true,
            })
            .collect();

        items.sort_by_key(|a| std::cmp::Reverse(a.last_accessed_at));
        items
    }

    pub fn put_metadata(&self, key: &str, data: &[u8], ttl: Option<Duration>) -> Result<PathBuf> {
        let _lock = self.lock()?;

        // Compress with Zstandard level 3 (fastest high-ratio compression)
        let compressed = zstd::encode_all(data, 3)
            .map_err(|e| CraftError::Other(format!("Zstandard compression failed: {}", e)))?;

        let safe_key = sanitize_cache_key(key);
        let filename = format!("{}.zst", safe_key);
        let full_path = self.meta_dir.join(&filename);

        let tmp_path = full_path.with_extension("tmp");
        fs::write(&tmp_path, &compressed)?;
        fs::rename(&tmp_path, &full_path)?;

        let now = Utc::now().timestamp_millis();
        let expires_at = ttl.map(|t| now + t.as_millis() as i64);
        let meta_key = format!("meta/{}", key);

        let meta = CacheEntryMeta {
            key: meta_key.clone(),
            rel_path: format!("meta/{}", filename),
            size_bytes: compressed.len() as u64,
            is_compressed: true,
            uncompressed_size: data.len() as u64,
            created_at: now,
            last_accessed_at: now,
            access_count: 1,
            sha256: Some(compute_sha256_bytes(data)),
            etag: None,
            expires_at,
            title: None,
            category: Some("metadata".to_string()),
        };

        let mut idx = self.load_index();
        idx.entries.insert(meta_key, meta);
        self.save_index(&idx)?;

        self.prune_if_needed_locked(&mut idx)?;

        Ok(full_path)
    }

    pub fn get_metadata(&self, key: &str) -> Result<Option<Vec<u8>>> {
        let _lock = self.lock()?;
        let meta_key = format!("meta/{}", key);
        let mut idx = self.load_index();

        let (rel_path, is_expired) = match idx.entries.get(&meta_key) {
            Some(entry) => {
                let now = Utc::now().timestamp_millis();
                let expired = entry.expires_at.map(|exp| now >= exp).unwrap_or(false);
                (entry.rel_path.clone(), expired)
            }
            None => return Ok(None),
        };

        let full_path = self.cache_dir.join(&rel_path);

        if is_expired {
            let _ = fs::remove_file(&full_path);
            idx.entries.remove(&meta_key);
            let _ = self.save_index(&idx);
            return Ok(None);
        }

        if !full_path.is_file() {
            idx.entries.remove(&meta_key);
            let _ = self.save_index(&idx);
            return Ok(None);
        }

        let compressed = fs::read(&full_path)?;
        let decompressed = match zstd::decode_all(compressed.as_slice()) {
            Ok(d) => d,
            Err(_) => {
                // Self-healing: remove corrupt cache entry
                let _ = fs::remove_file(&full_path);
                idx.entries.remove(&meta_key);
                let _ = self.save_index(&idx);
                return Ok(None);
            }
        };

        if let Some(entry) = idx.entries.get_mut(&meta_key) {
            entry.last_accessed_at = Utc::now().timestamp_millis();
            entry.access_count += 1;
            let _ = self.save_index(&idx);
        }

        Ok(Some(decompressed))
    }

    pub fn prune_to_watermark(&self) -> Result<u64> {
        let _lock = self.lock()?;
        let mut idx = self.load_index();
        self.prune_to_watermark_locked(&mut idx)
    }

    fn prune_if_needed_locked(&self, idx: &mut CacheIndex) -> Result<u64> {
        let total: u64 = idx.entries.values().map(|e| e.size_bytes).sum();
        if total > self.max_bytes {
            self.prune_to_watermark_locked(idx)
        } else {
            Ok(0)
        }
    }

    fn prune_to_watermark_locked(&self, idx: &mut CacheIndex) -> Result<u64> {
        let now = Utc::now().timestamp_millis();
        let mut freed: u64 = 0;

        // Phase 1: Clean expired metadata entries
        let expired_keys: Vec<String> = idx
            .entries
            .iter()
            .filter(|(_, meta)| meta.expires_at.map(|exp| now >= exp).unwrap_or(false))
            .map(|(k, _)| k.clone())
            .collect();

        for k in expired_keys {
            if let Some(meta) = idx.entries.remove(&k) {
                let p = self.cache_dir.join(&meta.rel_path);
                let _ = fs::remove_file(p);
                freed += meta.size_bytes;
            }
        }

        // Phase 2: LRU Eviction down to low watermark
        let mut current_total: u64 = idx.entries.values().map(|e| e.size_bytes).sum();
        if current_total > self.low_watermark_bytes {
            // Sort by last_accessed_at ascending (oldest first)
            let mut sorted_keys: Vec<(String, i64, u64)> = idx
                .entries
                .iter()
                .map(|(k, v)| (k.clone(), v.last_accessed_at, v.access_count))
                .collect();
            sorted_keys.sort_by(|a, b| a.1.cmp(&b.1).then_with(|| a.2.cmp(&b.2)));

            for (k, _, _) in sorted_keys {
                if current_total <= self.low_watermark_bytes {
                    break;
                }
                if let Some(meta) = idx.entries.remove(&k) {
                    let p = self.cache_dir.join(&meta.rel_path);
                    let _ = fs::remove_file(p);
                    current_total = current_total.saturating_sub(meta.size_bytes);
                    freed += meta.size_bytes;
                }
            }
        }

        self.save_index(idx)?;
        Ok(freed)
    }

    pub fn clean_expired(&self) -> Result<u64> {
        let _lock = self.lock()?;
        let mut idx = self.load_index();
        let now = Utc::now().timestamp_millis();
        let mut freed: u64 = 0;

        let expired_keys: Vec<String> = idx
            .entries
            .iter()
            .filter(|(_, meta)| meta.expires_at.map(|exp| now >= exp).unwrap_or(false))
            .map(|(k, _)| k.clone())
            .collect();

        for k in expired_keys {
            if let Some(meta) = idx.entries.remove(&k) {
                let p = self.cache_dir.join(&meta.rel_path);
                let _ = fs::remove_file(p);
                freed += meta.size_bytes;
            }
        }

        self.save_index(&idx)?;
        Ok(freed)
    }

    pub fn clean_all(&self) -> Result<u64> {
        let _lock = self.lock()?;
        let idx = self.load_index();
        let total: u64 = idx.entries.values().map(|e| e.size_bytes).sum();

        let _ = fs::remove_dir_all(&self.artifacts_dir);
        let _ = fs::remove_dir_all(&self.meta_dir);
        let _ = fs::create_dir_all(&self.artifacts_dir);
        let _ = fs::create_dir_all(&self.meta_dir);

        let clean_idx = CacheIndex {
            version: 1,
            entries: HashMap::new(),
        };
        self.save_index(&clean_idx)?;

        Ok(total)
    }

    pub fn get_stats(&self) -> CacheStats {
        let idx = self.load_index();
        let mut artifacts_count = 0;
        let mut artifacts_bytes = 0;
        let mut metadata_count = 0;
        let mut metadata_compressed_bytes = 0;
        let mut metadata_uncompressed_bytes = 0;

        for meta in idx.entries.values() {
            if meta.is_compressed {
                metadata_count += 1;
                metadata_compressed_bytes += meta.size_bytes;
                metadata_uncompressed_bytes += meta.uncompressed_size;
            } else {
                artifacts_count += 1;
                artifacts_bytes += meta.size_bytes;
            }
        }

        let total_bytes = artifacts_bytes + metadata_compressed_bytes;
        let savings_bytes = metadata_uncompressed_bytes.saturating_sub(metadata_compressed_bytes);
        let savings_ratio_pct = if metadata_uncompressed_bytes > 0 {
            (savings_bytes as f64 / metadata_uncompressed_bytes as f64) * 100.0
        } else {
            0.0
        };

        CacheStats {
            total_bytes,
            max_bytes: self.max_bytes,
            low_watermark_bytes: self.low_watermark_bytes,
            artifacts_count,
            artifacts_bytes,
            metadata_count,
            metadata_compressed_bytes,
            metadata_uncompressed_bytes,
            savings_bytes,
            savings_ratio_pct,
        }
    }
}

fn sanitize_cache_key(key: &str) -> String {
    let cleaned: String = key
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' || c == '-' {
                c
            } else {
                '_'
            }
        })
        .collect();
    if cleaned.len() > 100 {
        let hash = compute_sha256_bytes(key.as_bytes());
        format!("{}_{}", &cleaned[..40], &hash[..16])
    } else {
        cleaned
    }
}

fn compute_sha256_bytes(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex::encode(hasher.finalize())
}

fn compute_sha256_file(path: &Path) -> Result<String> {
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

/// Parses a human-readable size string (e.g., "500MB", "2GB", "1024KB", "1.5G") into bytes
pub fn parse_size(s: &str) -> Option<u64> {
    let s = s.trim().to_uppercase();
    if s.is_empty() {
        return None;
    }

    let (num_part, multiplier): (&str, u64) = if s.ends_with("GIB") || s.ends_with("GB") {
        let prefix = s.trim_end_matches("GIB").trim_end_matches("GB").trim();
        (prefix, 1024 * 1024 * 1024)
    } else if s.ends_with('G') {
        (s.trim_end_matches('G').trim(), 1024 * 1024 * 1024)
    } else if s.ends_with("MIB") || s.ends_with("MB") {
        let prefix = s.trim_end_matches("MIB").trim_end_matches("MB").trim();
        (prefix, 1024 * 1024)
    } else if s.ends_with('M') {
        (s.trim_end_matches('M').trim(), 1024 * 1024)
    } else if s.ends_with("KIB") || s.ends_with("KB") {
        let prefix = s.trim_end_matches("KIB").trim_end_matches("KB").trim();
        (prefix, 1024)
    } else if s.ends_with('K') {
        (s.trim_end_matches('K').trim(), 1024)
    } else if s.ends_with('B') {
        (s.trim_end_matches('B').trim(), 1)
    } else {
        (s.as_str(), 1)
    };

    if let Ok(val) = num_part.parse::<f64>() {
        if val >= 0.0 {
            return Some((val * multiplier as f64) as u64);
        }
    }
    None
}

/// Formats a byte size into a human-readable string (e.g., "25.50 MB", "1.20 GB")
pub fn format_size(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;
    const KIB: f64 = 1024.0;

    let b = bytes as f64;
    if b >= GIB {
        format!("{:.2} GiB", b / GIB)
    } else if b >= MIB {
        format!("{:.2} MiB", b / MIB)
    } else if b >= KIB {
        format!("{:.2} KiB", b / KIB)
    } else {
        format!("{} B", bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_zstd_metadata_compression_and_ratio() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CacheStore::new(tmp.path().to_path_buf(), 1024 * 1024).unwrap();

        // Simulate typical Minecraft JSON version manifest
        let manifest_data = r#"{"latest":{"release":"1.21.4","snapshot":"25w02a"},"versions":[{"id":"1.21.4","type":"release","url":"https://piston-meta.mojang.com/v1/packages/123/1.21.4.json"},{"id":"1.21.3","type":"release","url":"https://piston-meta.mojang.com/v1/packages/456/1.21.3.json"}]}"#.repeat(50);
        let raw_len = manifest_data.len() as u64;

        store
            .put_metadata(
                "test_manifest",
                manifest_data.as_bytes(),
                Some(Duration::from_secs(3600)),
            )
            .unwrap();

        let retrieved = store.get_metadata("test_manifest").unwrap().unwrap();
        assert_eq!(retrieved, manifest_data.as_bytes());

        let stats = store.get_stats();
        assert_eq!(stats.metadata_count, 1);
        assert!(stats.metadata_compressed_bytes < raw_len / 4); // Expect > 75% compression
        assert!(stats.savings_ratio_pct > 70.0);
    }

    #[test]
    fn test_hardlink_deduplication_and_fallback() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CacheStore::new(tmp.path().join("cache"), 10 * 1024 * 1024).unwrap();

        let jar_bytes = b"sample-server-jar-binary-content-123456789";
        let (cached_path, _) = store
            .put_artifact("jars/paper-1.21.4.jar", jar_bytes, None)
            .unwrap();

        let server_dest1 = tmp.path().join("server1").join("server.jar");
        let server_dest2 = tmp.path().join("server2").join("server.jar");

        store.link_or_copy(&cached_path, &server_dest1).unwrap();
        store.link_or_copy(&cached_path, &server_dest2).unwrap();

        assert_eq!(fs::read(&server_dest1).unwrap(), jar_bytes);
        assert_eq!(fs::read(&server_dest2).unwrap(), jar_bytes);

        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            let meta_cache = fs::metadata(&cached_path).unwrap();
            let meta_s1 = fs::metadata(&server_dest1).unwrap();
            let meta_s2 = fs::metadata(&server_dest2).unwrap();
            if meta_cache.dev() == meta_s1.dev() {
                assert_eq!(meta_cache.ino(), meta_s1.ino());
                assert_eq!(meta_cache.ino(), meta_s2.ino());
                assert!(meta_cache.nlink() >= 3);
            }
        }
    }

    #[test]
    fn test_lru_watermark_eviction() {
        let tmp = tempfile::tempdir().unwrap();
        // Max limit 1000 bytes, watermark 750 bytes
        let store = CacheStore::new(tmp.path().join("cache"), 1000).unwrap();

        // Write 3 artifacts of 300 bytes each (total 900 bytes <= 1000)
        let chunk1 = vec![1u8; 300];
        let chunk2 = vec![2u8; 300];
        let chunk3 = vec![3u8; 300];

        store.put_artifact("item1.bin", &chunk1, None).unwrap();
        std::thread::sleep(Duration::from_millis(15));
        store.put_artifact("item2.bin", &chunk2, None).unwrap();
        std::thread::sleep(Duration::from_millis(15));
        store.put_artifact("item3.bin", &chunk3, None).unwrap();

        assert_eq!(store.get_stats().total_bytes, 900);

        // Access item1 to make it newer than item2 in LRU ordering
        std::thread::sleep(Duration::from_millis(15));
        assert!(store.get_artifact("item1.bin").is_some());

        // Adding 4th item of 300 bytes pushes total to 1200 > 1000, triggering prune to <= 750
        let chunk4 = vec![4u8; 300];
        store.put_artifact("item4.bin", &chunk4, None).unwrap();

        let stats = store.get_stats();
        assert!(stats.total_bytes <= 750);
        // item2 was the oldest accessed, so item2 must have been evicted!
        assert!(store.get_artifact("item2.bin").is_none());
        assert!(store.get_artifact("item4.bin").is_some());
    }

    #[test]
    fn test_ttl_metadata_expiration() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CacheStore::new(tmp.path().join("cache"), 1024 * 1024).unwrap();

        // 1 second TTL
        store
            .put_metadata("short_lived", b"data", Some(Duration::from_secs(1)))
            .unwrap();
        assert!(store.get_metadata("short_lived").unwrap().is_some());

        // Sleep 1.2 seconds to expire
        std::thread::sleep(Duration::from_millis(1200));

        assert!(store.get_metadata("short_lived").unwrap().is_none());
        assert_eq!(store.get_stats().metadata_count, 0);
    }

    #[test]
    fn test_parse_and_format_size() {
        assert_eq!(parse_size("500MB"), Some(500 * 1024 * 1024));
        assert_eq!(parse_size("2GB"), Some(2 * 1024 * 1024 * 1024));
        assert_eq!(
            parse_size("1.5G"),
            Some((1.5 * 1024.0 * 1024.0 * 1024.0) as u64)
        );
        assert_eq!(parse_size("1024KB"), Some(1024 * 1024));
        assert_eq!(parse_size("4096B"), Some(4096));
        assert_eq!(parse_size("invalid"), None);

        assert_eq!(format_size(500), "500 B");
        assert_eq!(format_size(1024), "1.00 KiB");
        assert_eq!(format_size(1048576), "1.00 MiB");
        assert_eq!(format_size(1073741824), "1.00 GiB");
    }

    #[test]
    fn test_put_artifact_compressed_and_extract() {
        let tmp = tempfile::tempdir().unwrap();
        let store = CacheStore::new(tmp.path().join("cache"), 10 * 1024 * 1024).unwrap();

        let sample_data = b"Lorem ipsum dolor sit amet, consectetur adipiscing elit. ".repeat(100);
        let (path, meta) = store
            .put_artifact_compressed(
                "plugins/EssentialsX.jar",
                &sample_data,
                Some("EssentialsX"),
                Some("plugin"),
                None,
            )
            .unwrap();

        assert!(path.exists());
        assert!(meta.is_compressed);
        assert!(meta.size_bytes < meta.uncompressed_size);
        assert_eq!(meta.uncompressed_size, sample_data.len() as u64);

        // Verify retrieval of uncompressed bytes
        let retrieved = store
            .get_artifact_data("plugins/EssentialsX.jar")
            .unwrap()
            .unwrap();
        assert_eq!(retrieved, sample_data);

        // Verify extraction directly into a destination file
        let dest = tmp.path().join("server/plugins/EssentialsX.jar");
        store
            .extract_artifact_to("plugins/EssentialsX.jar", &dest)
            .unwrap();
        assert!(dest.exists());
        assert_eq!(fs::read(&dest).unwrap(), sample_data);

        // Verify listing by category
        let plugins = store.list_cached_artifacts(Some("plugin"));
        assert_eq!(plugins.len(), 1);
        assert_eq!(plugins[0].title.as_deref(), Some("EssentialsX"));

        let maps = store.list_cached_artifacts(Some("map"));
        assert_eq!(maps.len(), 0);
    }
}
