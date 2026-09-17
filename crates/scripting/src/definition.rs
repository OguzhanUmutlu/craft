use serde::{Deserialize, Serialize};
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SoftwareMetaSection {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub display_name: Option<String>,
    #[serde(default = "default_game")]
    pub game: String,
    #[serde(default = "default_edition")]
    pub edition: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub version: Option<String>,
    #[serde(default)]
    pub website: Option<String>,
}

fn default_game() -> String {
    String::new()
}

fn default_edition() -> String {
    "native".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct CapabilitiesSection {
    #[serde(default)]
    pub plugins: bool,
    #[serde(default)]
    pub mods: bool,
    #[serde(default)]
    pub datapacks: bool,
    #[serde(default)]
    pub rcon: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeSection {
    #[serde(default = "default_runtime_kind")]
    pub kind: String,
    #[serde(default)]
    pub default_server_file: Option<String>,
    #[serde(default)]
    pub executable: Option<String>,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default)]
    pub stop_method: Option<String>,
    #[serde(default)]
    pub stop_command: Option<String>,
    #[serde(default)]
    pub stop_timeout_seconds: Option<u64>,
}

fn default_runtime_kind() -> String {
    "java".to_string()
}

impl Default for RuntimeSection {
    fn default() -> Self {
        Self {
            kind: default_runtime_kind(),
            default_server_file: None,
            executable: None,
            arguments: Vec::new(),
            stop_method: None,
            stop_command: None,
            stop_timeout_seconds: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct NetworkSection {
    #[serde(default = "default_port")]
    pub default_port: u16,
    #[serde(default)]
    pub default_query_port: Option<u16>,
    #[serde(default = "default_protocol")]
    pub protocol: String,
    #[serde(default)]
    pub query_protocol: Option<String>,
}

fn default_port() -> u16 {
    25565
}

fn default_protocol() -> String {
    "tcp".to_string()
}

impl Default for NetworkSection {
    fn default() -> Self {
        Self {
            default_port: default_port(),
            default_query_port: None,
            protocol: default_protocol(),
            query_protocol: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct VersionsSection {
    #[serde(default)]
    pub recommended: Option<String>,
    #[serde(default)]
    pub bundled: Vec<String>,
    #[serde(default = "default_fetch_mode")]
    pub fetch_mode: String,
    #[serde(default)]
    pub api_url: Option<String>,
    #[serde(default)]
    pub json_path: Option<String>,
    #[serde(default)]
    pub script: Option<String>,
}

fn default_fetch_mode() -> String {
    "static".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct AssetsSection {
    #[serde(default = "default_download_mode")]
    pub download_mode: String,
    #[serde(default)]
    pub url_template: Option<String>,
    #[serde(default)]
    pub filename: Option<String>,
    #[serde(default)]
    pub is_archive: bool,
    #[serde(default)]
    pub strip_components: Option<usize>,
    #[serde(default)]
    pub script: Option<String>,
}

fn default_download_mode() -> String {
    "url_template".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PropertiesSection {
    #[serde(default)]
    pub file: Option<String>,
    #[serde(default)]
    pub format: Option<String>,
    #[serde(default)]
    pub schema_file: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PluginsSection {
    #[serde(default)]
    pub folder: Option<String>,
    #[serde(default)]
    pub search_providers: Vec<String>,
    #[serde(default)]
    pub script: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct DeveloperSection {
    #[serde(default)]
    pub jdwp: bool,
    #[serde(default)]
    pub reload_command: Option<String>,
    #[serde(default)]
    pub scaffolding: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SoftwareDefinition {
    pub software: SoftwareMetaSection,
    #[serde(default)]
    pub capabilities: CapabilitiesSection,
    #[serde(default)]
    pub runtime: RuntimeSection,
    #[serde(default)]
    pub network: NetworkSection,
    #[serde(default)]
    pub versions: VersionsSection,
    #[serde(default)]
    pub assets: AssetsSection,
    #[serde(default)]
    pub properties: PropertiesSection,
    #[serde(default)]
    pub plugins: PluginsSection,
    #[serde(default)]
    pub developer: DeveloperSection,
}

impl SoftwareDefinition {
    pub fn parse(toml_str: &str) -> Result<Self, String> {
        toml::from_str(toml_str).map_err(|e| format!("Failed to parse software.toml: {}", e))
    }

    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize software.toml: {}", e))
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        Self::parse(&content)
    }

    pub fn save_to_file<P: AsRef<Path>>(&self, path: P) -> Result<(), String> {
        let path = path.as_ref();
        let content = self.to_toml()?;
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        fs::write(path, content).map_err(|e| format!("Failed to write {}: {}", path.display(), e))
    }

    pub fn id(&self) -> &str {
        &self.software.id
    }

    pub fn name(&self) -> &str {
        &self.software.name
    }

    pub fn display_name(&self) -> &str {
        self.software
            .display_name
            .as_deref()
            .unwrap_or(&self.software.name)
    }

    pub fn game(&self) -> &str {
        &self.software.game
    }

    pub fn edition(&self) -> &str {
        &self.software.edition
    }

    pub fn description(&self) -> &str {
        self.software.description.as_deref().unwrap_or("")
    }

    pub fn default_server_file(&self) -> &str {
        self.runtime
            .default_server_file
            .as_deref()
            .unwrap_or("server.jar")
    }
}
