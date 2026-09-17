use crate::cli::CacheCommands;
use colored::Colorize;
use craft_core::{format_size, parse_size, CraftError, CraftPaths, GlobalSettings, Result};
use craft_providers::CacheManager;

pub fn handle_cache(action: Option<CacheCommands>, paths: &CraftPaths) -> Result<()> {
    let mut cache = CacheManager::new(paths);

    match action {
        None | Some(CacheCommands::Info) => {
            let stats = cache.get_stats();
            let pct = if stats.max_bytes > 0 {
                (stats.total_bytes as f64 / stats.max_bytes as f64) * 100.0
            } else {
                0.0
            };

            let bar = render_bar(stats.total_bytes, stats.max_bytes, 24);

            println!("{}", "Craft Cache Status:".bold().cyan());
            println!(
                "  Directory:       {}",
                cache.cache_dir().display().to_string().yellow()
            );
            println!(
                "  Usage:           {} {} / {} ({:.1}%)",
                bar.cyan(),
                format_size(stats.total_bytes).bold(),
                format_size(stats.max_bytes),
                pct
            );
            println!(
                "  Limits:          High (Limit): {} | Low Watermark: {}",
                format_size(stats.max_bytes),
                format_size(stats.low_watermark_bytes)
            );
            println!();
            println!("{}", "Storage Breakdown:".bold());
            println!("  Binary Artifacts (Jars, Zips):");
            println!("    Count:         {}", stats.artifacts_count);
            println!("    Disk Usage:    {}", format_size(stats.artifacts_bytes));
            println!("    Note:          Deduplicated across servers via zero-copy OS hardlinks.");
            println!("  Compressed Metadata (API Manifests, Search Queries):");
            println!("    Count:         {}", stats.metadata_count);
            println!(
                "    Compressed:    {} (Zstandard level 3)",
                format_size(stats.metadata_compressed_bytes)
            );
            println!(
                "    Uncompressed:  {}",
                format_size(stats.metadata_uncompressed_bytes)
            );
            println!(
                "    Savings:       {} ({:.1}% reduction)",
                format_size(stats.savings_bytes).green(),
                stats.savings_ratio_pct
            );
        }
        Some(CacheCommands::Clean { force, expired }) => {
            if expired {
                let cleaned = cache.clean_expired()?;
                println!(
                    "{}",
                    format!(
                        "Cleared {} of expired metadata cache.",
                        format_size(cleaned)
                    )
                    .green()
                );
            } else {
                if !force {
                    return Err(CraftError::Other(
                        "Please provide '--force' to confirm cache cleaning. This action is irreversible.".to_string(),
                    ));
                }
                let cleaned = cache.clean_cache()?;
                println!(
                    "{}",
                    format!("Cleared {} of download cache.", format_size(cleaned)).green()
                );
            }
        }
        Some(CacheCommands::Prune) => {
            let freed = cache.prune()?;
            let stats = cache.get_stats();
            if freed > 0 {
                println!(
                    "{}",
                    format!(
                        "Pruned {} from cache (evicted oldest items to low watermark).",
                        format_size(freed)
                    )
                    .green()
                );
            } else {
                println!(
                    "{}",
                    format!(
                        "Cache is already within optimal limits ({} <= {}).",
                        format_size(stats.total_bytes),
                        format_size(stats.low_watermark_bytes)
                    )
                    .cyan()
                );
            }
        }
        Some(CacheCommands::SetLimit { limit }) => {
            let max_bytes = parse_size(&limit).ok_or_else(|| {
                CraftError::Other(format!(
                    "Invalid size format: '{}'. Expected format like '500MB', '2GB', '5GB'.",
                    limit
                ))
            })?;

            if max_bytes == 0 {
                return Err(CraftError::Other(
                    "Cache limit must be greater than zero.".to_string(),
                ));
            }

            let mut settings = GlobalSettings::load(paths).unwrap_or_default();
            settings.cache_max_bytes = max_bytes;
            settings.save(paths)?;

            cache.store_mut().set_max_bytes(max_bytes);

            let freed = cache.prune()?;
            let stats = cache.get_stats();

            println!(
                "{}",
                format!(
                    "Cache limit set to {} (low watermark: {}).",
                    format_size(max_bytes).bold(),
                    format_size(stats.low_watermark_bytes)
                )
                .green()
            );

            if freed > 0 {
                println!(
                    "{}",
                    format!(
                        "Automatically pruned {} to respect new capacity limit.",
                        format_size(freed)
                    )
                    .yellow()
                );
            }
        }
    }

    Ok(())
}

fn render_bar(used: u64, total: u64, width: usize) -> String {
    if total == 0 {
        return format!("[{}]", "-".repeat(width));
    }
    let ratio = (used as f64 / total as f64).clamp(0.0, 1.0);
    let filled = (ratio * width as f64).round() as usize;
    let empty = width.saturating_sub(filled);
    format!("[{}{}]", "#".repeat(filled), "-".repeat(empty))
}
