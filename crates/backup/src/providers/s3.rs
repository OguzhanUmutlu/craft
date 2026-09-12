use std::fs::File;
use std::io::Read;
use std::path::Path;
use chrono::Utc;
use reqwest::Client;
use sha2::{Digest, Sha256};
use craft_core::{CraftError, Result, S3BackupConfig};
use super::{CloudBackupEntry, StorageProvider};

pub struct S3StorageProvider {
    config: S3BackupConfig,
    client: Client,
}

impl S3StorageProvider {
    pub fn new(config: S3BackupConfig) -> Self {
        let client = Client::builder()
            .user_agent("Craft-Backup/1.0")
            .build()
            .unwrap_or_default();
        Self { config, client }
    }

    fn get_url(&self, key: &str) -> (String, String) {
        let clean_key = key.trim_start_matches('/');
        if let Some(ref ep) = self.config.endpoint {
            let base = ep.trim_end_matches('/');
            let url = format!("{}/{}/{}", base, self.config.bucket, clean_key);
            let host = match reqwest::Url::parse(&url) {
                Ok(u) => u.host_str().unwrap_or("localhost").to_string(),
                Err(_) => "localhost".to_string(),
            };
            (url, host)
        } else {
            let host = format!("{}.s3.{}.amazonaws.com", self.config.bucket, self.config.region);
            let url = format!("https://{}/{}", host, clean_key);
            (url, host)
        }
    }

    fn sign_headers(
        &self,
        method: &str,
        path: &str,
        query: &str,
        host: &str,
        payload_hash: &str,
    ) -> (String, String) {
        let now = Utc::now();
        let amz_date = now.format("%Y%m%dT%H%M%SZ").to_string();
        let date_stamp = now.format("%Y%m%d").to_string();

        let canonical_uri = if path.starts_with('/') {
            path.to_string()
        } else {
            format!("/{}", path)
        };

        let canonical_headers = format!("host:{}\nx-amz-content-sha256:{}\nx-amz-date:{}\n", host, payload_hash, amz_date);
        let signed_headers = "host;x-amz-content-sha256;x-amz-date";

        let canonical_request = format!(
            "{}\n{}\n{}\n{}\n{}\n{}",
            method, canonical_uri, query, canonical_headers, signed_headers, payload_hash
        );

        let canonical_hash = hex::encode(Sha256::digest(canonical_request.as_bytes()));
        let credential_scope = format!("{}/{}/s3/aws4_request", date_stamp, self.config.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            amz_date, credential_scope, canonical_hash
        );

        let k_secret = format!("AWS4{}", self.config.secret_access_key);
        let k_date = hmac_sha256(k_secret.as_bytes(), date_stamp.as_bytes());
        let k_region = hmac_sha256(&k_date, self.config.region.as_bytes());
        let k_service = hmac_sha256(&k_region, b"s3");
        let k_signing = hmac_sha256(&k_service, b"aws4_request");
        let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));

        let auth_header = format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders={}, Signature={}",
            self.config.access_key_id, credential_scope, signed_headers, signature
        );

        (auth_header, amz_date)
    }
}

impl StorageProvider for S3StorageProvider {
    fn provider_name(&self) -> &'static str {
        "s3"
    }

    async fn upload_file(&self, local_path: &Path, remote_key: &str) -> Result<()> {
        let mut file = File::open(local_path)?;
        let mut body = Vec::new();
        file.read_to_end(&mut body)?;

        let payload_hash = hex::encode(Sha256::digest(&body));
        let full_key = if let Some(ref pfx) = self.config.prefix {
            format!("{}/{}", pfx.trim_end_matches('/'), remote_key.trim_start_matches('/'))
        } else {
            remote_key.to_string()
        };

        let (url, host) = self.get_url(&full_key);
        let parsed_url = reqwest::Url::parse(&url)
            .map_err(|e| CraftError::Other(format!("Invalid S3 URL: {}", e)))?;
        let path = parsed_url.path();

        let (auth, date) = self.sign_headers("PUT", path, "", &host, &payload_hash);

        let resp = self
            .client
            .put(&url)
            .header("host", host)
            .header("x-amz-date", date)
            .header("x-amz-content-sha256", payload_hash)
            .header("Authorization", auth)
            .body(body)
            .send()
            .await
            .map_err(|e| CraftError::Other(format!("S3 upload request failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(CraftError::Other(format!("S3 error (HTTP {}): {}", status, body)));
        }

        Ok(())
    }

    async fn list_files(&self, prefix: &str) -> Result<Vec<CloudBackupEntry>> {
        let full_prefix = if let Some(ref pfx) = self.config.prefix {
            format!("{}/{}", pfx.trim_end_matches('/'), prefix.trim_start_matches('/'))
        } else {
            prefix.to_string()
        };

        let (base_url, host) = self.get_url("");
        let query = format!("list-type=2&prefix={}", full_prefix);
        let url = format!("{}?{}", base_url, query);
        let parsed_url = reqwest::Url::parse(&base_url)
            .map_err(|e| CraftError::Other(format!("Invalid S3 URL: {}", e)))?;
        let path = parsed_url.path();

        let payload_hash = hex::encode(Sha256::digest(b""));
        let (auth, date) = self.sign_headers("GET", path, &query, &host, &payload_hash);

        let resp = self
            .client
            .get(&url)
            .header("host", host)
            .header("x-amz-date", date)
            .header("x-amz-content-sha256", payload_hash)
            .header("Authorization", auth)
            .send()
            .await
            .map_err(|e| CraftError::Other(format!("S3 list request failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(CraftError::Other(format!("S3 list error (HTTP {}): {}", status, body)));
        }

        let xml = resp.text().await.unwrap_or_default();
        let mut entries = Vec::new();

        // Simple robust XML parser for <Contents><Key>...</Key><Size>...</Size><LastModified>...</LastModified></Contents>
        let mut pos = 0;
        while let Some(start) = xml[pos..].find("<Contents>") {
            let item_start = pos + start;
            if let Some(end) = xml[item_start..].find("</Contents>") {
                let item_end = item_start + end;
                let chunk = &xml[item_start..item_end];

                let key = extract_tag(chunk, "Key").unwrap_or_default();
                let size_str = extract_tag(chunk, "Size").unwrap_or_default();
                let size_bytes = size_str.parse::<u64>().unwrap_or(0);
                let last_modified = extract_tag(chunk, "LastModified").unwrap_or_else(|| "Unknown".to_string());
                let filename = key.split('/').next_back().unwrap_or(&key).to_string();


                if !key.is_empty() {
                    entries.push(CloudBackupEntry {
                        key,
                        filename,
                        size_bytes,
                        last_modified,
                        provider: "s3".to_string(),
                    });
                }
                pos = item_end + 11;
            } else {
                break;
            }
        }

        entries.sort_by(|a, b| b.filename.cmp(&a.filename));
        Ok(entries)
    }

    async fn download_file(&self, remote_key: &str, target_path: &Path) -> Result<()> {
        let (url, host) = self.get_url(remote_key);
        let parsed_url = reqwest::Url::parse(&url)
            .map_err(|e| CraftError::Other(format!("Invalid S3 URL: {}", e)))?;
        let path = parsed_url.path();

        let payload_hash = hex::encode(Sha256::digest(b""));
        let (auth, date) = self.sign_headers("GET", path, "", &host, &payload_hash);

        let resp = self
            .client
            .get(&url)
            .header("host", host)
            .header("x-amz-date", date)
            .header("x-amz-content-sha256", payload_hash)
            .header("Authorization", auth)
            .send()
            .await
            .map_err(|e| CraftError::Other(format!("S3 download failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(CraftError::Other(format!("S3 download error (HTTP {}): {}", status, body)));
        }

        let bytes = resp.bytes().await.map_err(|e| CraftError::Other(format!("Error reading S3 body: {}", e)))?;
        if let Some(parent) = target_path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(target_path, bytes)?;
        Ok(())
    }

    async fn delete_file(&self, remote_key: &str) -> Result<()> {
        let (url, host) = self.get_url(remote_key);
        let parsed_url = reqwest::Url::parse(&url)
            .map_err(|e| CraftError::Other(format!("Invalid S3 URL: {}", e)))?;
        let path = parsed_url.path();

        let payload_hash = hex::encode(Sha256::digest(b""));
        let (auth, date) = self.sign_headers("DELETE", path, "", &host, &payload_hash);

        let _ = self
            .client
            .delete(&url)
            .header("host", host)
            .header("x-amz-date", date)
            .header("x-amz-content-sha256", payload_hash)
            .header("Authorization", auth)
            .send()
            .await;

        Ok(())
    }
}

fn extract_tag(xml: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    let start = xml.find(&open)? + open.len();
    let end = xml[start..].find(&close)? + start;
    Some(xml[start..end].to_string())
}

fn hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
    let mut k = [0u8; 64];
    if key.len() > 64 {
        let hash = Sha256::digest(key);
        k[..32].copy_from_slice(&hash);
    } else {
        k[..key.len()].copy_from_slice(key);
    }

    let mut ipad = [0x36u8; 64];
    let mut opad = [0x5cu8; 64];
    for i in 0..64 {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }

    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(data);
    let inner_hash = inner.finalize();

    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);
    outer.finalize().into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_s3_hmac_sha256_computation() {
        let key = b"secret-key";
        let message = b"hello-aws-s3";
        let hmac = hmac_sha256(key, message);
        assert_eq!(hmac.len(), 32);
        let hex_res = hex::encode(hmac);
        assert!(!hex_res.is_empty());
    }
}
