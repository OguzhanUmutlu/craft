use craft_core::{CraftPaths, Result};

pub async fn handle_hibernate(name: &str, wake: bool, paths: &CraftPaths) -> Result<()> {
    crate::commands::autoscale::handle_hibernate(name, wake, paths).await
}
