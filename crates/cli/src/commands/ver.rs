use colored::Colorize;
use craft_core::{CraftError, Result};
use craft_providers::{find_software, get_all_softwares};

pub async fn handle_ver(software_arg: Option<String>) -> Result<()> {
    if let Some(soft_id) = software_arg {
        let software =
            find_software(&soft_id).ok_or_else(|| CraftError::UnknownSoftware(soft_id.clone()))?;

        println!(
            "{}",
            format!("Available versions for {}:", software.name())
                .cyan()
                .bold()
        );
        let catalog_mgr = craft_providers::CatalogManager::new().ok();
        let catalog_versions = if let Some(ref mgr) = catalog_mgr {
            mgr.load().get_versions(software.id())
        } else {
            Vec::new()
        };
        let mut versions = if !catalog_versions.is_empty() {
            catalog_versions
        } else {
            software.bundled_versions()
        };
        craft_core::sort_versions_descending(&mut versions);
        let recommended = software.recommended_version();
        println!("Recommended: {}", recommended.green().bold());
        println!("{}", versions.join(", "));
    } else {
        println!("{}", "Available server softwares:".cyan().bold());
        for software in get_all_softwares() {
            let rec = software.recommended_version();
            println!(
                "  - {:<18} ({}) - Recommended: {}",
                software.id().green(),
                software.name(),
                rec.cyan()
            );
        }
        println!("\nUse 'craft ver <software>' to view all versions for a given server software.");
    }

    Ok(())
}
