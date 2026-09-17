use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};
use serde::{Deserialize, Serialize};
use craft_core::{CraftError, CraftPaths, Result, is_stable_version, sort_versions_descending};
use crate::traits::AssetDownload;
use crate::get_all_softwares;

pub const DEFAULT_CATALOG_URL: &str = "https://craft.oguzhanumutlu.com/api/versions.zst";
pub const DEFAULT_CATALOG_TTL_SECS: u64 = 6 * 3600; // 6 hours

/// Complete software and version catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VersionCatalog {
    pub schema_version: u32,
    pub generated_at: i64,
    pub softwares: HashMap<String, SoftwareCatalogEntry>,
}

impl Default for VersionCatalog {
    fn default() -> Self {
        Self {
            schema_version: 1,
            generated_at: 0,
            softwares: HashMap::new(),
        }
    }
}

impl VersionCatalog {
    pub fn new() -> Self {
        Self {
            schema_version: 1,
            generated_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs() as i64,
            softwares: HashMap::new(),
        }
    }

    /// Compresses and serializes the catalog into zstandard binary bytes (compression level 19).
    pub fn encode_zstd(&self) -> Result<Vec<u8>> {
        let json_bytes = serde_json::to_vec(self)
            .map_err(|e| CraftError::Other(format!("Failed to serialize version catalog: {}", e)))?;
        zstd::encode_all(&json_bytes[..], 19)
            .map_err(|e| CraftError::Other(format!("Failed to zstd-compress version catalog: {}", e)))
    }

    /// Decompresses and deserializes the catalog from zstandard binary bytes.
    pub fn decode_zstd(bytes: &[u8]) -> Result<Self> {
        let decompressed = zstd::decode_all(bytes)
            .map_err(|e| CraftError::Other(format!("Failed to zstd-decompress version catalog: {}", e)))?;
        serde_json::from_slice(&decompressed)
            .map_err(|e| CraftError::Other(format!("Failed to deserialize version catalog: {}", e)))
    }

    /// Gets a software entry by ID.
    pub fn get_software(&self, id: &str) -> Option<&SoftwareCatalogEntry> {
        let lower = id.to_lowercase();
        self.softwares.get(&lower).or_else(|| {
            self.softwares.values().find(|s| {
                s.id.eq_ignore_ascii_case(&lower)
                    || s.name.eq_ignore_ascii_case(&lower)
                    || (lower == "vanilla" && s.id == "vanilla_java")
                    || (lower == "bedrock" && s.id == "vanilla_bedrock")
                    || (lower == "bungee" && s.id == "bungeecord")
                    || (lower == "palworld" && s.id == "palserver")
                    || (lower == "terraria" && s.id == "tshock")
            })
        })
    }

    /// Returns sorted versions for a software, or empty vec if unknown.
    pub fn get_versions(&self, software_id: &str) -> Vec<String> {
        self.get_software(software_id)
            .map(|s| s.versions.clone())
            .unwrap_or_default()
    }

    /// Returns recommended / latest stable version for a software.
    pub fn get_recommended_version(&self, software_id: &str) -> Option<String> {
        self.get_software(software_id)
            .map(|s| s.recommended_version.clone())
    }

    /// Returns pre-resolved asset downloads for a specific software and version.
    pub fn get_assets(&self, software_id: &str, version: &str) -> Option<Vec<AssetDownload>> {
        let sw = self.get_software(software_id)?;
        sw.assets.get(version).map(|assets| {
            assets
                .iter()
                .map(|a| AssetDownload {
                    filename: a.filename.clone(),
                    url: a.url.clone(),
                    sha256: a.sha256.clone(),
                    is_archive: a.is_archive,
                })
                .collect()
        })
    }
}

/// Catalog entry for a specific software implementation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SoftwareCatalogEntry {
    pub id: String,
    pub name: String,
    pub game_id: String,
    pub edition: String,
    pub description: String,
    pub default_server_file: String,
    pub latest_version: String,
    pub recommended_version: String,
    pub versions: Vec<String>,
    #[serde(default)]
    pub assets: HashMap<String, Vec<CatalogAsset>>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Download asset specification within the catalog.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CatalogAsset {
    pub filename: String,
    pub url: String,
    #[serde(default)]
    pub sha256: Option<String>,
    #[serde(default)]
    pub is_archive: bool,
}

/// Manages fetching, caching, and querying the centralized version catalog.
pub struct CatalogManager {
    cache_path: PathBuf,
    remote_url: String,
    ttl: Duration,
}

impl CatalogManager {
    /// Creates a new `CatalogManager` with default paths and remote endpoint.
    pub fn new() -> Result<Self> {
        let paths = CraftPaths::new()?;
        let cache_dir = paths.cache_dir.join("catalog");
        fs::create_dir_all(&cache_dir)?;
        let cache_path = cache_dir.join("versions.zst");

        let remote_url = std::env::var("CRAFT_VERSIONS_URL")
            .unwrap_or_else(|_| DEFAULT_CATALOG_URL.to_string());

        Ok(Self {
            cache_path,
            remote_url,
            ttl: Duration::from_secs(DEFAULT_CATALOG_TTL_SECS),
        })
    }

    /// Sets custom cache path.
    pub fn with_cache_path(mut self, path: impl Into<PathBuf>) -> Self {
        self.cache_path = path.into();
        self
    }

    /// Sets custom remote catalog URL.
    pub fn with_remote_url(mut self, url: impl Into<String>) -> Self {
        self.remote_url = url.into();
        self
    }

    /// Sets custom cache TTL duration.
    pub fn with_ttl(mut self, ttl: Duration) -> Self {
        self.ttl = ttl;
        self
    }

    pub fn cache_path(&self) -> &Path {
        &self.cache_path
    }

    /// Loads the version catalog asynchronously, checking local cache first,
    /// fetching from remote if missing/stale, and falling back to bundled data.
    pub async fn load_or_fetch(&self) -> VersionCatalog {
        // 1. Try local cache if fresh
        if self.is_cache_fresh() {
            if let Ok(catalog) = self.load_from_cache() {
                return catalog;
            }
        }

        // 2. Try fetching from remote worker
        if let Ok(catalog) = self.fetch_remote().await {
            let _ = self.save_to_cache(&catalog);
            return catalog;
        }

        // 3. Fallback: try stale local cache if present
        if let Ok(catalog) = self.load_from_cache() {
            return catalog;
        }

        // 4. Ultimate fallback: build in-memory catalog from bundled manifests
        Self::build_bundled_fallback()
    }

    /// Loads the version catalog synchronously from local cache, or falls back to bundled data.
    pub fn load(&self) -> VersionCatalog {
        if let Ok(catalog) = self.load_from_cache() {
            return catalog;
        }
        Self::build_bundled_fallback()
    }

    /// Forces a refresh from the remote worker and updates the local cache.
    pub async fn update(&self) -> Result<VersionCatalog> {
        let catalog = self.fetch_remote().await?;
        self.save_to_cache(&catalog)?;
        Ok(catalog)
    }

    /// Returns true if local cache exists and is within TTL.
    pub fn is_cache_fresh(&self) -> bool {
        if let Ok(meta) = fs::metadata(&self.cache_path) {
            if let Ok(modified) = meta.modified() {
                if let Ok(elapsed) = modified.elapsed() {
                    return elapsed < self.ttl;
                }
            }
        }
        false
    }

    /// Loads catalog from local disk cache.
    pub fn load_from_cache(&self) -> Result<VersionCatalog> {
        let mut file = File::open(&self.cache_path)
            .map_err(|e| CraftError::Other(format!("Failed to open cached catalog: {}", e)))?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes)
            .map_err(|e| CraftError::Other(format!("Failed to read cached catalog: {}", e)))?;
        VersionCatalog::decode_zstd(&bytes)
    }

    /// Saves catalog to local disk cache.
    pub fn save_to_cache(&self, catalog: &VersionCatalog) -> Result<()> {
        if let Some(parent) = self.cache_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let encoded = catalog.encode_zstd()?;
        let temp_path = self.cache_path.with_extension("tmp");
        {
            let mut file = File::create(&temp_path)?;
            file.write_all(&encoded)?;
            file.flush()?;
        }
        fs::rename(temp_path, &self.cache_path)?;
        Ok(())
    }

    /// Downloads and decodes the catalog from the remote HTTP endpoint.
    pub async fn fetch_remote(&self) -> Result<VersionCatalog> {
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(5))
            .user_agent("Craft-CLI/1.0 (VersionCatalogClient)")
            .build()
            .map_err(|e| CraftError::Download(format!("HTTP client error: {}", e)))?;

        let resp = client
            .get(&self.remote_url)
            .send()
            .await
            .map_err(|e| CraftError::Download(format!("Failed to fetch catalog from {}: {}", self.remote_url, e)))?;

        if !resp.status().is_success() {
            return Err(CraftError::Download(format!(
                "Catalog server returned HTTP {}: {}",
                resp.status(),
                self.remote_url
            )));
        }

        let bytes = resp
            .bytes()
            .await
            .map_err(|e| CraftError::Download(format!("Failed to read catalog response body: {}", e)))?;

        VersionCatalog::decode_zstd(&bytes)
    }

    /// Builds a fallback catalog in-memory from embedded bundled manifests and software definitions.
    pub fn build_bundled_fallback() -> VersionCatalog {
        let mut catalog = VersionCatalog::new();
        let softwares = get_all_softwares();

        for sw in softwares {
            let mut versions = sw.bundled_versions();
            sort_versions_descending(&mut versions);

            let recommended = versions
                .iter()
                .find(|v| is_stable_version(v))
                .cloned()
                .unwrap_or_else(|| versions.first().cloned().unwrap_or_else(|| "latest".to_string()));

            let latest = versions.first().cloned().unwrap_or_else(|| "latest".to_string());

            let mut assets_map = HashMap::new();
            for v in versions.iter().take(10) {
                if let Ok(assets) = sw.get_assets(v) {
                    assets_map.insert(
                        v.clone(),
                        assets
                            .into_iter()
                            .map(|a| CatalogAsset {
                                filename: a.filename,
                                url: a.url,
                                sha256: a.sha256,
                                is_archive: a.is_archive,
                            })
                            .collect(),
                    );
                }
            }

            let entry = SoftwareCatalogEntry {
                id: sw.id().to_string(),
                name: sw.name().to_string(),
                game_id: sw.game_id().to_string(),
                edition: format!("{:?}", sw.edition()),
                description: sw.description().to_string(),
                default_server_file: sw.default_server_file().to_string(),
                latest_version: latest,
                recommended_version: recommended,
                versions,
                assets: assets_map,
                metadata: HashMap::new(),
            };

            catalog.softwares.insert(sw.id().to_string(), entry);
        }

        catalog
    }
}

/// Tooling for building and generating the full version catalog (for use by the VDS worker).
pub struct CatalogBuilder;

impl CatalogBuilder {
    /// Asynchronously queries all upstream software providers, aggregates version data,
    /// and constructs a comprehensive `VersionCatalog`.
    pub async fn build_full_catalog() -> VersionCatalog {
        let mut catalog = VersionCatalog::new();
        let softwares = get_all_softwares();

        println!("Building version catalog for {} server softwares...", softwares.len());

        for sw in softwares {
            print!("  Fetching versions for {:<20} ... ", sw.name());
            let mut versions = match sw.fetch_versions().await {
                Ok(v) if !v.is_empty() => v,
                _ => sw.bundled_versions(),
            };

            sort_versions_descending(&mut versions);
            println!("done ({} versions)", versions.len());

            let recommended = versions
                .iter()
                .find(|v| is_stable_version(v))
                .cloned()
                .unwrap_or_else(|| versions.first().cloned().unwrap_or_else(|| "latest".to_string()));

            let latest = versions.first().cloned().unwrap_or_else(|| "latest".to_string());

            let mut assets_map = HashMap::new();
            // Pre-resolve assets for top versions
            for v in versions.iter().take(20) {
                if let Ok(assets) = sw.get_assets(v) {
                    assets_map.insert(
                        v.clone(),
                        assets
                            .into_iter()
                            .map(|a| CatalogAsset {
                                filename: a.filename,
                                url: a.url,
                                sha256: a.sha256,
                                is_archive: a.is_archive,
                            })
                            .collect(),
                    );
                }
            }

            let entry = SoftwareCatalogEntry {
                id: sw.id().to_string(),
                name: sw.name().to_string(),
                game_id: sw.game_id().to_string(),
                edition: format!("{:?}", sw.edition()),
                description: sw.description().to_string(),
                default_server_file: sw.default_server_file().to_string(),
                latest_version: latest,
                recommended_version: recommended,
                versions,
                assets: assets_map,
                metadata: HashMap::new(),
            };

            catalog.softwares.insert(sw.id().to_string(), entry);
        }

        catalog
    }

    /// Builds the catalog and compresses it to the specified output file.
    pub async fn build_and_save(output_path: &Path) -> Result<usize> {
        let catalog = Self::build_full_catalog().await;
        if let Some(parent) = output_path.parent() {
            fs::create_dir_all(parent)?;
        }
        let encoded = catalog.encode_zstd()?;
        fs::write(output_path, &encoded)?;
        Ok(encoded.len())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_core::compare_versions;

    #[test]
    fn test_catalog_encode_decode_zstd_roundtrip() {
        let mut catalog = VersionCatalog::new();
        let mut versions = vec![
            "1.9.4".to_string(),
            "1.21.4".to_string(),
            "1.21.9-pre4".to_string(),
            "1.21.9".to_string(),
        ];
        sort_versions_descending(&mut versions);

        let entry = SoftwareCatalogEntry {
            id: "paper".to_string(),
            name: "Paper".to_string(),
            game_id: "minecraft".to_string(),
            edition: "Java".to_string(),
            description: "Paper server".to_string(),
            default_server_file: "server.jar".to_string(),
            latest_version: "1.21.9".to_string(),
            recommended_version: "1.21.9".to_string(),
            versions,
            assets: HashMap::new(),
            metadata: HashMap::new(),
        };

        catalog.softwares.insert("paper".to_string(), entry);

        let encoded = catalog.encode_zstd().expect("encode_zstd");
        assert!(!encoded.is_empty());

        let decoded = VersionCatalog::decode_zstd(&encoded).expect("decode_zstd");
        assert_eq!(decoded.schema_version, 1);
        let paper = decoded.get_software("paper").expect("paper software exists");
        assert_eq!(paper.recommended_version, "1.21.9");
        assert_eq!(paper.versions[0], "1.21.9");
        assert_eq!(paper.versions[1], "1.21.9-pre4");
        assert_eq!(paper.versions[2], "1.21.4");
        assert_eq!(paper.versions[3], "1.9.4");
    }

    #[test]
    fn test_bundled_fallback_catalog() {
        let catalog = CatalogManager::build_bundled_fallback();
        assert!(!catalog.softwares.is_empty());
        let paper = catalog.get_software("paper").expect("paper present");
        assert!(!paper.versions.is_empty());
        // Verify natural ordering: first version must be >= 1.20
        assert!(compare_versions(&paper.versions[0], "1.20").is_gt());
        // Verify recommended version is stable
        assert!(is_stable_version(&paper.recommended_version));
    }
}
