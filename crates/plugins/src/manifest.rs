use craft_core::{CraftError, Result};
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use zip::ZipArchive;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum JarManifestKind {
    BukkitPlugin,
    PaperPlugin,
    VelocityPlugin,
    BungeePlugin,
    FabricMod,
    ForgeMod,
    QuiltMod,
    Unknown,
}

impl std::fmt::Display for JarManifestKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JarManifestKind::BukkitPlugin => write!(f, "Bukkit/Spigot Plugin"),
            JarManifestKind::PaperPlugin => write!(f, "Paper Plugin"),
            JarManifestKind::VelocityPlugin => write!(f, "Velocity Proxy Plugin"),
            JarManifestKind::BungeePlugin => write!(f, "BungeeCord Proxy Plugin"),
            JarManifestKind::FabricMod => write!(f, "Fabric Mod"),
            JarManifestKind::ForgeMod => write!(f, "Forge / NeoForge Mod"),
            JarManifestKind::QuiltMod => write!(f, "Quilt Mod"),
            JarManifestKind::Unknown => write!(f, "Unknown JAR Archive"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyRequirement {
    pub name_or_id: String,
    pub required: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version_range: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct JarManifestInfo {
    pub id_or_name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub display_name: Option<String>,
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    pub kind: JarManifestKind,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub dependencies: Vec<DependencyRequirement>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub authors: Vec<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_class: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
}

/// Inspects a `.jar` archive and parses its bytecode metadata manifest.
pub fn inspect_jar_manifest(jar_path: &Path) -> Result<JarManifestInfo> {
    if !jar_path.exists() {
        return Err(CraftError::InvalidPath(format!(
            "JAR file does not exist: {}",
            jar_path.display()
        )));
    }

    let file = File::open(jar_path)?;
    let mut archive = ZipArchive::new(file)
        .map_err(|e| CraftError::Other(format!("Failed to read JAR archive {}: {}", jar_path.display(), e)))?;

    // 1. Paper plugin manifest
    if let Ok(mut entry) = archive.by_name("paper-plugin.yml") {
        let mut content = String::new();
        if entry.read_to_string(&mut content).is_ok() {
            if let Some(info) = parse_bukkit_plugin_yml(&content, true) {
                return Ok(info);
            }
        }
    }

    // 2. Standard Bukkit/Spigot plugin manifest
    if let Ok(mut entry) = archive.by_name("plugin.yml") {
        let mut content = String::new();
        if entry.read_to_string(&mut content).is_ok() {
            if let Some(info) = parse_bukkit_plugin_yml(&content, false) {
                return Ok(info);
            }
        }
    }

    // 3. Fabric mod manifest
    if let Ok(mut entry) = archive.by_name("fabric.mod.json") {
        let mut content = String::new();
        if entry.read_to_string(&mut content).is_ok() {
            if let Some(info) = parse_fabric_mod_json(&content) {
                return Ok(info);
            }
        }
    }

    // 4. Quilt mod manifest
    if let Ok(mut entry) = archive.by_name("quilt.mod.json") {
        let mut content = String::new();
        if entry.read_to_string(&mut content).is_ok() {
            if let Some(info) = parse_quilt_mod_json(&content) {
                return Ok(info);
            }
        }
    }

    // 5. NeoForge or Forge mods.toml
    for name in &["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
        if let Ok(mut entry) = archive.by_name(name) {
            let mut content = String::new();
            if entry.read_to_string(&mut content).is_ok() {
                if let Some(info) = parse_forge_mods_toml(&content) {
                    return Ok(info);
                }
            }
        }
    }

    // 6. Velocity plugin manifest
    if let Ok(mut entry) = archive.by_name("velocity-plugin.json") {
        let mut content = String::new();
        if entry.read_to_string(&mut content).is_ok() {
            if let Some(info) = parse_velocity_plugin_json(&content) {
                return Ok(info);
            }
        }
    }

    // 7. BungeeCord plugin manifest
    if let Ok(mut entry) = archive.by_name("bungee.yml") {
        let mut content = String::new();
        if entry.read_to_string(&mut content).is_ok() {
            if let Some(info) = parse_bungee_yml(&content) {
                return Ok(info);
            }
        }
    }

    // Fallback: heuristic from filename
    let filename = jar_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown.jar");
    Ok(fallback_from_filename(filename))
}

/// Robust YAML parser for Bukkit / Spigot / Paper manifests
pub fn parse_bukkit_plugin_yml(content: &str, is_paper: bool) -> Option<JarManifestInfo> {
    let mut name = None;
    let mut version = None;
    let mut api_version = None;
    let mut description = None;
    let mut main_class = None;
    let mut website = None;
    let mut authors = Vec::new();
    let mut dependencies = Vec::new();

    let lines: Vec<&str> = content.lines().collect();
    let mut idx = 0;

    while idx < lines.len() {
        let line = lines[idx].trim();
        idx += 1;

        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        if let Some((k, v)) = line.split_once(':') {
            let key = k.trim().to_lowercase();
            let val = v.trim();

            match key.as_str() {
                "name" => name = Some(clean_val(val)),
                "version" => version = Some(clean_val(val)),
                "api-version" => api_version = Some(clean_val(val)),
                "description" => description = Some(clean_val(val)),
                "main" => main_class = Some(clean_val(val)),
                "website" => website = Some(clean_val(val)),
                "author" => {
                    let a = clean_val(val);
                    if !a.is_empty() {
                        authors.push(a);
                    }
                }
                "authors" => {
                    if val.starts_with('[') && val.ends_with(']') {
                        authors.extend(parse_inline_array(val));
                    } else {
                        // Multi-line list
                        while idx < lines.len() {
                            let next = lines[idx].trim();
                            if next.starts_with('-') {
                                authors.push(clean_val(next.trim_start_matches('-').trim()));
                                idx += 1;
                            } else {
                                break;
                            }
                        }
                    }
                }
                "depend" | "dependencies" => {
                    let deps = if val.starts_with('[') && val.ends_with(']') {
                        parse_inline_array(val)
                    } else {
                        let mut list = Vec::new();
                        while idx < lines.len() {
                            let next = lines[idx].trim();
                            if next.starts_with('-') {
                                list.push(clean_val(next.trim_start_matches('-').trim()));
                                idx += 1;
                            } else {
                                break;
                            }
                        }
                        list
                    };
                    for d in deps {
                        if !d.is_empty() {
                            dependencies.push(DependencyRequirement {
                                name_or_id: d,
                                required: true,
                                version_range: None,
                            });
                        }
                    }
                }
                "softdepend" | "soft-dependencies" => {
                    let deps = if val.starts_with('[') && val.ends_with(']') {
                        parse_inline_array(val)
                    } else {
                        let mut list = Vec::new();
                        while idx < lines.len() {
                            let next = lines[idx].trim();
                            if next.starts_with('-') {
                                list.push(clean_val(next.trim_start_matches('-').trim()));
                                idx += 1;
                            } else {
                                break;
                            }
                        }
                        list
                    };
                    for d in deps {
                        if !d.is_empty() {
                            dependencies.push(DependencyRequirement {
                                name_or_id: d,
                                required: false,
                                version_range: None,
                            });
                        }
                    }
                }
                _ => {}
            }
        }
    }

    let id_or_name = name?;
    Some(JarManifestInfo {
        id_or_name: id_or_name.clone(),
        display_name: Some(id_or_name),
        version: version.unwrap_or_else(|| "1.0.0".to_string()),
        api_version,
        description,
        kind: if is_paper {
            JarManifestKind::PaperPlugin
        } else {
            JarManifestKind::BukkitPlugin
        },
        dependencies,
        authors,
        main_class,
        website,
    })
}

/// Parses Fabric `fabric.mod.json`
pub fn parse_fabric_mod_json(content: &str) -> Option<JarManifestInfo> {
    let val: serde_json::Value = serde_json::from_str(content).ok()?;
    let id = val.get("id").and_then(|v| v.as_str())?.to_string();
    let display_name = val.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
    let version = val.get("version").and_then(|v| v.as_str()).unwrap_or("1.0.0").to_string();
    let description = val.get("description").and_then(|v| v.as_str()).map(|s| s.to_string());

    let mut authors = Vec::new();
    if let Some(arr) = val.get("authors").and_then(|v| v.as_array()) {
        for a in arr {
            if let Some(s) = a.as_str() {
                authors.push(s.to_string());
            } else if let Some(name) = a.get("name").and_then(|n| n.as_str()) {
                authors.push(name.to_string());
            }
        }
    }

    let mut dependencies = Vec::new();
    if let Some(deps_map) = val.get("depends").and_then(|v| v.as_object()) {
        for (k, v) in deps_map {
            let ver = v.as_str().map(|s| s.to_string());
            dependencies.push(DependencyRequirement {
                name_or_id: k.clone(),
                required: true,
                version_range: ver,
            });
        }
    }
    if let Some(recs_map) = val.get("recommends").and_then(|v| v.as_object()) {
        for (k, v) in recs_map {
            let ver = v.as_str().map(|s| s.to_string());
            dependencies.push(DependencyRequirement {
                name_or_id: k.clone(),
                required: false,
                version_range: ver,
            });
        }
    }

    Some(JarManifestInfo {
        id_or_name: id,
        display_name,
        version,
        api_version: None,
        description,
        kind: JarManifestKind::FabricMod,
        dependencies,
        authors,
        main_class: None,
        website: None,
    })
}

/// Parses Quilt `quilt.mod.json`
pub fn parse_quilt_mod_json(content: &str) -> Option<JarManifestInfo> {
    let val: serde_json::Value = serde_json::from_str(content).ok()?;
    let loader = val.get("quilt_loader")?;
    let id = loader.get("id").and_then(|v| v.as_str())?.to_string();
    let version = loader.get("version").and_then(|v| v.as_str()).unwrap_or("1.0.0").to_string();

    let meta = loader.get("metadata");
    let display_name = meta.and_then(|m| m.get("name")).and_then(|n| n.as_str()).map(|s| s.to_string());
    let description = meta.and_then(|m| m.get("description")).and_then(|n| n.as_str()).map(|s| s.to_string());

    let mut dependencies = Vec::new();
    if let Some(arr) = loader.get("depends").and_then(|v| v.as_array()) {
        for d in arr {
            if let Some(s) = d.as_str() {
                dependencies.push(DependencyRequirement {
                    name_or_id: s.to_string(),
                    required: true,
                    version_range: None,
                });
            } else if let Some(dep_id) = d.get("id").and_then(|i| i.as_str()) {
                let ver = d.get("versions").and_then(|v| v.as_str()).map(|s| s.to_string());
                let optional = d.get("optional").and_then(|o| o.as_bool()).unwrap_or(false);
                dependencies.push(DependencyRequirement {
                    name_or_id: dep_id.to_string(),
                    required: !optional,
                    version_range: ver,
                });
            }
        }
    }

    Some(JarManifestInfo {
        id_or_name: id,
        display_name,
        version,
        api_version: None,
        description,
        kind: JarManifestKind::QuiltMod,
        dependencies,
        authors: Vec::new(),
        main_class: None,
        website: None,
    })
}

/// Parses Forge / NeoForge `mods.toml`
pub fn parse_forge_mods_toml(content: &str) -> Option<JarManifestInfo> {
    let val: toml::Value = toml::from_str(content).ok()?;
    let mods_arr = val.get("mods").and_then(|v| v.as_array())?;
    let first_mod = mods_arr.first()?.as_table()?;

    let mod_id = first_mod.get("modId").and_then(|v| v.as_str())?.to_string();
    let display_name = first_mod.get("displayName").and_then(|v| v.as_str()).map(|s| s.to_string());
    let version = first_mod.get("version").and_then(|v| v.as_str()).unwrap_or("1.0.0").to_string();
    let description = first_mod.get("description").and_then(|v| v.as_str()).map(|s| s.to_string());
    let authors = first_mod
        .get("authors")
        .and_then(|v| v.as_str())
        .map(|s| s.split(',').map(|a| a.trim().to_string()).collect())
        .unwrap_or_default();

    let mut dependencies = Vec::new();
    if let Some(deps_table) = val.get("dependencies").and_then(|v| v.as_table()) {
        for (_k, mod_deps) in deps_table {
            if let Some(deps_arr) = mod_deps.as_array() {
                for dep in deps_arr {
                    if let Some(dep_tbl) = dep.as_table() {
                        if let Some(target_id) = dep_tbl.get("modId").and_then(|m| m.as_str()) {
                            let mandatory = dep_tbl
                                .get("mandatory")
                                .and_then(|m| m.as_bool())
                                .or_else(|| {
                                    dep_tbl
                                        .get("type")
                                        .and_then(|t| t.as_str())
                                        .map(|t| t.eq_ignore_ascii_case("required"))
                                })
                                .unwrap_or(true);
                            let ver = dep_tbl
                                .get("versionRange")
                                .and_then(|v| v.as_str())
                                .map(|s| s.to_string());

                            dependencies.push(DependencyRequirement {
                                name_or_id: target_id.to_string(),
                                required: mandatory,
                                version_range: ver,
                            });
                        }
                    }
                }
            }
        }
    }

    Some(JarManifestInfo {
        id_or_name: mod_id,
        display_name,
        version,
        api_version: None,
        description,
        kind: JarManifestKind::ForgeMod,
        dependencies,
        authors,
        main_class: None,
        website: None,
    })
}

/// Parses Velocity `velocity-plugin.json`
pub fn parse_velocity_plugin_json(content: &str) -> Option<JarManifestInfo> {
    let val: serde_json::Value = serde_json::from_str(content).ok()?;
    let id = val.get("id").and_then(|v| v.as_str())?.to_string();
    let display_name = val.get("name").and_then(|v| v.as_str()).map(|s| s.to_string());
    let version = val.get("version").and_then(|v| v.as_str()).unwrap_or("1.0.0").to_string();
    let description = val.get("description").and_then(|v| v.as_str()).map(|s| s.to_string());

    let mut authors = Vec::new();
    if let Some(arr) = val.get("authors").and_then(|v| v.as_array()) {
        for a in arr {
            if let Some(s) = a.as_str() {
                authors.push(s.to_string());
            }
        }
    }

    let mut dependencies = Vec::new();
    if let Some(arr) = val.get("dependencies").and_then(|v| v.as_array()) {
        for d in arr {
            if let Some(dep_id) = d.get("id").and_then(|i| i.as_str()) {
                let optional = d.get("optional").and_then(|o| o.as_bool()).unwrap_or(false);
                dependencies.push(DependencyRequirement {
                    name_or_id: dep_id.to_string(),
                    required: !optional,
                    version_range: None,
                });
            }
        }
    }

    Some(JarManifestInfo {
        id_or_name: id,
        display_name,
        version,
        api_version: None,
        description,
        kind: JarManifestKind::VelocityPlugin,
        dependencies,
        authors,
        main_class: None,
        website: None,
    })
}

/// Parses BungeeCord `bungee.yml`
pub fn parse_bungee_yml(content: &str) -> Option<JarManifestInfo> {
    let mut name = None;
    let mut version = None;
    let mut description = None;
    let mut main_class = None;
    let mut authors = Vec::new();
    let mut dependencies = Vec::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = trimmed.split_once(':') {
            let key = k.trim().to_lowercase();
            let val = v.trim();
            match key.as_str() {
                "name" => name = Some(clean_val(val)),
                "version" => version = Some(clean_val(val)),
                "description" => description = Some(clean_val(val)),
                "main" => main_class = Some(clean_val(val)),
                "author" => authors.push(clean_val(val)),
                "depends" => {
                    for d in parse_inline_array(val) {
                        dependencies.push(DependencyRequirement {
                            name_or_id: d,
                            required: true,
                            version_range: None,
                        });
                    }
                }
                "softdepends" => {
                    for d in parse_inline_array(val) {
                        dependencies.push(DependencyRequirement {
                            name_or_id: d,
                            required: false,
                            version_range: None,
                        });
                    }
                }
                _ => {}
            }
        }
    }

    let id = name?;
    Some(JarManifestInfo {
        id_or_name: id.clone(),
        display_name: Some(id),
        version: version.unwrap_or_else(|| "1.0.0".to_string()),
        api_version: None,
        description,
        kind: JarManifestKind::BungeePlugin,
        dependencies,
        authors,
        main_class,
        website: None,
    })
}

fn fallback_from_filename(filename: &str) -> JarManifestInfo {
    let base = filename.strip_suffix(".jar").unwrap_or(filename);
    let (name, ver) = if let Some((n, v)) = base.split_once('-') {
        (n, v)
    } else {
        (base, "1.0.0")
    };

    JarManifestInfo {
        id_or_name: name.to_string(),
        display_name: Some(name.to_string()),
        version: ver.to_string(),
        api_version: None,
        description: None,
        kind: JarManifestKind::Unknown,
        dependencies: Vec::new(),
        authors: Vec::new(),
        main_class: None,
        website: None,
    }
}

fn clean_val(val: &str) -> String {
    val.trim_matches(|c| c == '\'' || c == '"' || c == ' ')
        .to_string()
}

fn parse_inline_array(val: &str) -> Vec<String> {
    let inner = val.trim_start_matches('[').trim_end_matches(']');
    inner
        .split(',')
        .map(|s| clean_val(s))
        .filter(|s| !s.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_bukkit_plugin_yml() {
        let yaml = r#"
name: EssentialsX
version: 2.20.1
main: net.essentialsx.Essentials
api-version: '1.13'
description: The modern Essentials suite
author: EssentialsX Team
depend: [Vault]
softdepend:
  - LuckPerms
  - ProtocolLib
"#;
        let info = parse_bukkit_plugin_yml(yaml, false).unwrap();
        assert_eq!(info.id_or_name, "EssentialsX");
        assert_eq!(info.version, "2.20.1");
        assert_eq!(info.api_version.as_deref(), Some("1.13"));
        assert_eq!(info.kind, JarManifestKind::BukkitPlugin);
        assert_eq!(info.authors, vec!["EssentialsX Team"]);
        assert_eq!(info.dependencies.len(), 3);
        assert!(info.dependencies.iter().any(|d| d.name_or_id == "Vault" && d.required));
        assert!(info.dependencies.iter().any(|d| d.name_or_id == "LuckPerms" && !d.required));
        assert!(info.dependencies.iter().any(|d| d.name_or_id == "ProtocolLib" && !d.required));
    }

    #[test]
    fn test_parse_fabric_mod_json() {
        let json = r#"
{
  "schemaVersion": 1,
  "id": "fabric-api",
  "version": "0.100.0+1.21",
  "name": "Fabric API",
  "description": "Core API library for Fabric mods",
  "authors": ["FabricMC"],
  "depends": {
    "fabricloader": ">=0.15.0",
    "minecraft": "~1.21"
  }
}
"#;
        let info = parse_fabric_mod_json(json).unwrap();
        assert_eq!(info.id_or_name, "fabric-api");
        assert_eq!(info.display_name.as_deref(), Some("Fabric API"));
        assert_eq!(info.version, "0.100.0+1.21");
        assert_eq!(info.kind, JarManifestKind::FabricMod);
        assert_eq!(info.dependencies.len(), 2);
        assert!(info.dependencies.iter().any(|d| d.name_or_id == "minecraft" && d.version_range.as_deref() == Some("~1.21")));
    }

    #[test]
    fn test_parse_forge_mods_toml() {
        let toml_str = r#"
modLoader="javafml"
loaderVersion="[51,)"

[[mods]]
modId="jei"
version="19.0.0.1"
displayName="Just Enough Items"
authors="mezz"

[[dependencies.jei]]
modId="forge"
type="required"
versionRange="[51.0.0,)"
"#;
        let info = parse_forge_mods_toml(toml_str).unwrap();
        assert_eq!(info.id_or_name, "jei");
        assert_eq!(info.display_name.as_deref(), Some("Just Enough Items"));
        assert_eq!(info.version, "19.0.0.1");
        assert_eq!(info.kind, JarManifestKind::ForgeMod);
        assert_eq!(info.dependencies.len(), 1);
        assert_eq!(info.dependencies[0].name_or_id, "forge");
        assert!(info.dependencies[0].required);
    }
}
