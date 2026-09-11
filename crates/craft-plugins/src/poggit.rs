use serde::Deserialize;
use craft_core::{CraftError, Result};

#[derive(Debug, Clone, Deserialize)]
pub struct PoggitPlugin {
    pub name: String,
    pub tagline: Option<String>,
    pub version: String,
    pub html_url: String,
}

pub struct PoggitClient {
    client: reqwest::Client,
}

impl PoggitClient {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("Craft/1.0 (Minecraft Server Manager)")
            .build()
            .unwrap_or_default();
        Self { client }
    }

    pub async fn search(&self, query: &str) -> Result<Vec<PoggitPlugin>> {
        let url = format!("https://poggit.pmmp.io/releases.json?name={}", query);
        let resp = self.client.get(&url).send().await
            .map_err(|e| CraftError::Download(format!("Poggit search error: {}", e)))?;

        let list: Vec<PoggitPlugin> = resp.json().await
            .map_err(|e| CraftError::Download(format!("Invalid Poggit response: {}", e)))?;

        Ok(list)
    }
}
