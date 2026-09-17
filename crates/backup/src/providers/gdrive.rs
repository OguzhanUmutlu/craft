use super::{CloudBackupEntry, StorageProvider};
use craft_core::{CraftError, GDriveBackupConfig, Result};
use reqwest::Client;
use serde::Deserialize;
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[derive(Debug, Deserialize)]
struct GDriveFileList {
    #[serde(default)]
    files: Vec<GDriveFileItem>,
}

#[derive(Debug, Deserialize)]
struct GDriveFileItem {
    id: String,
    name: String,
    size: Option<String>,
    #[serde(rename = "modifiedTime")]
    modified_time: Option<String>,
}

pub struct GDriveStorageProvider {
    config: GDriveBackupConfig,
    client: Client,
}

fn url_encode(input: &str) -> String {
    let mut encoded = String::new();
    for byte in input.bytes() {
        match byte {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                encoded.push(byte as char);
            }
            _ => {
                encoded.push_str(&format!("%{:02X}", byte));
            }
        }
    }
    encoded
}

impl GDriveStorageProvider {
    pub fn new(config: GDriveBackupConfig) -> Self {
        let client = Client::builder()
            .user_agent("Craft-Backup/1.0")
            .build()
            .unwrap_or_default();
        Self { config, client }
    }

    fn get_auth_token(&self) -> Result<String> {
        if let Some(ref token) = self.config.api_token {
            if !token.trim().is_empty() {
                return Ok(token.trim().to_string());
            }
        }

        if let Some(ref sa_path) = self.config.service_account_path {
            if sa_path.exists() {
                if let Ok(content) = std::fs::read_to_string(sa_path) {
                    // Try parsing raw token or json
                    if let Ok(val) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(tok) = val.get("token").and_then(|t| t.as_str()) {
                            return Ok(tok.to_string());
                        }
                    }
                    return Ok(content.trim().to_string());
                }
            }
        }

        Err(CraftError::Other(
            "Google Drive authentication failed: neither api_token nor valid service_account_path provided."
                .to_string(),
        ))
    }

    async fn resolve_file_id(&self, filename: &str) -> Result<Option<String>> {
        let token = self.get_auth_token()?;
        let query = format!(
            "'{}' in parents and name = '{}' and trashed = false",
            self.config.folder_id, filename
        );
        let url = format!(
            "https://www.googleapis.com/drive/v3/files?q={}&fields=files(id,name)",
            url_encode(&query)
        );

        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| CraftError::Other(format!("GDrive query failed: {}", e)))?;

        if !resp.status().is_success() {
            return Ok(None);
        }

        let file_list: GDriveFileList = resp
            .json()
            .await
            .map_err(|e| CraftError::Other(format!("Failed to parse GDrive response: {}", e)))?;

        Ok(file_list.files.into_iter().next().map(|f| f.id))
    }
}

impl StorageProvider for GDriveStorageProvider {
    fn provider_name(&self) -> &'static str {
        "Google Drive"
    }

    async fn upload_file(&self, local_path: &Path, remote_key: &str) -> Result<()> {
        let token = self.get_auth_token()?;
        let mut file = File::open(local_path)?;
        let mut body = Vec::new();
        file.read_to_end(&mut body)?;

        let filename = Path::new(remote_key)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(remote_key);

        let metadata = serde_json::json!({
            "name": filename,
            "parents": [self.config.folder_id]
        });

        let boundary = "craft_gdrive_boundary_428190";
        let mut payload = Vec::new();

        // Part 1: Metadata
        payload.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        payload.extend_from_slice(b"Content-Type: application/json; charset=UTF-8\r\n\r\n");
        payload.extend_from_slice(metadata.to_string().as_bytes());
        payload.extend_from_slice(b"\r\n");

        // Part 2: Media
        payload.extend_from_slice(format!("--{}\r\n", boundary).as_bytes());
        payload.extend_from_slice(b"Content-Type: application/gzip\r\n\r\n");
        payload.extend_from_slice(&body);
        payload.extend_from_slice(b"\r\n");

        // End
        payload.extend_from_slice(format!("--{}--\r\n", boundary).as_bytes());

        let url = "https://www.googleapis.com/upload/drive/v3/files?uploadType=multipart";
        let resp = self
            .client
            .post(url)
            .header("Authorization", format!("Bearer {}", token))
            .header(
                "Content-Type",
                format!("multipart/related; boundary={}", boundary),
            )
            .body(payload)
            .send()
            .await
            .map_err(|e| CraftError::Other(format!("GDrive upload failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(CraftError::Other(format!(
                "GDrive upload returned HTTP {}: {}",
                status, text
            )));
        }

        Ok(())
    }

    async fn list_files(&self, prefix: &str) -> Result<Vec<CloudBackupEntry>> {
        let token = self.get_auth_token()?;
        let query = format!("'{}' in parents and trashed = false", self.config.folder_id);
        let url = format!(
            "https://www.googleapis.com/drive/v3/files?q={}&fields=files(id,name,size,modifiedTime)",
            url_encode(&query)
        );

        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| CraftError::Other(format!("GDrive list failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(CraftError::Other(format!(
                "GDrive list returned HTTP {}: {}",
                status, text
            )));
        }

        let file_list: GDriveFileList = resp.json().await.map_err(|e| {
            CraftError::Other(format!("Failed to parse GDrive list response: {}", e))
        })?;

        let mut entries = Vec::new();
        for file in file_list.files {
            if !prefix.is_empty() && !file.name.starts_with(prefix) {
                continue;
            }

            let size = file.size.and_then(|s| s.parse::<u64>().ok()).unwrap_or(0);
            let modified = file.modified_time.unwrap_or_default();

            entries.push(CloudBackupEntry {
                key: file.id,
                filename: file.name,
                size_bytes: size,
                last_modified: modified,
                provider: "Google Drive".to_string(),
            });
        }

        Ok(entries)
    }

    async fn download_file(&self, remote_key: &str, target_path: &Path) -> Result<()> {
        let token = self.get_auth_token()?;
        // remote_key could be file_id or filename
        let file_id = if remote_key.contains('/') || remote_key.ends_with(".gz") {
            let fname = Path::new(remote_key)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(remote_key);
            match self.resolve_file_id(fname).await? {
                Some(id) => id,
                None => {
                    return Err(CraftError::Other(format!(
                        "File '{}' not found in GDrive folder",
                        fname
                    )))
                }
            }
        } else {
            remote_key.to_string()
        };

        let url = format!(
            "https://www.googleapis.com/drive/v3/files/{}?alt=media",
            file_id
        );
        let resp = self
            .client
            .get(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| CraftError::Other(format!("GDrive download failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(CraftError::Other(format!(
                "GDrive download returned HTTP {}: {}",
                status, text
            )));
        }

        let bytes = resp.bytes().await.map_err(|e| {
            CraftError::Other(format!("Failed to read GDrive response body: {}", e))
        })?;

        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target_path, bytes)?;

        Ok(())
    }

    async fn delete_file(&self, remote_key: &str) -> Result<()> {
        let token = self.get_auth_token()?;
        let file_id = if remote_key.contains('/') || remote_key.ends_with(".gz") {
            let fname = Path::new(remote_key)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(remote_key);
            match self.resolve_file_id(fname).await? {
                Some(id) => id,
                None => return Ok(()),
            }
        } else {
            remote_key.to_string()
        };

        let url = format!("https://www.googleapis.com/drive/v3/files/{}", file_id);
        let resp = self
            .client
            .delete(&url)
            .header("Authorization", format!("Bearer {}", token))
            .send()
            .await
            .map_err(|e| CraftError::Other(format!("GDrive delete failed: {}", e)))?;

        if !resp.status().is_success() && resp.status() != reqwest::StatusCode::NOT_FOUND {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(CraftError::Other(format!(
                "GDrive delete returned HTTP {}: {}",
                status, text
            )));
        }

        Ok(())
    }
}
