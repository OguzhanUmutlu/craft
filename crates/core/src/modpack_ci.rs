use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::str::FromStr;

pub const DELTA_MAGIC: &[u8; 8] = b"CRFTDLTA";
pub const DELTA_VERSION: u16 = 1;
pub const DEFAULT_DELTA_BLOCK_SIZE: u32 = 4096;

/// Target deployment side for a mod or component
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ModSide {
    ClientOnly,
    ServerOnly,
    Both,
}

impl std::fmt::Display for ModSide {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ModSide::ClientOnly => write!(f, "client"),
            ModSide::ServerOnly => write!(f, "server"),
            ModSide::Both => write!(f, "both"),
        }
    }
}

impl FromStr for ModSide {
    type Err = String;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "client" | "client_only" | "clientonly" => Ok(ModSide::ClientOnly),
            "server" | "server_only" | "serveronly" => Ok(ModSide::ServerOnly),
            "both" | "universal" => Ok(ModSide::Both),
            _ => Err(format!(
                "Unknown mod side: '{}'. Valid sides are: client, server, both",
                s
            )),
        }
    }
}

/// Metadata for an individual mod or resource in a modpack
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModpackComponent {
    pub file_path: String,
    pub sha512: String,
    pub size_bytes: u64,
    pub side: ModSide,
    #[serde(default)]
    pub download_source: Option<String>,
}

/// Build manifest detailing a specific modpack release
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ModpackBuildManifest {
    pub name: String,
    pub version: String,
    pub loader: String,
    pub minecraft_version: String,
    pub created_at: u64,
    #[serde(default)]
    pub components: Vec<ModpackComponent>,
    #[serde(default)]
    pub server_archive_hash: Option<String>,
    #[serde(default)]
    pub client_archive_hash: Option<String>,
    #[serde(default)]
    pub metadata: HashMap<String, String>,
}

/// Header prepended to a binary delta patch payload
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BinaryDeltaHeader {
    pub magic: [u8; 8],
    pub version: u16,
    pub block_size: u32,
    pub source_sha256: String,
    pub target_sha256: String,
    pub original_size: u64,
    pub target_size: u64,
}

impl Default for BinaryDeltaHeader {
    fn default() -> Self {
        Self {
            magic: *DELTA_MAGIC,
            version: DELTA_VERSION,
            block_size: DEFAULT_DELTA_BLOCK_SIZE,
            source_sha256: String::new(),
            target_sha256: String::new(),
            original_size: 0,
            target_size: 0,
        }
    }
}

/// Atomic binary diff operation
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum DeltaOp {
    /// Copy a range of bytes from the source archive
    Copy { src_offset: u64, length: u32 },
    /// Insert novel bytes not present in the source archive
    Insert { data: Vec<u8> },
}

/// Binary patch manifest describing transition between two versions
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DeltaPatchManifest {
    pub pack_name: String,
    pub source_version: String,
    pub target_version: String,
    pub delta_file: String,
    pub delta_size: u64,
    pub full_size: u64,
    pub reduction_percent: f64,
    pub source_sha256: String,
    pub target_sha256: String,
    pub created_at: u64,
}

/// Registry entry storing all releases and delta patches for a modpack
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModpackRecord {
    pub name: String,
    #[serde(default)]
    pub versions: Vec<ModpackBuildManifest>,
    #[serde(default)]
    pub deltas: Vec<DeltaPatchManifest>,
}

/// Advisory-locked TOML registry of all modpacks on this host
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ModpackRegistry {
    #[serde(default)]
    pub modpacks: HashMap<String, ModpackRecord>,
}

impl ModpackRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if paths.modpack_registry_file.exists() {
            let content = fs::read_to_string(&paths.modpack_registry_file)?;
            let reg: ModpackRegistry = toml::from_str(&content).map_err(|e| {
                CraftError::Config(format!("Failed to parse modpacks.toml: {}", e))
            })?;
            return Ok(reg);
        }
        Ok(Self::default())
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let _ = fs::create_dir_all(&paths.locks_dir);
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&paths.modpack_lock)
            .map_err(|e| CraftError::Config(format!("Failed to open modpack.lock: {}", e)))?;

        lock_file
            .lock_exclusive()
            .map_err(|e| CraftError::Config(format!("Failed to acquire modpack.lock: {}", e)))?;

        let content = toml::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize modpacks.toml: {}", e)))?;
        let temp_path = paths.modpack_registry_file.with_extension("tmp");
        fs::write(&temp_path, content)?;
        fs::rename(&temp_path, &paths.modpack_registry_file)?;

        let _ = lock_file.unlock();
        Ok(())
    }

    pub fn register_build(&mut self, manifest: ModpackBuildManifest) {
        let entry = self
            .modpacks
            .entry(manifest.name.clone())
            .or_insert_with(|| ModpackRecord {
                name: manifest.name.clone(),
                versions: Vec::new(),
                deltas: Vec::new(),
            });

        if let Some(pos) = entry
            .versions
            .iter()
            .position(|v| v.version == manifest.version)
        {
            entry.versions[pos] = manifest;
        } else {
            entry.versions.push(manifest);
        }
    }

    pub fn register_delta(&mut self, delta: DeltaPatchManifest) {
        let entry = self
            .modpacks
            .entry(delta.pack_name.clone())
            .or_insert_with(|| ModpackRecord {
                name: delta.pack_name.clone(),
                versions: Vec::new(),
                deltas: Vec::new(),
            });

        if let Some(pos) = entry.deltas.iter().position(|d| {
            d.source_version == delta.source_version && d.target_version == delta.target_version
        }) {
            entry.deltas[pos] = delta;
        } else {
            entry.deltas.push(delta);
        }
    }

    pub fn get_manifest(&self, pack_name: &str, version: &str) -> Option<&ModpackBuildManifest> {
        self.modpacks
            .get(pack_name)?
            .versions
            .iter()
            .find(|v| v.version == version)
    }

    pub fn find_delta(
        &self,
        pack_name: &str,
        src_version: &str,
        target_version: &str,
    ) -> Option<&DeltaPatchManifest> {
        self.modpacks
            .get(pack_name)?
            .deltas
            .iter()
            .find(|d| d.source_version == src_version && d.target_version == target_version)
    }

    pub fn list_versions(&self, pack_name: &str) -> Vec<&ModpackBuildManifest> {
        self.modpacks
            .get(pack_name)
            .map(|r| r.versions.iter().collect())
            .unwrap_or_default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mod_side_parsing() {
        assert_eq!(ModSide::from_str("client").unwrap(), ModSide::ClientOnly);
        assert_eq!(ModSide::from_str("server").unwrap(), ModSide::ServerOnly);
        assert_eq!(ModSide::from_str("both").unwrap(), ModSide::Both);
        assert_eq!(ModSide::from_str("universal").unwrap(), ModSide::Both);
        assert!(ModSide::from_str("invalid").is_err());
    }

    #[test]
    fn test_modpack_registry_lifecycle() {
        let mut registry = ModpackRegistry::default();
        let manifest = ModpackBuildManifest {
            name: "speedcraft".to_string(),
            version: "1.0.0".to_string(),
            loader: "fabric".to_string(),
            minecraft_version: "1.20.4".to_string(),
            created_at: 1711200000,
            components: vec![ModpackComponent {
                file_path: "mods/fabric-api.jar".to_string(),
                sha512: "abc".to_string(),
                size_bytes: 1024,
                side: ModSide::Both,
                download_source: None,
            }],
            server_archive_hash: Some("sha256-server".to_string()),
            client_archive_hash: Some("sha256-client".to_string()),
            metadata: HashMap::new(),
        };

        registry.register_build(manifest.clone());
        assert_eq!(registry.list_versions("speedcraft").len(), 1);
        assert_eq!(
            registry.get_manifest("speedcraft", "1.0.0").unwrap().loader,
            "fabric"
        );

        let delta = DeltaPatchManifest {
            pack_name: "speedcraft".to_string(),
            source_version: "1.0.0".to_string(),
            target_version: "1.1.0".to_string(),
            delta_file: "speedcraft_1.0.0_to_1.1.0.patch".to_string(),
            delta_size: 4096,
            full_size: 1048576,
            reduction_percent: 99.6,
            source_sha256: "srcsha".to_string(),
            target_sha256: "tgtsha".to_string(),
            created_at: 1711203600,
        };

        registry.register_delta(delta.clone());
        let found = registry.find_delta("speedcraft", "1.0.0", "1.1.0");
        assert!(found.is_some());
        assert_eq!(found.unwrap().delta_size, 4096);
    }
}
