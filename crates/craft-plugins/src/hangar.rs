use serde::Deserialize;
use craft_core::{CraftError, Result};

#[derive(Debug, Clone, Deserialize)]
pub struct HangarResponse {
    pub result: Vec<HangarProject>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HangarProject {
    pub name: String,
    pub description: Option<String>,
    pub namespace: HangarNamespace,
    pub stats: HangarStats,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HangarNamespace {
    pub owner: String,
    pub slug: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct HangarStats {
    pub downloads: u64,
}

pub struct HangarClient {
    client: reqwest::Client,
}

impl HangarClient {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("Craft/1.0 (Minecraft Server Manager)")
            .build()
            .unwrap_or_default();
        Self { client }
    }

    pub async fn search(&self, query: &str) -> Result<Vec<HangarProject>> {
        let url = format!(
            "https://hangar.papermc.io/api/v1/projects?q={}&limit=15",
            query
        );

        let resp = self.client.get(&url).send().await
            .map_err(|e| CraftError::Download(format!("Hangar search error: {}", e)))?;

        let data: HangarResponse = resp.json().await
            .map_err(|e| CraftError::Download(format!("Invalid Hangar response: {}", e)))?;

        Ok(data.result)
    }
}
