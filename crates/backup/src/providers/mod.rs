pub mod gdrive;
pub mod local;
pub mod s3;

use craft_core::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

pub use gdrive::GDriveStorageProvider;
pub use local::LocalStorageProvider;
pub use s3::S3StorageProvider;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CloudBackupEntry {
    pub key: String,
    pub filename: String,
    pub size_bytes: u64,
    pub last_modified: String,
    pub provider: String,
}

pub trait StorageProvider: Send + Sync {
    fn provider_name(&self) -> &'static str;
    fn upload_file(
        &self,
        local_path: &Path,
        remote_key: &str,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
    fn list_files(
        &self,
        prefix: &str,
    ) -> impl std::future::Future<Output = Result<Vec<CloudBackupEntry>>> + Send;
    fn download_file(
        &self,
        remote_key: &str,
        target_path: &Path,
    ) -> impl std::future::Future<Output = Result<()>> + Send;
    fn delete_file(&self, remote_key: &str)
        -> impl std::future::Future<Output = Result<()>> + Send;
}
