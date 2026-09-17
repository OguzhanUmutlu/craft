use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use chrono::Utc;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::{self, File, OpenOptions};
use std::io::Read;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TrashItem {
    pub id: String,
    pub original_name: String,
    pub original_path: PathBuf,
    pub server_name: Option<String>,
    pub trashed_at: String,
    pub size_bytes: u64,
    pub content_hash: String,
    pub trash_filename: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrashManifest {
    #[serde(default)]
    pub items: Vec<TrashItem>,
}

pub struct TrashManager {
    paths: CraftPaths,
}

impl TrashManager {
    pub fn new(paths: &CraftPaths) -> Self {
        let _ = fs::create_dir_all(&paths.trash_dir);
        Self {
            paths: paths.clone(),
        }
    }

    pub fn manifest_path(&self) -> PathBuf {
        self.paths.trash_dir.join("manifest.toml")
    }

    pub fn load_manifest(&self) -> Result<TrashManifest> {
        let path = self.manifest_path();
        if !path.exists() {
            return Ok(TrashManifest::default());
        }
        let content = fs::read_to_string(&path)?;
        let manifest: TrashManifest = toml::from_str(&content).map_err(|e| {
            CraftError::Config(format!("Failed to parse trash manifest.toml: {}", e))
        })?;
        Ok(manifest)
    }

    pub fn save_manifest(&self, manifest: &TrashManifest) -> Result<()> {
        let path = self.manifest_path();
        let content = toml::to_string_pretty(manifest).map_err(|e| {
            CraftError::Config(format!("Failed to serialize trash manifest.toml: {}", e))
        })?;

        let lock_path = self.paths.locks_dir.join("trash.lock");
        let _ = fs::create_dir_all(&self.paths.locks_dir);
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)?;
        lock_file.lock_exclusive()?;

        let temp_path = path.with_extension("tmp");
        fs::write(&temp_path, content)?;
        fs::rename(&temp_path, &path)?;

        lock_file.unlock()?;
        Ok(())
    }

    /// Computes SHA-256 hash of a file
    pub fn compute_hash(path: &Path) -> Result<String> {
        let mut file = File::open(path)?;
        let mut hasher = Sha256::new();
        let mut buffer = [0u8; 64 * 1024];

        loop {
            let bytes_read = file.read(&mut buffer)?;
            if bytes_read == 0 {
                break;
            }
            hasher.update(&buffer[..bytes_read]);
        }

        let result = hasher.finalize();
        Ok(hex::encode(result))
    }

    /// Moves a file to the trash bin, logging its original location, metadata, and SHA-256 hash
    pub fn trash_file(&self, original_path: &Path, server_name: Option<&str>) -> Result<TrashItem> {
        if !original_path.exists() {
            return Err(CraftError::InvalidPath(format!(
                "Cannot trash file '{}': file not found.",
                original_path.display()
            )));
        }

        let meta = fs::metadata(original_path)?;
        let size_bytes = meta.len();
        let original_name = original_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("backup.tar.gz")
            .to_string();

        let content_hash = Self::compute_hash(original_path)?;
        let short_id = if content_hash.len() >= 12 {
            content_hash[..12].to_string()
        } else {
            content_hash.clone()
        };

        let mut manifest = self.load_manifest()?;
        let mut id = short_id;
        let mut suffix = 2;
        while manifest.items.iter().any(|i| i.id == id) {
            id = format!("{}-{}", &content_hash[..10], suffix);
            suffix += 1;
        }

        let trash_filename = format!("{}_{}", id, original_name);
        let trash_path = self.paths.trash_dir.join(&trash_filename);

        // Move the file into trash directory
        if fs::rename(original_path, &trash_path).is_err() {
            // Fallback for cross-filesystem move
            fs::copy(original_path, &trash_path)?;
            fs::remove_file(original_path)?;
        }

        let item = TrashItem {
            id,
            original_name,
            original_path: original_path.to_path_buf(),
            server_name: server_name.map(|s| s.to_string()),
            trashed_at: Utc::now().to_rfc3339(),
            size_bytes,
            content_hash,
            trash_filename,
        };

        manifest.items.push(item.clone());
        self.save_manifest(&manifest)?;

        Ok(item)
    }

    /// Lists all items currently in the trash bin
    pub fn list_items(&self) -> Result<Vec<TrashItem>> {
        let manifest = self.load_manifest()?;
        Ok(manifest.items)
    }

    /// Finds a trash item by ID
    pub fn find_item(&self, id: &str) -> Result<Option<TrashItem>> {
        let manifest = self.load_manifest()?;
        Ok(manifest
            .items
            .into_iter()
            .find(|i| i.id == id || i.original_name == id))
    }

    /// Restores a trashed item back to its original location, verifying SHA-256 hash first
    pub fn restore_item(&self, id: &str) -> Result<PathBuf> {
        let mut manifest = self.load_manifest()?;
        let pos = manifest
            .items
            .iter()
            .position(|i| i.id == id || i.original_name == id)
            .ok_or_else(|| CraftError::Other(format!("Trash item '{}' not found", id)))?;

        let item = manifest.items.remove(pos);
        let trash_path = self.paths.trash_dir.join(&item.trash_filename);

        if !trash_path.exists() {
            return Err(CraftError::Other(format!(
                "Archive file '{}' was missing from trash directory",
                trash_path.display()
            )));
        }

        // Verify SHA-256 hash integrity before restoring
        let current_hash = Self::compute_hash(&trash_path)?;
        if current_hash != item.content_hash {
            return Err(CraftError::Other(format!(
                "Integrity check failed for '{}'! SHA-256 mismatch (expected: {}, actual: {}). Restore aborted.",
                item.original_name, item.content_hash, current_hash
            )));
        }

        // Ensure parent directory exists
        if let Some(parent) = item.original_path.parent() {
            fs::create_dir_all(parent)?;
        }

        if item.original_path.exists() {
            return Err(CraftError::Other(format!(
                "Cannot restore '{}': a file already exists at '{}'",
                item.original_name,
                item.original_path.display()
            )));
        }

        // Move back from trash to original location
        if fs::rename(&trash_path, &item.original_path).is_err() {
            fs::copy(&trash_path, &item.original_path)?;
            fs::remove_file(&trash_path)?;
        }

        self.save_manifest(&manifest)?;
        Ok(item.original_path)
    }

    /// Permanently deletes an item from trash
    pub fn delete_permanently(&self, id: &str) -> Result<TrashItem> {
        let mut manifest = self.load_manifest()?;
        let pos = manifest
            .items
            .iter()
            .position(|i| i.id == id || i.original_name == id)
            .ok_or_else(|| CraftError::Other(format!("Trash item '{}' not found", id)))?;

        let item = manifest.items.remove(pos);
        let trash_path = self.paths.trash_dir.join(&item.trash_filename);
        if trash_path.exists() {
            let _ = fs::remove_file(&trash_path);
        }

        self.save_manifest(&manifest)?;
        Ok(item)
    }

    /// Empties all items in the trash bin
    pub fn empty_trash(&self) -> Result<usize> {
        let manifest = self.load_manifest()?;
        let count = manifest.items.len();

        for item in &manifest.items {
            let p = self.paths.trash_dir.join(&item.trash_filename);
            if p.exists() {
                let _ = fs::remove_file(&p);
            }
        }

        let empty_manifest = TrashManifest::default();
        self.save_manifest(&empty_manifest)?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_trash_lifecycle() {
        let temp = tempfile::tempdir().unwrap();
        let paths = CraftPaths::from_base(temp.path().to_path_buf());
        let manager = TrashManager::new(&paths);

        // Create a dummy backup file
        let backup_file = temp.path().join("my-server_2026-09-12.tar.gz");
        fs::write(&backup_file, b"test backup archive content 12345").unwrap();

        // 1. Trash the file
        let item = manager.trash_file(&backup_file, Some("my-server")).unwrap();
        assert_eq!(item.original_name, "my-server_2026-09-12.tar.gz");
        assert_eq!(item.server_name.as_deref(), Some("my-server"));
        assert!(!backup_file.exists());

        let trash_file_path = paths.trash_dir.join(&item.trash_filename);
        assert!(trash_file_path.exists());

        // 2. List trash
        let items = manager.list_items().unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].id, item.id);

        // 3. Restore the file
        let restored_path = manager.restore_item(&item.id).unwrap();
        assert_eq!(restored_path, backup_file);
        assert!(backup_file.exists());
        assert!(!trash_file_path.exists());

        let empty_items = manager.list_items().unwrap();
        assert!(empty_items.is_empty());
    }

    #[test]
    fn test_trash_corrupted_hash_prevention() {
        let temp = tempfile::tempdir().unwrap();
        let paths = CraftPaths::from_base(temp.path().to_path_buf());
        let manager = TrashManager::new(&paths);

        let backup_file = temp.path().join("backup.tar.gz");
        fs::write(&backup_file, b"original clean data").unwrap();

        let item = manager.trash_file(&backup_file, None).unwrap();
        let trash_file_path = paths.trash_dir.join(&item.trash_filename);

        // Tamper with the file in trash
        fs::write(&trash_file_path, b"corrupted tampered data").unwrap();

        // Attempting to restore must fail due to SHA-256 mismatch
        let res = manager.restore_item(&item.id);
        assert!(res.is_err());
        assert!(res
            .unwrap_err()
            .to_string()
            .contains("Integrity check failed"));
    }

    #[test]
    fn test_trash_empty() {
        let temp = tempfile::tempdir().unwrap();
        let paths = CraftPaths::from_base(temp.path().to_path_buf());
        let manager = TrashManager::new(&paths);

        let f1 = temp.path().join("f1.tar.gz");
        let f2 = temp.path().join("f2.tar.gz");
        fs::write(&f1, b"one").unwrap();
        fs::write(&f2, b"two").unwrap();

        manager.trash_file(&f1, None).unwrap();
        manager.trash_file(&f2, None).unwrap();

        assert_eq!(manager.list_items().unwrap().len(), 2);
        let removed = manager.empty_trash().unwrap();
        assert_eq!(removed, 2);
        assert_eq!(manager.list_items().unwrap().len(), 0);
    }
}
