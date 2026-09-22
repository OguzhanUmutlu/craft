use crate::manifest::{JarManifestInfo, JarManifestKind};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompatibilityReport {
    pub is_compatible: bool,
    pub server_version: String,
    pub server_software: String,
    pub required_api: Option<String>,
    pub loader: String,
    pub warnings: Vec<String>,
    pub errors: Vec<String>,
}

pub fn evaluate_compatibility(
    manifest: &JarManifestInfo,
    server_version: &str,
    server_software: &str,
) -> CompatibilityReport {
    let mut warnings = Vec::new();
    let mut errors = Vec::new();

    let sw_lower = server_software.to_lowercase();
    let server_ver = server_version.trim();

    // 1. Loader & Ecosystem Validation
    match manifest.kind {
        JarManifestKind::FabricMod => {
            if !sw_lower.contains("fabric") && !sw_lower.contains("quilt") {
                errors.push(format!(
                    "Incompatible platform: '{}' is a Fabric mod, but server software is '{}'. Fabric mods require a Fabric or Quilt server runtime.",
                    manifest.id_or_name, server_software
                ));
            }
        }
        JarManifestKind::QuiltMod => {
            if !sw_lower.contains("quilt") && !sw_lower.contains("fabric") {
                errors.push(format!(
                    "Incompatible platform: '{}' is a Quilt mod, but server software is '{}'.",
                    manifest.id_or_name, server_software
                ));
            }
        }
        JarManifestKind::ForgeMod => {
            if !sw_lower.contains("forge") && !sw_lower.contains("neoforge") {
                errors.push(format!(
                    "Incompatible platform: '{}' is a Forge/NeoForge mod, but server software is '{}'. Forge mods require Forge or NeoForge.",
                    manifest.id_or_name, server_software
                ));
            }
        }
        JarManifestKind::VelocityPlugin => {
            if !sw_lower.contains("velocity") {
                errors.push(format!(
                    "Incompatible platform: '{}' is a Velocity proxy plugin, but server software is '{}'.",
                    manifest.id_or_name, server_software
                ));
            }
        }
        JarManifestKind::BungeePlugin => {
            if !sw_lower.contains("bungee") && !sw_lower.contains("waterfall") && !sw_lower.contains("waterdog") {
                warnings.push(format!(
                    "Platform warning: '{}' is designed for BungeeCord/Waterfall proxies, but current server is '{}'.",
                    manifest.id_or_name, server_software
                ));
            }
        }
        JarManifestKind::BukkitPlugin | JarManifestKind::PaperPlugin => {
            if sw_lower == "vanilla" || sw_lower == "fabric" || sw_lower == "forge" || sw_lower == "neoforge" {
                errors.push(format!(
                    "Incompatible platform: '{}' is a Bukkit/Paper plugin, but server software is '{}' which does not support Bukkit plugins.",
                    manifest.id_or_name, server_software
                ));
            }
        }
        JarManifestKind::Unknown => {
            warnings.push(format!(
                "Unknown manifest format for '{}'. Verification could not determine required platform loader.",
                manifest.id_or_name
            ));
        }
    }

    // 2. Minecraft / API Version Validation
    if let Some(ref api) = manifest.api_version {
        if let Some(cmp) = compare_simple_versions(api, server_ver) {
            if cmp > 0 {
                warnings.push(format!(
                    "API Version Warning: Plugin requires API version {} or higher, but server is running Minecraft {}.",
                    api, server_ver
                ));
            }
        }
    }

    // 3. Minecraft Version Bounds in Mod Dependencies
    for dep in &manifest.dependencies {
        if dep.name_or_id.eq_ignore_ascii_case("minecraft") {
            if let Some(ref range) = dep.version_range {
                if !matches_version_range(server_ver, range) {
                    warnings.push(format!(
                        "Target Minecraft version '{}' may not satisfy mod requirement '{}'.",
                        server_ver, range
                    ));
                }
            }
        }
    }

    let is_compatible = errors.is_empty();

    CompatibilityReport {
        is_compatible,
        server_version: server_ver.to_string(),
        server_software: server_software.to_string(),
        required_api: manifest.api_version.clone(),
        loader: manifest.kind.to_string(),
        warnings,
        errors,
    }
}

fn compare_simple_versions(a: &str, b: &str) -> Option<i32> {
    let parse_nums = |s: &str| -> Vec<u32> {
        s.split('.')
            .filter_map(|part| part.trim_matches(|c: char| !c.is_ascii_digit()).parse::<u32>().ok())
            .collect()
    };

    let va = parse_nums(a);
    let vb = parse_nums(b);

    if va.is_empty() || vb.is_empty() {
        return None;
    }

    for (na, nb) in va.iter().zip(vb.iter()) {
        if na != nb {
            return Some(if na > nb { 1 } else { -1 });
        }
    }

    Some(if va.len() > vb.len() {
        1
    } else if va.len() < vb.len() {
        -1
    } else {
        0
    })
}

fn matches_version_range(version: &str, range: &str) -> bool {
    let r = range.trim();
    if r == "*" || r.is_empty() {
        return true;
    }
    if let Some(target) = r.strip_prefix(">=") {
        return compare_simple_versions(version, target.trim()).unwrap_or(0) >= 0;
    }
    if let Some(target) = r.strip_prefix('~') {
        let clean_target = target.trim();
        let target_prefix = clean_target.split('.').take(2).collect::<Vec<_>>().join(".");
        return version.starts_with(&target_prefix);
    }
    if let Some(target) = r.strip_prefix('=') {
        return version == target.trim();
    }
    // Simple substring match fallback
    version.contains(r)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_fabric_mod_incompatible_on_paper() {
        let manifest = JarManifestInfo {
            id_or_name: "sodium".to_string(),
            display_name: Some("Sodium".to_string()),
            version: "0.5.8".to_string(),
            api_version: None,
            description: None,
            kind: JarManifestKind::FabricMod,
            dependencies: vec![],
            authors: vec![],
            main_class: None,
            website: None,
        };

        let report = evaluate_compatibility(&manifest, "1.21.1", "paper");
        assert!(!report.is_compatible);
        assert!(!report.errors.is_empty());
        assert!(report.errors[0].contains("Fabric mod"));
    }

    #[test]
    fn test_paper_plugin_compatible_on_purpur() {
        let manifest = JarManifestInfo {
            id_or_name: "WorldGuard".to_string(),
            display_name: Some("WorldGuard".to_string()),
            version: "7.0.9".to_string(),
            api_version: Some("1.20".to_string()),
            description: None,
            kind: JarManifestKind::PaperPlugin,
            dependencies: vec![],
            authors: vec![],
            main_class: None,
            website: None,
        };

        let report = evaluate_compatibility(&manifest, "1.20.4", "purpur");
        assert!(report.is_compatible);
        assert!(report.errors.is_empty());
    }

    #[test]
    fn test_api_version_higher_than_server() {
        let manifest = JarManifestInfo {
            id_or_name: "NextGenPlugin".to_string(),
            display_name: Some("NextGenPlugin".to_string()),
            version: "1.0.0".to_string(),
            api_version: Some("1.21".to_string()),
            description: None,
            kind: JarManifestKind::BukkitPlugin,
            dependencies: vec![],
            authors: vec![],
            main_class: None,
            website: None,
        };

        let report = evaluate_compatibility(&manifest, "1.20.4", "paper");
        assert!(report.is_compatible); // Not hard error, but emits warning
        assert!(!report.warnings.is_empty());
        assert!(report.warnings[0].contains("API Version Warning"));
    }
}
