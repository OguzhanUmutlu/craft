use std::sync::Arc;

pub mod builtins;
pub mod bundled;
pub mod bungee;
pub mod cache;
pub mod catalog;
pub mod custom;
pub mod dynamic;
pub mod fabric;
pub mod factorio;
pub mod geyser;
pub mod neoforge;
pub mod nukkit;
pub mod palworld;
pub mod paper;
pub mod pocketmine;
pub mod purpur;
pub mod quilt;
pub mod registry;
pub mod spigot;
pub mod terraria;
pub mod traits;
pub mod valheim;
pub mod vanilla_bedrock;
pub mod vanilla_java;
pub mod waterdog;

pub use bungee::BungeeProvider;
pub use cache::CacheManager;
pub use catalog::{
    CatalogAsset, CatalogBuilder, CatalogManager, SoftwareCatalogEntry, VersionCatalog,
    DEFAULT_CATALOG_TTL_SECS, DEFAULT_CATALOG_URL,
};
pub use custom::CustomGameProvider;
pub use dynamic::DynamicSoftwareProvider;
pub use fabric::FabricProvider;
pub use factorio::FactorioProvider;
pub use geyser::GeyserProvider;
pub use neoforge::NeoForgeProvider;
pub use nukkit::NukkitProvider;
pub use palworld::PalworldProvider;
pub use paper::PaperProvider;
pub use pocketmine::PocketmineProvider;
pub use purpur::PurpurProvider;
pub use quilt::QuiltProvider;
pub use registry::{global_registry, reload_registry, SoftwareRegistry};
pub use spigot::SpigotProvider;
pub use terraria::TerrariaProvider;
pub use traits::{AssetDownload, ServerEdition, ServerSoftware};
pub use valheim::ValheimProvider;
pub use vanilla_bedrock::VanillaBedrockProvider;
pub use vanilla_java::VanillaJavaProvider;
pub use waterdog::WaterdogProvider;

pub fn get_all_softwares() -> Vec<Arc<dyn ServerSoftware>> {
    global_registry().read().unwrap().get_all()
}

pub fn get_softwares_for_game(game_id: &str) -> Vec<Arc<dyn ServerSoftware>> {
    global_registry().read().unwrap().for_game(game_id)
}

pub fn find_software(id: &str) -> Option<Arc<dyn ServerSoftware>> {
    global_registry().read().unwrap().find(id)
}

pub fn get_software_bundle(id: &str) -> Option<craft_scripting::SoftwareDefinitionBundle> {
    global_registry().read().unwrap().get_bundle(id).cloned()
}

pub fn get_content_capabilities(software: &str) -> craft_core::ContentCapabilities {
    if let Some(soft) = find_software(software) {
        soft.content_capabilities()
    } else {
        let lower = software.to_lowercase();
        if lower.contains("paper")
            || lower.contains("purpur")
            || lower.contains("spigot")
            || lower.contains("folia")
            || lower.contains("bukkit")
        {
            craft_core::ContentCapabilities::plugins_and_datapacks()
        } else if lower.contains("fabric") || lower.contains("forge") || lower.contains("quilt") {
            craft_core::ContentCapabilities::mods_and_datapacks()
        } else if lower.contains("velocity")
            || lower.contains("bungee")
            || lower.contains("waterfall")
            || lower.contains("pocketmine")
            || lower.contains("nukkit")
            || lower.contains("waterdog")
        {
            craft_core::ContentCapabilities::plugins_only()
        } else if lower.contains("vanilla") {
            craft_core::ContentCapabilities::datapacks_only()
        } else {
            craft_core::ContentCapabilities::none()
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_all_supported_softwares() {
        assert!(find_software("paper").is_some());
        assert!(find_software("purpur").is_some());
        assert!(find_software("folia").is_some());
        assert!(find_software("velocity").is_some());
        assert!(find_software("waterfall").is_some());
        assert!(find_software("vanilla_java").is_some());
        assert!(find_software("vanilla").is_some()); // Alias
        assert!(find_software("fabric").is_some());
        assert!(find_software("quilt").is_some());
        assert!(find_software("neoforge").is_some());
        assert!(find_software("spigot").is_some());
        assert!(find_software("bungeecord").is_some());
        assert!(find_software("bungee").is_some()); // Alias
        assert!(find_software("geyser").is_some());
        assert!(find_software("vanilla_bedrock").is_some());
        assert!(find_software("bedrock").is_some()); // Alias
        assert!(find_software("pocketmine").is_some());
        assert!(find_software("nukkit").is_some());
        assert!(find_software("waterdog").is_some());
        assert!(find_software("palserver").is_some());
        assert!(find_software("palworld").is_some());
        assert!(find_software("tshock").is_some());
        assert!(find_software("terraria").is_some());
        assert!(find_software("valheim").is_some());
        assert!(find_software("factorio").is_some());
        assert!(find_software("custom").is_some());
    }

    #[test]
    fn test_get_softwares_for_game() {
        let mc = get_softwares_for_game("minecraft");
        assert_eq!(mc.len(), 16);
        let pal = get_softwares_for_game("palworld");
        assert_eq!(pal.len(), 1);
        assert_eq!(pal[0].id(), "palserver");
        let terraria = get_softwares_for_game("terraria");
        assert_eq!(terraria.len(), 1);
        assert_eq!(terraria[0].id(), "tshock");
        let valheim = get_softwares_for_game("valheim");
        assert_eq!(valheim.len(), 1);
        let factorio = get_softwares_for_game("factorio");
        assert_eq!(factorio.len(), 1);
        let custom = get_softwares_for_game("");
        assert_eq!(custom.len(), 1);
        assert_eq!(custom[0].id(), "custom");
        assert_eq!(custom[0].game_id(), "");
    }

    #[test]
    fn test_bundled_versions_non_empty() {
        for soft in get_all_softwares() {
            let versions = soft.bundled_versions();
            assert!(
                !versions.is_empty(),
                "Software {} has empty bundled versions",
                soft.name()
            );
        }
    }

    #[test]
    fn test_software_descriptions() {
        let softwares = get_all_softwares();
        assert_eq!(softwares.len(), 21);
        for soft in softwares {
            let desc = soft.description();
            assert!(
                !desc.is_empty(),
                "Software {} has empty description",
                soft.name()
            );
            assert_ne!(
                desc,
                "Supported Minecraft Server Platform",
                "Software {} has generic placeholder",
                soft.name()
            );
            assert!(
                desc.len() <= 48,
                "Software {} description is too long ({} chars): {}",
                soft.name(),
                desc.len(),
                desc
            );
        }
    }

    #[test]
    fn test_content_capabilities() {
        // Paper, Purpur, Spigot, Folia: plugins=true, mods=false, datapacks=true
        for id in &["paper", "purpur", "spigot", "folia"] {
            let caps = get_content_capabilities(id);
            assert!(caps.plugins, "{} should support plugins", id);
            assert!(!caps.mods, "{} should NOT support mods", id);
            assert!(caps.datapacks, "{} should support datapacks", id);
        }

        // Fabric, Quilt, NeoForge: plugins=false, mods=true, datapacks=true
        for id in &["fabric", "quilt", "neoforge"] {
            let caps = get_content_capabilities(id);
            assert!(!caps.plugins, "{} should NOT support plugins", id);
            assert!(caps.mods, "{} should support mods", id);
            assert!(caps.datapacks, "{} should support datapacks", id);
        }

        // Vanilla Java: plugins=false, mods=false, datapacks=true
        let v_java = get_content_capabilities("vanilla_java");
        assert!(!v_java.plugins);
        assert!(!v_java.mods);
        assert!(v_java.datapacks);

        // Vanilla Bedrock: plugins=false, mods=false, datapacks=false
        let v_bedrock = get_content_capabilities("vanilla_bedrock");
        assert!(!v_bedrock.plugins);
        assert!(!v_bedrock.mods);
        assert!(!v_bedrock.datapacks);

        // Proxies (Velocity, BungeeCord, Waterfall): plugins=true, mods=false, datapacks=false
        for id in &["velocity", "bungeecord", "waterfall"] {
            let caps = get_content_capabilities(id);
            assert!(caps.plugins, "{} should support plugins", id);
            assert!(!caps.mods, "{} should NOT support mods", id);
            assert!(!caps.datapacks, "{} should NOT support datapacks", id);
        }

        // Bedrock servers (PocketMine, Nukkit, Waterdog): plugins=true, mods=false, datapacks=false
        for id in &["pocketmine", "nukkit", "waterdog"] {
            let caps = get_content_capabilities(id);
            assert!(caps.plugins, "{} should support plugins", id);
            assert!(!caps.mods, "{} should NOT support mods", id);
            assert!(!caps.datapacks, "{} should NOT support datapacks", id);
        }

        // Non-Minecraft games:
        // Terraria (tshock): plugins=true, mods=false, datapacks=false
        let terraria = get_content_capabilities("tshock");
        assert!(terraria.plugins);
        assert!(!terraria.mods);
        assert!(!terraria.datapacks);

        // Factorio: plugins=false, mods=true, datapacks=false
        let factorio = get_content_capabilities("factorio");
        assert!(!factorio.plugins);
        assert!(factorio.mods);
        assert!(!factorio.datapacks);

        // Palworld, Valheim, Custom: none
        for id in &["palserver", "valheim", "custom"] {
            let caps = get_content_capabilities(id);
            assert!(
                !caps.has_any(),
                "{} should have no content capabilities",
                id
            );
        }
    }

    #[test]
    fn test_software_registry_user_override_and_custom_package() {
        use craft_core::CraftPaths;
        use craft_scripting::package_directory;
        use tempfile::tempdir;

        let dir = tempdir().expect("tempdir");
        let paths = CraftPaths::from_base(dir.path().to_path_buf());
        std::fs::create_dir_all(&paths.softwares_dir).expect("create softwares dir");

        // 1. Initial registry has 21 softwares
        let mut reg = SoftwareRegistry::load(&paths);
        assert_eq!(reg.get_all().len(), 21);

        // 2. Add a new custom software definition as a directory
        let new_sw_dir = paths.softwares_dir.join("myserver");
        std::fs::create_dir_all(&new_sw_dir).expect("create myserver dir");
        let toml_content = r#"
[software]
id = "myserver"
name = "My Custom Server"
game = "custom"
edition = "native"
description = "My test custom game"
"#;
        std::fs::write(new_sw_dir.join("software.toml"), toml_content)
            .expect("write software.toml");

        // 3. Package another custom software into a .zip bundle
        let bundle_src = dir.path().join("bundled_src");
        std::fs::create_dir_all(&bundle_src).expect("create bundle src");
        let bundle_toml = r#"
[software]
id = "bundlegame"
name = "Bundle Game"
game = "custom"
edition = "native"
description = "Packed .zip bundle test"
"#;
        std::fs::write(bundle_src.join("software.toml"), bundle_toml).expect("write toml");
        let zip_file = paths.softwares_dir.join("bundlegame.zip");
        package_directory(&bundle_src, &zip_file).expect("package zip file");

        // 4. Reload registry and verify both new softwares are discovered
        reg = SoftwareRegistry::load(&paths);
        assert_eq!(reg.get_all().len(), 23);

        let myserver = reg.find("myserver").expect("myserver found");
        assert_eq!(myserver.name(), "My Custom Server");
        assert_eq!(myserver.game_id(), "custom");

        let bundlegame = reg.find("bundlegame").expect("bundlegame found");
        assert_eq!(bundlegame.name(), "Bundle Game");

        // 5. Test override of an existing built-in software (e.g. paper)
        let override_dir = paths.softwares_dir.join("paper_override");
        std::fs::create_dir_all(&override_dir).expect("create override dir");
        let override_toml = r#"
[software]
id = "paper"
name = "Paper Custom Override"
game = "minecraft"
edition = "java"
description = "User overridden Paper definition"
"#;
        std::fs::write(override_dir.join("software.toml"), override_toml)
            .expect("write override toml");

        reg = SoftwareRegistry::load(&paths);
        let paper = reg.find("paper").expect("paper found");
        assert_eq!(paper.name(), "Paper Custom Override");
        assert_eq!(paper.description(), "User overridden Paper definition");
    }

    #[test]
    fn test_standalone_software_definition_empty_game_and_zip() {
        let temp_dir = tempfile::tempdir().expect("tempdir");
        let paths = craft_core::CraftPaths::from_base(temp_dir.path().to_path_buf());
        std::fs::create_dir_all(&paths.softwares_dir).expect("create softwares_dir");

        // 1. Create a standalone software with empty game
        let enshrouded_dir = paths.softwares_dir.join("enshrouded");
        std::fs::create_dir_all(&enshrouded_dir).expect("create enshrouded dir");
        let toml_content = r#"
[software]
id = "enshrouded"
name = "Enshrouded"
display_name = "Enshrouded Dedicated Server"
game = ""
edition = "native"
description = "Survival action RPG server"
version = "1.0.0"
"#;
        std::fs::write(enshrouded_dir.join("software.toml"), toml_content).expect("write toml");

        let reg = SoftwareRegistry::load(&paths);
        let enshrouded = reg.find("enshrouded").expect("enshrouded found");
        assert_eq!(enshrouded.name(), "Enshrouded");
        assert_eq!(enshrouded.display_name(), "Enshrouded Dedicated Server");
        assert_eq!(enshrouded.game_id(), "");

        // Standalone softwares with empty game_id appear in for_game("")
        let standalones = reg.for_game("");
        assert!(standalones.iter().any(|s| s.id() == "custom"));
        assert!(standalones.iter().any(|s| s.id() == "enshrouded"));
    }
}
