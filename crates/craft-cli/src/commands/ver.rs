use colored::Colorize;
use craft_core::{CraftError, Result};
use craft_providers::{find_software, get_all_softwares};

pub async fn handle_ver(software_arg: Option<String>) -> Result<()> {
    if let Some(soft_id) = software_arg {
        let software = find_software(&soft_id).ok_or_else(|| {
            CraftError::UnknownSoftware(soft_id.clone())
        })?;

        println!("{}", format!("Available versions for {}:", software.name()).cyan().bold());
        let versions = software.bundled_versions();
        println!("{}", versions.join(", "));
    } else {
        println!("{}", "Available server softwares:".cyan().bold());
        for software in get_all_softwares() {
            println!("  - {:<18} ({})", software.id().green(), software.name());
        }
        println!("\nUse 'craft ver <software>' to view all versions for a given server software.");
    }

    Ok(())
}
