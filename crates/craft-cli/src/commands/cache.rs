use colored::Colorize;
use craft_core::{CraftError, CraftPaths, Result};
use craft_providers::CacheManager;
use crate::cli::CacheCommands;

pub fn handle_cache(action: Option<CacheCommands>, paths: &CraftPaths) -> Result<()> {
    let cache = CacheManager::new(paths);

    match action {
        None => {
            let size = cache.get_cache_size();
            let mb = (size as f64) / (1024.0 * 1024.0);
            println!("{}", format!("Current download cache size: {:.2} MB", mb).cyan());
        }
        Some(CacheCommands::Clean { force }) => {
            if !force {
                return Err(CraftError::Other(
                    "Please provide '--force' to confirm cache cleaning. This action is irreversible.".to_string(),
                ));
            }
            let cleaned = cache.clean_cache()?;
            let mb = (cleaned as f64) / (1024.0 * 1024.0);
            println!("{}", format!("Cleared {:.2} MB of download cache.", mb).green());
        }
    }

    Ok(())
}
