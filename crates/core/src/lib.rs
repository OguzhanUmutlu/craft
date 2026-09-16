pub mod error;
pub mod path;
pub mod config;
pub mod game;
pub mod remote_config;
pub mod backup_config;
pub mod java;
pub mod process;
pub mod trash;
pub mod cache;
pub mod nbt;
pub mod properties;

pub use error::{CraftError, Result};
pub use path::CraftPaths;
pub use game::{
    find_game, get_supported_games, ConfigFormat, GameDefinition, QueryProtocolKind, RuntimeKind,
    TransportProtocol,
};
pub use config::{
    default_game_id, get_default_world, get_dimension_worlds, set_default_world, set_end_world,
    set_nether_world, GlobalSettings, ServerConfig, ServersRegistry,
};
pub use cache::{CacheStore, CacheStats, CacheEntryMeta, parse_size, format_size};
pub use remote_config::{RemoteHostConfig, RemoteAuthType, RemoteOsType, RemotesRegistry};
pub use backup_config::{
    AutoBackupPolicy, GDriveBackupConfig, GDriveBackupTarget, GlobalBackupRegistry,
    LocalBackupTarget, S3BackupConfig, S3BackupTarget,
};
pub use trash::{TrashItem, TrashManifest, TrashManager};
pub use java::{JavaInstallation, get_jar_java_version, get_java_installations, find_best_java};
pub use process::{
    is_process_running, kill_process, read_pid_file, write_pid_file, remove_pid_file,
    auto_heal_server_jar, auto_heal_server_file, ServerLockGuard, get_server_running_pid, is_server_locked,
};
pub use nbt::{NbtFile, NbtTag};
pub use properties::{PropertyCategory, PropertyLine, ServerProperties};

pub const CRAFT_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Truncates a string to at most `max_chars` Unicode scalar values without slicing across UTF-8 boundaries.
pub fn truncate_str(s: &str, max_chars: usize) -> &str {
    match s.char_indices().nth(max_chars) {
        None => s,
        Some((idx, _)) => &s[..idx],
    }
}

/// Truncates a string to at most `max_chars` characters, appending an ellipsis ("...") if truncated.
/// The resulting string length in characters will not exceed `max_chars` (unless `max_chars < 3`).
pub fn truncate_ellipsis(s: &str, max_chars: usize) -> String {
    let char_count = s.chars().count();
    if char_count <= max_chars {
        s.to_string()
    } else {
        let keep_chars = max_chars.saturating_sub(3);
        let truncated = truncate_str(s, keep_chars);
        format!("{}...", truncated)
    }
}

/// Parses a semantic version string (e.g. "1.0.1" or "v1.2.3-alpha") into (major, minor, patch)
pub fn parse_semver(v: &str) -> Option<(u32, u32, u32)> {
    let clean = v.trim().trim_start_matches('v');
    let parts: Vec<&str> = clean.split('.').collect();
    if parts.len() >= 2 {
        let major = parts[0].parse().ok()?;
        let minor = parts[1].parse().ok()?;
        let patch = parts
            .get(2)
            .and_then(|p| p.split('-').next())
            .and_then(|p| p.parse().ok())
            .unwrap_or(0);
        Some((major, minor, patch))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_semver() {
        assert_eq!(parse_semver("1.0.1"), Some((1, 0, 1)));
        assert_eq!(parse_semver("v1.0.1"), Some((1, 0, 1)));
        assert_eq!(parse_semver("1.2"), Some((1, 2, 0)));
        assert_eq!(parse_semver("1.2.3-rc1"), Some((1, 2, 3)));
        assert_eq!(parse_semver("invalid"), None);
        assert!(parse_semver("1.0.0").unwrap() < parse_semver("1.0.1").unwrap());
        assert!(parse_semver("1.0.1").unwrap() < parse_semver("1.0.2").unwrap());
    }

    #[test]
    fn test_truncate_utf8_safety() {
        // Cyrillic string where byte index 37 falls inside a 2-byte character 'и'
        let cyrillic = "Плагин для серверов Minecraft и других игр";
        // Ensure truncate_str does not panic
        let t = truncate_str(cyrillic, 37);
        assert!(t.len() <= cyrillic.len());

        // Ensure truncate_ellipsis does not panic and ends with ellipsis
        let el = truncate_ellipsis(cyrillic, 40);
        assert!(el.ends_with("..."));
        assert_eq!(el.chars().count(), 40);

        // Multi-byte CJK and 4-byte Unicode characters
        let multibyte_str = "Minecraft Server \u{10348} Best Plugins & Performance 日本語";
        let em = truncate_ellipsis(multibyte_str, 25);
        assert!(em.ends_with("..."));
        assert_eq!(em.chars().count(), 25);

        // Short string should not be truncated
        assert_eq!(truncate_ellipsis("short", 10), "short");
        assert_eq!(truncate_str("hello", 10), "hello");
    }
}
