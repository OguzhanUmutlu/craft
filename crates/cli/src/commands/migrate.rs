use colored::Colorize;
use craft_core::{CraftPaths, Result};
use craft_remote::{MigrationOptions, ServerMigrator};

pub async fn handle_migrate(
    server: &str,
    to: &str,
    remote_name: Option<String>,
    remote_port: Option<u16>,
    trash_source: bool,
    start: bool,
    paths: &CraftPaths,
) -> Result<()> {
    println!(
        "{}",
        format!(
            "[MIGRATE] Preparing cross-node migration of server '{}' -> remote host '{}'...",
            server, to
        )
        .bold()
        .cyan()
    );

    let options = MigrationOptions {
        source_server: server.to_string(),
        remote_alias: to.to_string(),
        remote_name,
        remote_port,
        trash_source,
        start_remote: start,
    };

    let result = ServerMigrator::migrate(paths, &options, |step| {
        println!("  {} {}", "->".blue().bold(), step);
    })?;

    println!();
    println!(
        "{}",
        "[SUCCESS] Server migration completed successfully!"
            .green()
            .bold()
    );
    println!("  {:<20} {}", "Source Server:".dimmed(), result.source_name);
    println!("  {:<20} {}", "Remote Host:".dimmed(), result.remote_alias);
    println!("  {:<20} {}", "Remote Name:".dimmed(), result.target_name);
    println!("  {:<20} {}", "Remote Path:".dimmed(), result.remote_path);
    if let Some(port) = result.remote_port {
        println!("  {:<20} {}", "Network Port:".dimmed(), port);
    }
    println!(
        "  {:<20} {:.2} MB",
        "Archive Size:".dimmed(),
        result.archive_size_bytes as f64 / (1024.0 * 1024.0)
    );
    println!("  {:<20} {}", "SHA-256 Checksum:".dimmed(), result.checksum);
    if result.trashed_locally {
        println!(
            "  {:<20} {}",
            "Source Cleanup:".dimmed(),
            "Moved to non-destructive trash bin (recoverable with 'craft trash restore')"
        );
    }
    if result.started_remote {
        println!(
            "  {:<20} {}",
            "Daemon Status:".dimmed(),
            "Started on remote supervisor"
        );
    }

    Ok(())
}
