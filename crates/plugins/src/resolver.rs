use crate::manifest::{inspect_jar_manifest, JarManifestInfo};
use crate::modrinth::ModrinthClient;
use craft_core::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ResolvedDependency {
    pub name: String,
    pub source: String,
    pub project_id: String,
    pub download_url: String,
    pub filename: String,
    pub required_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DependencyResolutionResult {
    pub resolved: Vec<ResolvedDependency>,
    pub missing: Vec<String>,
    pub already_installed: Vec<String>,
}

/// Resolves known ecosystem plugin and mod aliases to upstream project identifiers
pub fn resolve_ecosystem_alias(name: &str) -> &'static str {
    match name.to_lowercase().trim() {
        "vault" => "vault",
        "protocollib" => "protocollib",
        "luckperms" => "luckperms",
        "worldedit" => "worldedit",
        "worldguard" => "worldguard",
        "placeholderapi" | "papi" => "placeholderapi",
        "citizens" | "citizens2" => "citizens2",
        "multiverse-core" | "multiverse" => "multiverse-core",
        "essentials" | "essentialsx" => "essentialsx",
        "dynmap" => "dynmap",
        "coreprotect" => "coreprotect",
        "viaversion" => "viaversion",
        "viabackwards" => "viabackwards",
        "geyser" | "geyser-spigot" => "geyser",
        "floodgate" => "floodgate",
        "discordsrv" => "discordsrv",
        "chunky" => "chunky",
        "spark" => "spark",
        "fabric-api" | "fabric" => "fabric-api",
        "fabricloader" => "fabric-api",
        "cloth-config" | "cloth-config2" => "cloth-config",
        "architectury" | "architectury-api" => "architectury-api",
        "modmenu" => "modmenu",
        "sodium" => "sodium",
        "lithium" => "lithium",
        "ferritecore" => "ferrite-core",
        _ => "",
    }
}

/// Scans installed JARs in the server's plugins or mods directory and resolves unsatisfied dependencies
pub async fn resolve_missing_dependencies(
    server_path: &Path,
    manifest: &JarManifestInfo,
    server_version: Option<&str>,
    loader: Option<&str>,
) -> Result<DependencyResolutionResult> {
    let subfolder = if manifest.kind == crate::manifest::JarManifestKind::FabricMod
        || manifest.kind == crate::manifest::JarManifestKind::ForgeMod
        || manifest.kind == crate::manifest::JarManifestKind::QuiltMod
    {
        "mods"
    } else {
        "plugins"
    };

    let target_dir = server_path.join(subfolder);
    let mut installed_names: HashSet<String> = HashSet::new();

    if target_dir.exists() {
        if let Ok(entries) = fs::read_dir(&target_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("jar") {
                    if let Ok(inst_manifest) = inspect_jar_manifest(&p) {
                        installed_names.insert(inst_manifest.id_or_name.to_lowercase());
                        if let Some(dn) = inst_manifest.display_name {
                            installed_names.insert(dn.to_lowercase());
                        }
                    } else if let Some(fname) = p.file_name().and_then(|n| n.to_str()) {
                        let base = fname.strip_suffix(".jar").unwrap_or(fname);
                        if let Some((n, _)) = base.split_once('-') {
                            installed_names.insert(n.to_lowercase());
                        } else {
                            installed_names.insert(base.to_lowercase());
                        }
                    }
                }
            }
        }
    }

    let mut already_installed = Vec::new();
    let mut resolved = Vec::new();
    let mut missing = Vec::new();

    let client = ModrinthClient::new();
    let loaders: Vec<&str> = loader.into_iter().collect();
    let gvs: Vec<&str> = server_version.into_iter().collect();

    for dep in &manifest.dependencies {
        let dep_name = &dep.name_or_id;
        let lower = dep_name.to_lowercase();

        // Skip internal Minecraft/runtime anchors
        if lower == "minecraft" || lower == "java" || lower == "forge" || lower == "neoforge" {
            continue;
        }

        if installed_names.contains(&lower) {
            already_installed.push(dep_name.clone());
            continue;
        }

        if !dep.required {
            continue;
        }

        // Try alias lookup or direct query
        let query_term = {
            let alias = resolve_ecosystem_alias(dep_name);
            if !alias.is_empty() {
                alias
            } else {
                dep_name.as_str()
            }
        };

        match client.get_latest_compatible_file(query_term, &loaders, &gvs).await {
            Ok(file) => {
                resolved.push(ResolvedDependency {
                    name: dep_name.clone(),
                    source: "Modrinth".to_string(),
                    project_id: query_term.to_string(),
                    download_url: file.url,
                    filename: file.filename,
                    required_by: manifest.id_or_name.clone(),
                });
            }
            Err(_) => {
                // Try searching Modrinth if direct slug fetch missed
                if let Ok(hits) = client.search(query_term, None).await {
                    if let Some(first) = hits.first() {
                        if let Ok(file) = client.get_latest_compatible_file(&first.project_id, &loaders, &gvs).await {
                            resolved.push(ResolvedDependency {
                                name: dep_name.clone(),
                                source: "Modrinth".to_string(),
                                project_id: first.project_id.clone(),
                                download_url: file.url,
                                filename: file.filename,
                                required_by: manifest.id_or_name.clone(),
                            });
                            continue;
                        }
                    }
                }
                missing.push(dep_name.clone());
            }
        }
    }

    Ok(DependencyResolutionResult {
        resolved,
        missing,
        already_installed,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ecosystem_alias_mapping() {
        assert_eq!(resolve_ecosystem_alias("Vault"), "vault");
        assert_eq!(resolve_ecosystem_alias("ProtocolLib"), "protocollib");
        assert_eq!(resolve_ecosystem_alias("LuckPerms"), "luckperms");
        assert_eq!(resolve_ecosystem_alias("Fabric-API"), "fabric-api");
        assert_eq!(resolve_ecosystem_alias("unknown_custom_plugin"), "");
    }
}
