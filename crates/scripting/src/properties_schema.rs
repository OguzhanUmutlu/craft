use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PropertiesMeta {
    #[serde(default = "default_properties_file")]
    pub file: String,
    #[serde(default = "default_properties_format")]
    pub format: String,
}

fn default_properties_file() -> String {
    "server.properties".to_string()
}

fn default_properties_format() -> String {
    "properties".to_string()
}

impl Default for PropertiesMeta {
    fn default() -> Self {
        Self {
            file: default_properties_file(),
            format: default_properties_format(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CategoryDefinition {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum PropertyType {
    #[default]
    String,
    Integer,
    Boolean,
    Enum,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct PropertyDefinition {
    pub key: String,
    pub category: String,
    pub label: String,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(rename = "type", default)]
    pub property_type: PropertyType,
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    #[serde(default)]
    pub min: Option<i64>,
    #[serde(default)]
    pub max: Option<i64>,
    #[serde(default)]
    pub options: Option<Vec<String>>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct PropertiesSchema {
    #[serde(default)]
    pub meta: PropertiesMeta,
    #[serde(default)]
    pub categories: Vec<CategoryDefinition>,
    #[serde(default)]
    pub properties: Vec<PropertyDefinition>,
}

impl PropertiesSchema {
    pub fn parse(toml_str: &str) -> Result<Self, String> {
        toml::from_str(toml_str).map_err(|e| format!("Failed to parse properties.toml: {}", e))
    }

    pub fn to_toml(&self) -> Result<String, String> {
        toml::to_string_pretty(self)
            .map_err(|e| format!("Failed to serialize properties.toml: {}", e))
    }

    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Self, String> {
        let path = path.as_ref();
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read {}: {}", path.display(), e))?;
        Self::parse(&content)
    }

    pub fn get_property(&self, key: &str) -> Option<&PropertyDefinition> {
        self.properties.iter().find(|p| p.key == key)
    }

    pub fn properties_for_category(&self, cat_id: &str) -> Vec<&PropertyDefinition> {
        self.properties
            .iter()
            .filter(|p| p.category == cat_id)
            .collect()
    }
}

/// Generic key-value store for server properties
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct GenericProperties {
    pub entries: HashMap<String, String>,
    pub raw_lines: Vec<String>,
    pub format: String,
}

impl GenericProperties {
    pub fn new(format: &str) -> Self {
        Self {
            entries: HashMap::new(),
            raw_lines: Vec::new(),
            format: format.to_string(),
        }
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.entries.get(key).map(|s| s.as_str())
    }

    pub fn set<K: Into<String>, V: Into<String>>(&mut self, key: K, value: V) {
        let key_str = key.into();
        let val_str = value.into();
        self.entries.insert(key_str.clone(), val_str.clone());

        // Update raw_lines if format is properties/ini
        let mut found = false;
        let prefix = format!("{}=", key_str);
        for line in &mut self.raw_lines {
            let trimmed = line.trim();
            if trimmed.starts_with(&prefix) {
                *line = format!("{}={}", key_str, val_str);
                found = true;
                break;
            }
        }
        if !found && (self.format == "properties" || self.format == "ini") {
            self.raw_lines.push(format!("{}={}", key_str, val_str));
        }
    }

    pub fn parse_properties(content: &str) -> Self {
        let mut entries = HashMap::new();
        let mut raw_lines = Vec::new();

        for line in content.lines() {
            raw_lines.push(line.to_string());
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') || trimmed.starts_with('!') {
                continue;
            }
            if let Some((k, v)) = trimmed.split_once('=') {
                entries.insert(k.trim().to_string(), v.trim().to_string());
            } else if let Some((k, v)) = trimmed.split_once(':') {
                entries.insert(k.trim().to_string(), v.trim().to_string());
            }
        }

        Self {
            entries,
            raw_lines,
            format: "properties".to_string(),
        }
    }

    pub fn dump_properties(&self) -> String {
        if !self.raw_lines.is_empty() {
            let mut result = self.raw_lines.join("\n");
            if !result.ends_with('\n') {
                result.push('\n');
            }
            result
        } else {
            let mut out = String::new();
            let mut keys: Vec<&String> = self.entries.keys().collect();
            keys.sort();
            for k in keys {
                if let Some(v) = self.entries.get(k) {
                    out.push_str(&format!("{}={}\n", k, v));
                }
            }
            out
        }
    }

    pub fn load_file<P: AsRef<Path>>(path: P, format: &str) -> Result<Self, String> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(Self::new(format));
        }
        let content = fs::read_to_string(path)
            .map_err(|e| format!("Failed to read properties file {}: {}", path.display(), e))?;

        match format {
            "json" => {
                let value: serde_json::Value = serde_json::from_str(&content)
                    .map_err(|e| format!("Failed to parse JSON properties: {}", e))?;
                let mut entries = HashMap::new();
                if let serde_json::Value::Object(map) = value {
                    for (k, v) in map {
                        match v {
                            serde_json::Value::String(s) => {
                                entries.insert(k, s);
                            }
                            _ => {
                                entries.insert(k, v.to_string());
                            }
                        }
                    }
                }
                Ok(Self {
                    entries,
                    raw_lines: content.lines().map(|s| s.to_string()).collect(),
                    format: "json".to_string(),
                })
            }
            "toml" => {
                let table: toml::Table = toml::from_str(&content)
                    .map_err(|e| format!("Failed to parse TOML properties: {}", e))?;
                let mut entries = HashMap::new();
                for (k, v) in table {
                    match v {
                        toml::Value::String(s) => {
                            entries.insert(k, s);
                        }
                        _ => {
                            entries.insert(k, v.to_string());
                        }
                    }
                }
                Ok(Self {
                    entries,
                    raw_lines: content.lines().map(|s| s.to_string()).collect(),
                    format: "toml".to_string(),
                })
            }
            _ => Ok(Self::parse_properties(&content)),
        }
    }

    pub fn save_file<P: AsRef<Path>>(&self, path: P) -> Result<(), String> {
        let path = path.as_ref();
        if let Some(parent) = path.parent() {
            let _ = fs::create_dir_all(parent);
        }

        let content = match self.format.as_str() {
            "json" => {
                let mut map = serde_json::Map::new();
                for (k, v) in &self.entries {
                    if let Ok(b) = v.parse::<bool>() {
                        map.insert(k.clone(), serde_json::Value::Bool(b));
                    } else if let Ok(i) = v.parse::<i64>() {
                        map.insert(k.clone(), serde_json::Value::Number(i.into()));
                    } else {
                        map.insert(k.clone(), serde_json::Value::String(v.clone()));
                    }
                }
                serde_json::to_string_pretty(&map)
                    .map_err(|e| format!("Failed to serialize JSON properties: {}", e))?
            }
            "toml" => {
                let mut table = toml::Table::new();
                for (k, v) in &self.entries {
                    if let Ok(b) = v.parse::<bool>() {
                        table.insert(k.clone(), toml::Value::Boolean(b));
                    } else if let Ok(i) = v.parse::<i64>() {
                        table.insert(k.clone(), toml::Value::Integer(i));
                    } else {
                        table.insert(k.clone(), toml::Value::String(v.clone()));
                    }
                }
                toml::to_string_pretty(&table)
                    .map_err(|e| format!("Failed to serialize TOML properties: {}", e))?
            }
            _ => self.dump_properties(),
        };

        fs::write(path, content)
            .map_err(|e| format!("Failed to write properties file {}: {}", path.display(), e))
    }
}
