use crate::binary_delta::BinaryDeltaEngine;
use crate::manifest::inspect_jar_manifest;
use crate::update::compute_file_sha512;
use craft_core::{
    CraftError, ModSide, ModpackBuildManifest, ModpackComponent, Result,
};
use std::collections::{HashMap, HashSet};
use std::fs::{self, File};
use std::io::Read;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use zip::ZipArchive;

/// Detects the intended deployment side for a Minecraft mod JAR
pub fn detect_mod_side(jar_path: &Path) -> ModSide {
    let lower_name = jar_path
        .file_name()
        .map(|f| f.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    // 1. Inspect archive metadata if valid ZIP/JAR
    if let Ok(file) = File::open(jar_path) {
        if let Ok(mut archive) = ZipArchive::new(file) {
            // Check fabric.mod.json
            if let Ok(mut entry) = archive.by_name("fabric.mod.json") {
                let mut content = String::new();
                if entry.read_to_string(&mut content).is_ok() {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(env) = v.get("environment").and_then(|e| e.as_str()) {
                            match env {
                                "client" => return ModSide::ClientOnly,
                                "server" => return ModSide::ServerOnly,
                                "*" | "both" => return ModSide::Both,
                                _ => {}
                            }
                        }
                    }
                }
            }

            // Check quilt.mod.json
            if let Ok(mut entry) = archive.by_name("quilt.mod.json") {
                let mut content = String::new();
                if entry.read_to_string(&mut content).is_ok() {
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&content) {
                        if let Some(env) = v
                            .get("minecraft")
                            .and_then(|m| m.get("environment"))
                            .and_then(|e| e.as_str())
                        {
                            match env {
                                "client" => return ModSide::ClientOnly,
                                "server" => return ModSide::ServerOnly,
                                _ => {}
                            }
                        }
                    }
                }
            }

            // Check NeoForge / Forge mods.toml
            for name in &["META-INF/neoforge.mods.toml", "META-INF/mods.toml"] {
                if let Ok(mut entry) = archive.by_name(name) {
                    let mut content = String::new();
                    if entry.read_to_string(&mut content).is_ok() {
                        let upper = content.to_uppercase();
                        if upper.contains("SIDE=\"CLIENT\"") || upper.contains("SIDE = \"CLIENT\"") {
                            return ModSide::ClientOnly;
                        }
                        if upper.contains("SIDE=\"SERVER\"") || upper.contains("SIDE = \"SERVER\"") {
                            return ModSide::ServerOnly;
                        }
                    }
                }
            }
        }
    }

    // 2. Client-only keyword heuristics
    const CLIENT_PATTERNS: &[&str] = &[
        "iris", "sodium", "optifine", "rubidium", "embeddium", "oculus",
        "modmenu", "appleskin", "journeymap", "xaero", "voxelmap",
        "wthit", "jade", "jei", "rei", "emi", "dynamiclights", "lambdynamiclights",
        "entityculling", "ferritecore", "controlling", "soundphysics", "presencefootsteps",
    ];

    for pat in CLIENT_PATTERNS {
        if lower_name.contains(pat) {
            return ModSide::ClientOnly;
        }
    }

    // 3. Server-only keyword heuristics
    const SERVER_PATTERNS: &[&str] = &[
        "geyser", "floodgate", "viaversion", "viabackwards", "chunky",
        "spark", "luckperms", "discordsrv", "dynmap", "bluemap",
    ];

    for pat in SERVER_PATTERNS {
        if lower_name.contains(pat) {
            return ModSide::ServerOnly;
        }
    }

    ModSide::Both
}

/// Automated modpack build engine
pub struct ModpackBuilder;

impl ModpackBuilder {
    /// Inspects a directory (or server directory) and builds a ModpackBuildManifest
    pub fn build_manifest(
        name: &str,
        version: &str,
        loader: &str,
        minecraft_version: &str,
        base_dir: &Path,
    ) -> Result<ModpackBuildManifest> {
        let mods_dir = base_dir.join("mods");
        let mut components = Vec::new();

        if mods_dir.exists() {
            if let Ok(entries) = fs::read_dir(&mods_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if path.is_file() && path.extension().and_then(|e| e.to_str()) == Some("jar") {
                        let rel_path = format!("mods/{}", entry.file_name().to_string_lossy());
                        let sha512 = compute_file_sha512(&path).unwrap_or_default();
                        let size_bytes = fs::metadata(&path).map(|m| m.len()).unwrap_or(0);
                        let side = detect_mod_side(&path);

                        components.push(ModpackComponent {
                            file_path: rel_path,
                            sha512,
                            size_bytes,
                            side,
                            download_source: None,
                        });
                    }
                }
            }
        }

        // Sort components deterministically by file_path
        components.sort_by(|a, b| a.file_path.cmp(&b.file_path));

        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let mut metadata = HashMap::new();
        metadata.insert("builder".to_string(), "craft-modpack-ci".to_string());
        metadata.insert("total_components".to_string(), components.len().to_string());

        Ok(ModpackBuildManifest {
            name: name.to_string(),
            version: version.to_string(),
            loader: loader.to_string(),
            minecraft_version: minecraft_version.to_string(),
            created_at,
            components,
            server_archive_hash: None,
            client_archive_hash: None,
            metadata,
        })
    }

    /// Validates dependencies across all mod JARs in base_dir/mods
    pub fn validate_dependencies(base_dir: &Path) -> Result<Vec<String>> {
        let mods_dir = base_dir.join("mods");
        if !mods_dir.exists() {
            return Ok(Vec::new());
        }

        let mut installed_ids = HashSet::new();
        let mut required_deps: Vec<(String, String)> = Vec::new(); // (requiring_mod, required_id)

        if let Ok(entries) = fs::read_dir(&mods_dir) {
            for entry in entries.flatten() {
                let p = entry.path();
                if p.is_file() && p.extension().and_then(|e| e.to_str()) == Some("jar") {
                    if let Ok(info) = inspect_jar_manifest(&p) {
                        installed_ids.insert(info.id_or_name.to_lowercase());
                        for dep in info.dependencies {
                            if dep.required {
                                required_deps.push((info.id_or_name.clone(), dep.name_or_id));
                            }
                        }
                    }
                }
            }
        }

        // Standard ecosystem runtime IDs that are always satisfied
        const BUILTIN_IDS: &[&str] = &["minecraft", "java", "fabricloader", "quilt_loader", "forge", "neoforge"];
        for builtin in BUILTIN_IDS {
            installed_ids.insert(builtin.to_string());
        }

        let mut missing = Vec::new();
        for (mod_name, dep_id) in required_deps {
            let lower_dep = dep_id.to_lowercase();
            if !installed_ids.contains(&lower_dep) {
                missing.push(format!("Mod '{}' requires missing dependency '{}'", mod_name, dep_id));
            }
        }

        Ok(missing)
    }

    /// Bundles files into a compressed `.tar.zst` archive according to the target side
    pub fn export_bundle(
        manifest: &ModpackBuildManifest,
        base_dir: &Path,
        target_side: Option<ModSide>,
        output_archive_path: &Path,
    ) -> Result<String> {
        if let Some(parent) = output_archive_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let out_file = File::create(output_archive_path)?;
        let zstd_encoder = zstd::Encoder::new(out_file, 3)
            .map_err(|e| CraftError::Other(format!("Failed to initialize zstd encoder: {}", e)))?;
        let mut tar_builder = tar::Builder::new(zstd_encoder);

        // Filter components based on side
        for comp in &manifest.components {
            let include = match target_side {
                Some(ModSide::ServerOnly) => comp.side != ModSide::ClientOnly,
                Some(ModSide::ClientOnly) => comp.side != ModSide::ServerOnly,
                _ => true,
            };

            if include {
                let full_path = base_dir.join(&comp.file_path);
                if full_path.exists() {
                    tar_builder
                        .append_path_with_name(&full_path, &comp.file_path)
                        .map_err(|e| {
                            CraftError::Other(format!(
                                "Failed to append {} to archive: {}",
                                comp.file_path, e
                            ))
                        })?;
                }
            }
        }

        // Include config directory if it exists
        let config_dir = base_dir.join("config");
        if config_dir.exists() && config_dir.is_dir() {
            Self::append_dir_recursive(&mut tar_builder, &config_dir, "config")?;
        }

        let zstd_encoder = tar_builder.into_inner().map_err(|e| {
            CraftError::Other(format!("Failed to finish tar archive construction: {}", e))
        })?;
        zstd_encoder
            .finish()
            .map_err(|e| CraftError::Other(format!("Failed to finalize zstd archive: {}", e)))?;

        // Compute and return SHA-256 of generated archive
        BinaryDeltaEngine::compute_file_sha256(output_archive_path)
    }

    fn append_dir_recursive<W: std::io::Write>(
        builder: &mut tar::Builder<W>,
        disk_dir: &Path,
        archive_prefix: &str,
    ) -> Result<()> {
        if let Ok(entries) = fs::read_dir(disk_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let fname = entry.file_name().to_string_lossy().to_string();
                let sub_prefix = format!("{}/{}", archive_prefix, fname);
                if path.is_file() {
                    let _ = builder.append_path_with_name(&path, &sub_prefix);
                } else if path.is_dir() {
                    let _ = Self::append_dir_recursive(builder, &path, &sub_prefix);
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[test]
    fn test_detect_mod_side_heuristics() {
        assert_eq!(
            detect_mod_side(Path::new("mods/sodium-fabric-0.5.8.jar")),
            ModSide::ClientOnly
        );
        assert_eq!(
            detect_mod_side(Path::new("mods/iris-1.6.14.jar")),
            ModSide::ClientOnly
        );
        assert_eq!(
            detect_mod_side(Path::new("mods/geyser-spigot.jar")),
            ModSide::ServerOnly
        );
        assert_eq!(
            detect_mod_side(Path::new("mods/viaversion-4.9.2.jar")),
            ModSide::ServerOnly
        );
        assert_eq!(
            detect_mod_side(Path::new("mods/custom-mechanics-1.0.jar")),
            ModSide::Both
        );
    }

    #[test]
    fn test_build_manifest_and_export_bundle() {
        let dir = tempdir().unwrap();
        let mods_dir = dir.path().join("mods");
        fs::create_dir_all(&mods_dir).unwrap();

        let client_mod = mods_dir.join("sodium-mc1.20.jar");
        let server_mod = mods_dir.join("geyser-fabric.jar");
        let universal_mod = mods_dir.join("apples-core.jar");

        File::create(&client_mod).unwrap().write_all(b"dummy client mod").unwrap();
        File::create(&server_mod).unwrap().write_all(b"dummy server mod").unwrap();
        File::create(&universal_mod).unwrap().write_all(b"dummy universal mod").unwrap();

        let manifest = ModpackBuilder::build_manifest(
            "testpack",
            "1.0.0",
            "fabric",
            "1.20.4",
            dir.path(),
        )
        .unwrap();

        assert_eq!(manifest.components.len(), 3);

        let out_archive = dir.path().join("testpack-server.tar.zst");
        let hash = ModpackBuilder::export_bundle(
            &manifest,
            dir.path(),
            Some(ModSide::ServerOnly),
            &out_archive,
        )
        .unwrap();

        assert!(!hash.is_empty());
        assert!(out_archive.exists());
    }
}
