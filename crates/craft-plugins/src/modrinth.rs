use serde::Deserialize;
use craft_core::{CraftError, Result};

#[derive(Debug, Clone, Deserialize)]
pub struct ModrinthSearchResponse {
    pub hits: Vec<ModrinthHit>,
    pub total_hits: u64,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModrinthHit {
    pub project_id: String,
    pub title: String,
    pub description: String,
    pub client_side: Option<String>,
    pub server_side: Option<String>,
    pub project_type: String,
    pub downloads: u64,
    pub icon_url: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModrinthVersion {
    pub id: String,
    pub version_number: String,
    pub game_versions: Vec<String>,
    pub files: Vec<ModrinthFile>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ModrinthFile {
    pub url: String,
    pub filename: String,
    pub primary: bool,
    pub size: u64,
}

pub struct ModrinthClient {
    client: reqwest::Client,
}

impl ModrinthClient {
    pub fn new() -> Self {
        let client = reqwest::Client::builder()
            .user_agent("Craft/2.0 (Minecraft Server Manager)")
            .build()
            .unwrap_or_default();
        Self { client }
    }

    pub async fn search(&self, query: &str, project_type: Option<&str>) -> Result<Vec<ModrinthHit>> {
        let mut url = format!(
            "https://api.modrinth.com/v2/search?query={}&limit=15",
            urlencoding(query)
        );

        if let Some(pt) = project_type {
            let facet = format!("[[\"project_type:{}\"]]", pt);
            url.push_str(&format!("&facets={}", urlencoding(&facet)));
        }

        let resp = self.client.get(&url).send().await
            .map_err(|e| CraftError::Download(format!("Modrinth search error: {}", e)))?;

        let data: ModrinthSearchResponse = resp.json().await
            .map_err(|e| CraftError::Download(format!("Invalid Modrinth response: {}", e)))?;

        Ok(data.hits)
    }

    pub async fn get_latest_file(&self, project_id: &str) -> Result<ModrinthFile> {
        let url = format!("https://api.modrinth.com/v2/project/{}/version", project_id);
        let resp = self.client.get(&url).send().await
            .map_err(|e| CraftError::Download(format!("Modrinth versions error: {}", e)))?;

        let versions: Vec<ModrinthVersion> = resp.json().await
            .map_err(|e| CraftError::Download(format!("Invalid versions response: {}", e)))?;

        let first = versions.into_iter().next()
            .ok_or_else(|| CraftError::Other("No releases found for this project".to_string()))?;

        let primary_file = first.files.into_iter()
            .find(|f| f.primary)
            .or_else(|| None)
            .ok_or_else(|| CraftError::Other("No downloadable files found in release".to_string()))?;

        Ok(primary_file)
    }
}

fn urlencoding(input: &str) -> String {
    let mut out = String::new();
    for b in input.bytes() {
        if b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b'~' {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}
