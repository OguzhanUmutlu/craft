use crate::cli::AuditCommands;
use colored::Colorize;
use comfy_table::modifiers::UTF8_ROUND_CORNERS;
use comfy_table::presets::UTF8_FULL;
use comfy_table::{Cell, Color, Row, Table};
use craft_core::{
    AuditLedger, CraftError, CraftPaths, Result, DEFAULT_AUDIT_SECRET, GENESIS_HASH,
};

pub fn handle_audit(action: AuditCommands, paths: &CraftPaths) -> Result<()> {
    match action {
        AuditCommands::Ls { limit } => handle_list(limit, paths),
        AuditCommands::Verify => handle_verify(paths),
    }
}

fn handle_list(limit: usize, paths: &CraftPaths) -> Result<()> {
    let entries = AuditLedger::read_entries(paths, Some(limit))?;

    if entries.is_empty() {
        println!("{}", "No audit log entries recorded yet in audit.log.".dimmed());
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(Row::from(vec![
            Cell::new("#").fg(Color::Yellow),
            Cell::new("Timestamp (UTC)").fg(Color::Cyan),
            Cell::new("Actor").fg(Color::Green),
            Cell::new("Action").fg(Color::White),
            Cell::new("Resource").fg(Color::Blue),
            Cell::new("Status").fg(Color::Magenta),
            Cell::new("HMAC Hash").fg(Color::DarkGrey),
        ]));

    for (idx, entry) in entries.iter().enumerate() {
        let status_cell = if entry.status.eq_ignore_ascii_case("success") {
            Cell::new(&entry.status).fg(Color::Green)
        } else {
            Cell::new(&entry.status).fg(Color::Red)
        };

        let short_hash = if entry.entry_hash.len() >= 12 {
            format!("{}..", &entry.entry_hash[..10])
        } else {
            entry.entry_hash.clone()
        };

        let timestamp_str = entry.timestamp.format("%Y-%m-%d %H:%M:%S").to_string();

        table.add_row(Row::from(vec![
            Cell::new((idx + 1).to_string()),
            Cell::new(timestamp_str),
            Cell::new(&entry.actor),
            Cell::new(&entry.action),
            Cell::new(entry.resource.as_deref().unwrap_or("-")),
            status_cell,
            Cell::new(short_hash).fg(Color::DarkGrey),
        ]));
    }

    println!("\n{}", format!("Recent Audit Ledger Entries (Showing last {}):", entries.len()).bold());
    println!("{}", table);
    Ok(())
}

fn handle_verify(paths: &CraftPaths) -> Result<()> {
    println!("{}", "Cryptographically verifying audit log HMAC hash chain...".cyan());

    let result = AuditLedger::verify_chain(paths, DEFAULT_AUDIT_SECRET)?;

    if result.is_valid {
        println!("\n{} Audit ledger cryptographic integrity verified successfully.", "[OK]".green().bold());
        println!("  Total entries:    {}", result.total_entries.to_string().cyan());
        println!("  Verified entries: {}", result.verified_entries.to_string().green());
        println!("  Genesis hash:     {}", GENESIS_HASH.dimmed());
        println!("  Chain status:     {}", "Continuous HMAC-SHA256 signature chain is intact and untampered.".green());
        Ok(())
    } else {
        eprintln!("\n{} Audit ledger verification FAILED - tampering or corruption detected!", "[ERROR]".red().bold());
        if let Some(corrupted) = result.corrupted_index {
            eprintln!("  Corrupted entry index: {}", corrupted.to_string().red().bold());
        }
        if let Some(err) = result.error_message {
            eprintln!("  Failure reason:        {}", err.red());
        }
        Err(CraftError::Other(
            "Audit ledger verification failed: cryptographic chain mismatch or invalid HMAC signature".to_string(),
        ))
    }
}
