pub mod error;
pub mod path;
pub mod config;
pub mod remote_config;
pub mod backup_config;
pub mod java;
pub mod process;
pub mod trash;

pub use error::{CraftError, Result};
pub use path::CraftPaths;
pub use config::{ServerConfig, ServersRegistry, GlobalSettings};
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

pub const CRAFT_VERSION: &str = env!("CARGO_PKG_VERSION");

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
}
