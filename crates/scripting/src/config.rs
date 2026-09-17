use craft_core::{CraftError, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

pub const CUSTOM_CONFIG_FILE: &str = "craft.custom.toml";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum CustomRuntimeType {
    #[default]
    Binary,
    Lua,
    Script,
}

impl CustomRuntimeType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Binary => "binary",
            Self::Lua => "lua",
            Self::Script => "script",
        }
    }

    pub fn display_name(&self) -> &'static str {
        match self {
            Self::Binary => "Native Binary Executable",
            Self::Lua => "Lua Script Server",
            Self::Script => "Shell / Batch Script",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomServerSection {
    pub name: String,
    #[serde(default)]
    pub runtime_type: CustomRuntimeType,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomRuntimeSection {
    pub executable: String,
    #[serde(default)]
    pub arguments: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_dir: Option<String>,
    #[serde(default)]
    pub environment: HashMap<String, String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomNetworkSection {
    #[serde(default = "default_port")]
    pub port: u16,
    #[serde(default = "default_protocol")]
    pub protocol: String,
    #[serde(default = "default_query_protocol")]
    pub query_protocol: String,
}

fn default_port() -> u16 {
    8080
}

fn default_protocol() -> String {
    "tcp".to_string()
}

fn default_query_protocol() -> String {
    "port_probe".to_string()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomLifecycleSection {
    #[serde(default = "default_stop_method")]
    pub stop_method: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stop_command: Option<String>,
    #[serde(default = "default_stop_timeout")]
    pub stop_timeout_seconds: u64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub hooks: Option<String>,
}

fn default_stop_method() -> String {
    "stdin".to_string()
}

fn default_stop_timeout() -> u64 {
    10
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CustomServerConfig {
    pub server: CustomServerSection,
    pub runtime: CustomRuntimeSection,
    #[serde(default)]
    pub network: CustomNetworkSection,
    #[serde(default)]
    pub lifecycle: CustomLifecycleSection,
}

impl Default for CustomNetworkSection {
    fn default() -> Self {
        Self {
            port: default_port(),
            protocol: default_protocol(),
            query_protocol: default_query_protocol(),
        }
    }
}

impl Default for CustomLifecycleSection {
    fn default() -> Self {
        Self {
            stop_method: default_stop_method(),
            stop_command: Some("stop\n".to_string()),
            stop_timeout_seconds: default_stop_timeout(),
            hooks: Some("hooks.lua".to_string()),
        }
    }
}

impl CustomServerConfig {
    pub fn default_for(
        name: &str,
        runtime_type: CustomRuntimeType,
        port: u16,
        executable: Option<&str>,
    ) -> Self {
        let exec = match executable {
            Some(e) => e.to_string(),
            None => match runtime_type {
                CustomRuntimeType::Binary => {
                    if cfg!(windows) {
                        "server.exe".to_string()
                    } else {
                        "./server".to_string()
                    }
                }
                CustomRuntimeType::Lua => "server.lua".to_string(),
                CustomRuntimeType::Script => {
                    if cfg!(windows) {
                        "server.cmd".to_string()
                    } else {
                        "./server.sh".to_string()
                    }
                }
            },
        };

        let default_stop = match runtime_type {
            CustomRuntimeType::Binary => "stop\n",
            CustomRuntimeType::Lua => "stop\n",
            CustomRuntimeType::Script => "stop\n",
        };

        Self {
            server: CustomServerSection {
                name: name.to_string(),
                runtime_type,
                description: Some(format!(
                    "Custom game server ({})",
                    runtime_type.display_name()
                )),
            },
            runtime: CustomRuntimeSection {
                executable: exec,
                arguments: vec![],
                working_dir: None,
                environment: HashMap::new(),
            },
            network: CustomNetworkSection {
                port,
                protocol: "tcp".to_string(),
                query_protocol: "port_probe".to_string(),
            },
            lifecycle: CustomLifecycleSection {
                stop_method: "stdin".to_string(),
                stop_command: Some(default_stop.to_string()),
                stop_timeout_seconds: 10,
                hooks: Some("hooks.lua".to_string()),
            },
        }
    }

    pub fn default_lua(name: &str, port: u16) -> Self {
        Self::default_for(name, CustomRuntimeType::Lua, port, None)
    }

    pub fn default_binary(name: &str, port: u16, executable: &str) -> Self {
        Self::default_for(name, CustomRuntimeType::Binary, port, Some(executable))
    }

    pub fn default_script(name: &str, port: u16, script: &str) -> Self {
        Self::default_for(name, CustomRuntimeType::Script, port, Some(script))
    }

    pub fn load_from_dir(server_dir: &Path) -> Result<Option<Self>> {
        let path = server_dir.join(CUSTOM_CONFIG_FILE);
        if !path.exists() {
            return Ok(None);
        }
        let content = fs::read_to_string(&path).map_err(|e| {
            CraftError::Other(format!("Failed to read {}: {}", CUSTOM_CONFIG_FILE, e))
        })?;
        let config: Self = toml::from_str(&content).map_err(|e| {
            CraftError::Other(format!("Failed to parse {}: {}", CUSTOM_CONFIG_FILE, e))
        })?;
        Ok(Some(config))
    }

    pub fn save_to_dir(&self, server_dir: &Path) -> Result<()> {
        let path = server_dir.join(CUSTOM_CONFIG_FILE);
        let content = toml::to_string_pretty(self).map_err(|e| {
            CraftError::Other(format!("Failed to serialize {}: {}", CUSTOM_CONFIG_FILE, e))
        })?;
        fs::write(&path, content).map_err(|e| {
            CraftError::Other(format!("Failed to write {}: {}", CUSTOM_CONFIG_FILE, e))
        })?;
        Ok(())
    }

    pub fn resolve_executable(&self, server_dir: &Path) -> PathBuf {
        let exec_path = PathBuf::from(&self.runtime.executable);
        if exec_path.is_absolute() {
            exec_path
        } else {
            server_dir.join(exec_path)
        }
    }
}
