use serde_json::Value;
use std::collections::HashMap;

pub static BUNDLED_PAPER: &str = include_str!("../../../data/paper-versions.json");
pub static BUNDLED_PURPUR: &str = include_str!("../../../data/purpur-versions.json");
pub static BUNDLED_FOLIA: &str = include_str!("../../../data/folia-versions.json");
pub static BUNDLED_SPIGOT: &str = include_str!("../../../data/spigot-versions.json");
pub static BUNDLED_VANILLA_JAVA: &str = include_str!("../../../data/vanilla-java-versions.json");
pub static BUNDLED_BEDROCK_WIN: &str =
    include_str!("../../../data/vanilla-bedrock-windows-versions.json");
pub static BUNDLED_BEDROCK_LINUX: &str =
    include_str!("../../../data/vanilla-bedrock-linux-versions.json");
pub static BUNDLED_POCKETMINE: &str = include_str!("../../../data/pocketmine-versions.json");

/// Parses a simple key-value version mapping (excluding "latest")
pub fn parse_bundled_manifest(raw_json: &str) -> (Option<String>, HashMap<String, String>) {
    let parsed: Value = serde_json::from_str(raw_json).unwrap_or(Value::Null);
    let mut map = HashMap::new();
    let mut latest = None;

    if let Some(obj) = parsed.as_object() {
        for (k, v) in obj {
            if k == "latest" {
                if let Some(s) = v.as_str() {
                    latest = Some(s.to_string());
                }
            } else if let Some(s) = v.as_str() {
                map.insert(k.clone(), s.to_string());
            }
        }
    }

    (latest, map)
}
