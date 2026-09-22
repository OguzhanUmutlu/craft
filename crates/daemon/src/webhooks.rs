use chrono::{DateTime, Utc};
use craft_core::{CraftPaths, WebhookEndpoint, WebhookEvent, WebhookKind, WebhooksRegistry};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tracing::{error, info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WebhookPayload {
    pub event: WebhookEvent,
    pub timestamp: DateTime<Utc>,
    pub server_name: Option<String>,
    pub server_path: Option<PathBuf>,
    pub exit_code: Option<i32>,
    pub crashes: Option<usize>,
    pub title: String,
    pub message: String,
    pub details: HashMap<String, String>,
}

impl WebhookPayload {
    pub fn server_crash(server: &str, path: &Path, code: i32, crashes: usize) -> Self {
        let mut details = HashMap::new();
        details.insert("server".to_string(), server.to_string());
        details.insert("exit_code".to_string(), code.to_string());
        details.insert("recent_crashes".to_string(), crashes.to_string());
        details.insert("path".to_string(), path.display().to_string());

        Self {
            event: WebhookEvent::ServerCrash,
            timestamp: Utc::now(),
            server_name: Some(server.to_string()),
            server_path: Some(path.to_path_buf()),
            exit_code: Some(code),
            crashes: Some(crashes),
            title: format!("[CRITICAL] Server Crash: {}", server),
            message: format!(
                "Server '{}' terminated unexpectedly with exit code {}. Recent crashes: {}.",
                server, code, crashes
            ),
            details,
        }
    }

    pub fn auto_restart(server: &str, path: &Path, backoff_secs: u64) -> Self {
        let mut details = HashMap::new();
        details.insert("server".to_string(), server.to_string());
        details.insert("backoff_seconds".to_string(), backoff_secs.to_string());
        details.insert("path".to_string(), path.display().to_string());

        Self {
            event: WebhookEvent::AutoRestart,
            timestamp: Utc::now(),
            server_name: Some(server.to_string()),
            server_path: Some(path.to_path_buf()),
            exit_code: None,
            crashes: None,
            title: format!("[WARN] Auto-Restart Scheduled: {}", server),
            message: format!(
                "Server '{}' is restarting automatically after a {}s backoff delay.",
                server, backoff_secs
            ),
            details,
        }
    }

    pub fn circuit_trip(server: &str, path: &Path, crashes: usize, window_secs: u64) -> Self {
        let mut details = HashMap::new();
        details.insert("server".to_string(), server.to_string());
        details.insert("crashes".to_string(), crashes.to_string());
        details.insert("window_seconds".to_string(), window_secs.to_string());
        details.insert("path".to_string(), path.display().to_string());

        Self {
            event: WebhookEvent::CircuitTrip,
            timestamp: Utc::now(),
            server_name: Some(server.to_string()),
            server_path: Some(path.to_path_buf()),
            exit_code: None,
            crashes: Some(crashes),
            title: format!("[CRITICAL] Circuit Breaker Tripped: {}", server),
            message: format!(
                "Crash circuit breaker TRIPPED for server '{}'. Exited {} times in {}s. Auto-restart suspended.",
                server, crashes, window_secs
            ),
            details,
        }
    }

    pub fn backup_complete(
        server: &str,
        path: &Path,
        size_bytes: u64,
        success: bool,
        error_msg: Option<&str>,
    ) -> Self {
        let mut details = HashMap::new();
        details.insert("server".to_string(), server.to_string());
        details.insert("size_bytes".to_string(), size_bytes.to_string());
        details.insert("formatted_size".to_string(), craft_core::format_size(size_bytes));
        details.insert("path".to_string(), path.display().to_string());
        if let Some(err) = error_msg {
            details.insert("error".to_string(), err.to_string());
        }

        let (title, message) = if success {
            (
                format!("[OK] Backup Succeeded: {}", server),
                format!(
                    "Automated backup for server '{}' completed successfully ({}).",
                    server,
                    craft_core::format_size(size_bytes)
                ),
            )
        } else {
            (
                format!("[ERROR] Backup Failed: {}", server),
                format!(
                    "Automated backup for server '{}' failed: {}.",
                    server,
                    error_msg.unwrap_or("Unknown error")
                ),
            )
        };

        Self {
            event: WebhookEvent::BackupComplete,
            timestamp: Utc::now(),
            server_name: Some(server.to_string()),
            server_path: Some(path.to_path_buf()),
            exit_code: None,
            crashes: None,
            title,
            message,
            details,
        }
    }

    pub fn storage_exhaustion(
        path: &Path,
        available_bytes: u64,
        total_bytes: u64,
        percent_free: f64,
    ) -> Self {
        let mut details = HashMap::new();
        details.insert("path".to_string(), path.display().to_string());
        details.insert("available_bytes".to_string(), available_bytes.to_string());
        details.insert("total_bytes".to_string(), total_bytes.to_string());
        details.insert("available".to_string(), craft_core::format_size(available_bytes));
        details.insert("total".to_string(), craft_core::format_size(total_bytes));
        details.insert("percent_free".to_string(), format!("{:.1}%", percent_free));

        Self {
            event: WebhookEvent::StorageExhaustion,
            timestamp: Utc::now(),
            server_name: None,
            server_path: Some(path.to_path_buf()),
            exit_code: None,
            crashes: None,
            title: "[WARN] Storage Warning: Disk Space Running Low".to_string(),
            message: format!(
                "Storage for '{}' is critically low: {} free out of {} ({:.1}% remaining).",
                path.display(),
                craft_core::format_size(available_bytes),
                craft_core::format_size(total_bytes),
                percent_free
            ),
            details,
        }
    }

    pub fn server_start(server: &str, path: &Path) -> Self {
        let mut details = HashMap::new();
        details.insert("server".to_string(), server.to_string());
        details.insert("path".to_string(), path.display().to_string());

        Self {
            event: WebhookEvent::ServerStart,
            timestamp: Utc::now(),
            server_name: Some(server.to_string()),
            server_path: Some(path.to_path_buf()),
            exit_code: None,
            crashes: None,
            title: format!("[INFO] Server Started: {}", server),
            message: format!("Server '{}' was successfully started by supervisor.", server),
            details,
        }
    }

    pub fn server_stop(server: &str, path: &Path, intentional: bool) -> Self {
        let mut details = HashMap::new();
        details.insert("server".to_string(), server.to_string());
        details.insert("intentional".to_string(), intentional.to_string());
        details.insert("path".to_string(), path.display().to_string());

        Self {
            event: WebhookEvent::ServerStop,
            timestamp: Utc::now(),
            server_name: Some(server.to_string()),
            server_path: Some(path.to_path_buf()),
            exit_code: None,
            crashes: None,
            title: format!("[INFO] Server Stopped: {}", server),
            message: format!(
                "Server '{}' was stopped {}.",
                server,
                if intentional { "intentionally" } else { "by supervisor shutdown" }
            ),
            details,
        }
    }

    pub fn test_event(id: &str) -> Self {
        let mut details = HashMap::new();
        details.insert("webhook_id".to_string(), id.to_string());
        details.insert("status".to_string(), "verification_ping".to_string());

        Self {
            event: WebhookEvent::All,
            timestamp: Utc::now(),
            server_name: None,
            server_path: None,
            exit_code: None,
            crashes: None,
            title: format!("[OK] Craft Webhook Test: {}", id),
            message: format!(
                "Test webhook notification received successfully for endpoint '{}'.",
                id
            ),
            details,
        }
    }

    pub fn to_discord(&self) -> serde_json::Value {
        let color: u32 = match self.event {
            WebhookEvent::ServerCrash | WebhookEvent::CircuitTrip => 0xE74C3C, // Red
            WebhookEvent::AutoRestart | WebhookEvent::StorageExhaustion => 0xF1C40F, // Yellow
            WebhookEvent::BackupComplete | WebhookEvent::ServerStart => 0x2ECC71, // Green
            _ => 0x3498DB, // Blue
        };

        let mut fields = Vec::new();
        if let Some(ref s) = self.server_name {
            fields.push(serde_json::json!({
                "name": "Server",
                "value": s,
                "inline": true
            }));
        }
        if let Some(code) = self.exit_code {
            fields.push(serde_json::json!({
                "name": "Exit Code",
                "value": code.to_string(),
                "inline": true
            }));
        }
        if let Some(crashes) = self.crashes {
            fields.push(serde_json::json!({
                "name": "Crashes",
                "value": crashes.to_string(),
                "inline": true
            }));
        }
        for (k, v) in &self.details {
            if k != "server" && k != "exit_code" && k != "crashes" && k != "recent_crashes" {
                fields.push(serde_json::json!({
                    "name": k.replace('_', " "),
                    "value": v,
                    "inline": true
                }));
            }
        }

        serde_json::json!({
            "username": "Craft Supervisor",
            "embeds": [
                {
                    "title": self.title,
                    "description": self.message,
                    "color": color,
                    "fields": fields,
                    "timestamp": self.timestamp.to_rfc3339()
                }
            ]
        })
    }

    pub fn to_slack(&self) -> serde_json::Value {
        let color = match self.event {
            WebhookEvent::ServerCrash | WebhookEvent::CircuitTrip => "#e74c3c",
            WebhookEvent::AutoRestart | WebhookEvent::StorageExhaustion => "#f1c40f",
            WebhookEvent::BackupComplete | WebhookEvent::ServerStart => "#2ecc71",
            _ => "#3498db",
        };

        let mut fields = Vec::new();
        for (k, v) in &self.details {
            fields.push(serde_json::json!({
                "title": k.replace('_', " "),
                "value": v,
                "short": true
            }));
        }

        serde_json::json!({
            "text": format!("{} - {}", self.title, self.message),
            "attachments": [
                {
                    "color": color,
                    "title": self.title,
                    "text": self.message,
                    "fields": fields,
                    "ts": self.timestamp.timestamp()
                }
            ]
        })
    }

    pub fn to_generic_json(&self) -> serde_json::Value {
        serde_json::json!({
            "event": self.event.to_string(),
            "timestamp": self.timestamp.to_rfc3339(),
            "server_name": self.server_name,
            "server_path": self.server_path.as_ref().map(|p| p.to_string_lossy()),
            "exit_code": self.exit_code,
            "crashes": self.crashes,
            "title": self.title,
            "message": self.message,
            "details": self.details
        })
    }
}

pub fn compute_hmac_sha256(key: &[u8], data: &[u8]) -> [u8; 32] {
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

pub struct WebhookDispatcher;

impl WebhookDispatcher {
    pub fn dispatch(payload: WebhookPayload, paths: &CraftPaths) {
        let paths_clone = paths.clone();
        tokio::spawn(async move {
            let registry = match WebhooksRegistry::load(&paths_clone) {
                Ok(r) => r,
                Err(e) => {
                    warn!("Failed to load webhooks registry: {}", e);
                    return;
                }
            };

            let subscribers = registry.subscribers_for(&payload.event);
            if subscribers.is_empty() {
                return;
            }

            let client = Client::builder()
                .timeout(Duration::from_secs(10))
                .user_agent("Craft-Daemon/1.0")
                .build()
                .unwrap_or_default();

            for endpoint in subscribers {
                let ep_clone = endpoint.clone();
                let client_clone = client.clone();
                let payload_clone = payload.clone();

                tokio::spawn(async move {
                    if let Err(e) = Self::send_webhook(&client_clone, &ep_clone, &payload_clone).await {
                        error!("Failed to dispatch webhook '{}' ({}): {}", ep_clone.id, ep_clone.url, e);
                    } else {
                        info!("Successfully dispatched webhook '{}' for event '{}'", ep_clone.id, payload_clone.event);
                    }
                });
            }
        });
    }

    pub async fn send_webhook(
        client: &Client,
        endpoint: &WebhookEndpoint,
        payload: &WebhookPayload,
    ) -> craft_core::Result<()> {
        let body_value = match endpoint.kind {
            WebhookKind::Discord => payload.to_discord(),
            WebhookKind::Slack => payload.to_slack(),
            WebhookKind::GenericJson => payload.to_generic_json(),
        };

        let body_bytes = serde_json::to_vec(&body_value)
            .map_err(|e| craft_core::CraftError::Other(format!("Failed to serialize webhook body: {}", e)))?;

        let mut req = client
            .post(&endpoint.url)
            .header("Content-Type", "application/json")
            .body(body_bytes.clone());

        if let Some(ref secret) = endpoint.secret {
            let hmac = compute_hmac_sha256(secret.as_bytes(), &body_bytes);
            let hex_sig = hex::encode(hmac);
            req = req.header("X-Craft-Signature", format!("sha256={}", hex_sig));
            req = req.header("Authorization", format!("Bearer {}", secret));
        }

        let resp = req
            .send()
            .await
            .map_err(|e| craft_core::CraftError::Other(format!("Webhook HTTP request failed: {}", e)))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let err_text = resp.text().await.unwrap_or_default();
            return Err(craft_core::CraftError::Other(format!(
                "Webhook endpoint returned HTTP status {}: {}",
                status, err_text
            )));
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_discord_payload_formatting() {
        let p = WebhookPayload::server_crash("hub-1", Path::new("/srv/hub-1"), 137, 2);
        let val = p.to_discord();
        assert_eq!(val["username"], "Craft Supervisor");
        let embeds = val["embeds"].as_array().unwrap();
        assert_eq!(embeds.len(), 1);
        assert_eq!(embeds[0]["title"], "[CRITICAL] Server Crash: hub-1");
        assert_eq!(embeds[0]["color"], 0xE74C3C);
    }

    #[test]
    fn test_slack_payload_formatting() {
        let p = WebhookPayload::auto_restart("survival", Path::new("/srv/survival"), 15);
        let val = p.to_slack();
        assert!(val["text"].as_str().unwrap().contains("Auto-Restart"));
        let attachments = val["attachments"].as_array().unwrap();
        assert_eq!(attachments[0]["color"], "#f1c40f");
    }

    #[test]
    fn test_generic_json_payload_formatting() {
        let p = WebhookPayload::backup_complete("creative", Path::new("/srv/creative"), 1048576, true, None);
        let val = p.to_generic_json();
        assert_eq!(val["event"], "backup_complete");
        assert_eq!(val["server_name"], "creative");
        assert!(val["title"].as_str().unwrap().contains("[OK]"));
    }

    #[test]
    fn test_hmac_sha256_computation() {
        let key = b"my-secret-token";
        let data = b"{\"event\":\"server_crash\"}";
        let hmac = compute_hmac_sha256(key, data);
        assert_eq!(hmac.len(), 32);
        let hex_val = hex::encode(hmac);
        assert_eq!(hex_val.len(), 64);
    }
}
