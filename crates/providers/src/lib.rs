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

pub use traits::{AssetDownload, ServerEdition, ServerSoftware};
pub use cache::CacheManager;
pub use paper::PaperProvider;
pub use purpur::PurpurProvider;
pub use vanilla_java::VanillaJavaProvider;
pub use fabric::FabricProvider;
pub use spigot::SpigotProvider;
pub use vanilla_bedrock::VanillaBedrockProvider;
pub use pocketmine::PocketmineProvider;

pub fn get_all_softwares() -> Vec<Arc<dyn ServerSoftware>> {
    vec![
        Arc::new(PaperProvider::new_paper()),
        Arc::new(PurpurProvider::new()),
        Arc::new(PaperProvider::new_folia()),
        Arc::new(VanillaJavaProvider::new()),
        Arc::new(FabricProvider::new()),
        Arc::new(SpigotProvider::new()),
        Arc::new(VanillaBedrockProvider::new()),
        Arc::new(PocketmineProvider::new()),
    ]
}

pub fn find_software(id: &str) -> Option<Arc<dyn ServerSoftware>> {
    let lower = id.to_lowercase();
    get_all_softwares().into_iter().find(|s| {
        s.id().eq_ignore_ascii_case(&lower)
            || s.name().eq_ignore_ascii_case(&lower)
            || (lower == "vanilla" && s.id() == "vanilla_java")
            || (lower == "bedrock" && s.id() == "vanilla_bedrock")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_all_supported_softwares() {
        assert!(find_software("paper").is_some());
        assert!(find_software("purpur").is_some());
        assert!(find_software("folia").is_some());
        assert!(find_software("vanilla_java").is_some());
        assert!(find_software("vanilla").is_some()); // Alias
        assert!(find_software("fabric").is_some());
        assert!(find_software("spigot").is_some());
        assert!(find_software("vanilla_bedrock").is_some());
        assert!(find_software("bedrock").is_some()); // Alias
        assert!(find_software("pocketmine").is_some());
    }

    #[test]
    fn test_bundled_versions_non_empty() {
        for soft in get_all_softwares() {
            let versions = soft.bundled_versions();
            assert!(!versions.is_empty(), "Software {} has empty bundled versions", soft.name());
        }
    }
}
