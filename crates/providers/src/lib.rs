use std::sync::Arc;

pub mod traits;
pub mod cache;
pub mod bundled;
pub mod paper;
pub mod purpur;
pub mod vanilla_java;
pub mod fabric;
pub mod spigot;
pub mod vanilla_bedrock;
pub mod pocketmine;
pub mod bungee;
pub mod geyser;
pub mod neoforge;
pub mod quilt;
pub mod nukkit;
pub mod waterdog;
pub mod palworld;
pub mod terraria;
pub mod valheim;
pub mod factorio;
pub mod custom;

pub use traits::{AssetDownload, ServerEdition, ServerSoftware};
pub use cache::CacheManager;
pub use paper::PaperProvider;
pub use purpur::PurpurProvider;
pub use vanilla_java::VanillaJavaProvider;
pub use fabric::FabricProvider;
pub use spigot::SpigotProvider;
pub use vanilla_bedrock::VanillaBedrockProvider;
pub use pocketmine::PocketmineProvider;
pub use bungee::BungeeProvider;
pub use geyser::GeyserProvider;
pub use neoforge::NeoForgeProvider;
pub use quilt::QuiltProvider;
pub use nukkit::NukkitProvider;
pub use waterdog::WaterdogProvider;
pub use palworld::PalworldProvider;
pub use terraria::TerrariaProvider;
pub use valheim::ValheimProvider;
pub use factorio::FactorioProvider;
pub use custom::CustomGameProvider;

pub fn get_all_softwares() -> Vec<Arc<dyn ServerSoftware>> {
    vec![
        Arc::new(PaperProvider::new_paper()),
        Arc::new(PurpurProvider::new()),
        Arc::new(PaperProvider::new_folia()),
        Arc::new(PaperProvider::new_velocity()),
        Arc::new(PaperProvider::new_waterfall()),
        Arc::new(VanillaJavaProvider::new()),
        Arc::new(FabricProvider::new()),
        Arc::new(QuiltProvider::new()),
        Arc::new(NeoForgeProvider::new()),
        Arc::new(SpigotProvider::new()),
        Arc::new(BungeeProvider::new()),
        Arc::new(GeyserProvider::new()),
        Arc::new(VanillaBedrockProvider::new()),
        Arc::new(PocketmineProvider::new()),
        Arc::new(NukkitProvider::new()),
        Arc::new(WaterdogProvider::new()),
        Arc::new(PalworldProvider::new()),
        Arc::new(TerrariaProvider::new()),
        Arc::new(ValheimProvider::new()),
        Arc::new(FactorioProvider::new()),
        Arc::new(CustomGameProvider::new()),
    ]
}

pub fn get_softwares_for_game(game_id: &str) -> Vec<Arc<dyn ServerSoftware>> {
    get_all_softwares()
        .into_iter()
        .filter(|s| s.game_id().eq_ignore_ascii_case(game_id))
        .collect()
}

pub fn find_software(id: &str) -> Option<Arc<dyn ServerSoftware>> {
    let lower = id.to_lowercase();
    get_all_softwares().into_iter().find(|s| {
        s.id().eq_ignore_ascii_case(&lower)
            || s.name().eq_ignore_ascii_case(&lower)
            || (lower == "vanilla" && s.id() == "vanilla_java")
            || (lower == "bedrock" && s.id() == "vanilla_bedrock")
            || (lower == "bungee" && s.id() == "bungeecord")
            || (lower == "palworld" && s.id() == "palserver")
            || (lower == "terraria" && s.id() == "tshock")
    })
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
        let custom = get_softwares_for_game("custom");
        assert_eq!(custom.len(), 1);
    }

    #[test]
    fn test_bundled_versions_non_empty() {
        for soft in get_all_softwares() {
            let versions = soft.bundled_versions();
            assert!(!versions.is_empty(), "Software {} has empty bundled versions", soft.name());
        }
    }

    #[test]
    fn test_software_descriptions() {
        let softwares = get_all_softwares();
        assert_eq!(softwares.len(), 21);
        for soft in softwares {
            let desc = soft.description();
            assert!(!desc.is_empty(), "Software {} has empty description", soft.name());
            assert_ne!(desc, "Supported Minecraft Server Platform", "Software {} has generic placeholder", soft.name());
            assert!(desc.len() <= 48, "Software {} description is too long ({} chars): {}", soft.name(), desc.len(), desc);
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
            assert!(!caps.has_any(), "{} should have no content capabilities", id);
        }
    }
}
