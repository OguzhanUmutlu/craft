use colored::Colorize;
use craft_core::{CraftError, Result};
use craft_providers::{find_software, get_all_softwares};

pub async fn handle_update(softwares_arg: Vec<String>) -> Result<()> {
    let targets = if softwares_arg.is_empty() {
        get_all_softwares()
    } else {
        let mut list = Vec::new();
        for id in softwares_arg {
            let soft = find_software(&id).ok_or(CraftError::UnknownSoftware(id))?;
            list.push(soft);
        }
        list
    };

    for software in targets {
        println!("{}", format!("Updating versions for {}...", software.name()).cyan());
        match software.fetch_versions().await {
            Ok(versions) => {
                println!("{}", format!("✓ {} updated ({} versions available)", software.name(), versions.len()).green());
            }
            Err(e) => {
                println!("{}", format!("✗ Failed to update {}: {}", software.name(), e).red());
            }
        }
    }

    println!("{}", "Update check complete!".green().bold());
    Ok(())
}
