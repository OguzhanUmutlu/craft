use crate::client::RemoteCraftClient;
use chrono::Utc;
use craft_core::{
    is_server_locked, get_server_running_pid,
    CraftError, CraftPaths, RemotesRegistry, Result,
    ServersRegistry, TrashManager,
};
use flate2::write::GzEncoder;
use flate2::Compression;
use sha2::{Digest, Sha256};
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct MigrationOptions {
    pub source_server: String,
    pub remote_alias: String,
    pub remote_name: Option<String>,
    pub remote_port: Option<u16>,
    pub trash_source: bool,
    pub start_remote: bool,
}

#[derive(Debug, Clone)]
pub struct MigrationResult {
    pub source_name: String,
    pub target_name: String,
    pub remote_alias: String,
    pub archive_size_bytes: u64,
    pub checksum: String,
    pub remote_path: String,
    pub remote_port: Option<u16>,
    pub trashed_locally: bool,
    pub started_remote: bool,
}

pub struct ServerMigrator;

impl ServerMigrator {
    /// Migrates a local server to a remote Craft host.
    pub fn migrate<F>(
        paths: &CraftPaths,
        options: &MigrationOptions,
        progress: F,
    ) -> Result<MigrationResult>
    where
        F: Fn(&str),
    {
        progress("Verifying local server status...");
        let local_reg = ServersRegistry::load(paths)?;
        let source_config = local_reg
            .find_by_name(&options.source_server)
            .ok_or_else(|| {
                CraftError::Config(format!(
                    "Server '{}' was not found in the local registry.",
                    options.source_server
                ))
            })?
            .clone();

        if !source_config.path.exists() {
            return Err(CraftError::InvalidPath(format!(
                "Local server directory '{}' does not exist.",
                source_config.path.display()
            )));
        }

        if is_server_locked(&source_config.path)
            || get_server_running_pid(&source_config.path).is_some()
        {
            return Err(CraftError::Other(format!(
                "Server '{}' is currently running. Please stop it first ('craft stop {}') to ensure data consistency before migrating.",
                options.source_server, options.source_server
            )));
        }

        progress(&format!(
            "Connecting to remote host '{}'...",
            options.remote_alias
        ));
        let remotes_reg = RemotesRegistry::load(paths)?;
        let remote_cfg = remotes_reg
            .find(&options.remote_alias)
            .ok_or_else(|| {
                CraftError::Config(format!(
                    "Remote alias '{}' was not found in the remotes registry.",
                    options.remote_alias
                ))
            })?
            .clone();

        let client = RemoteCraftClient::connect(&remote_cfg)?;

        progress("Checking remote Craft installation...");
        if !client.is_craft_installed() {
            return Err(CraftError::Other(format!(
                "Craft is not installed on remote host '{}'. Please run 'craft remote bootstrap {}' first.",
                options.remote_alias, options.remote_alias
            )));
        }

        let target_name = options
            .remote_name
            .clone()
            .unwrap_or_else(|| options.source_server.clone());

        let remote_servers = client.list_servers()?;
        if remote_servers
            .iter()
            .any(|s| s.name.eq_ignore_ascii_case(&target_name))
        {
            return Err(CraftError::Other(format!(
                "A server named '{}' already exists on remote host '{}'.",
                target_name, options.remote_alias
            )));
        }

        // Check if remote supports zstd
        let (code, _, _) = client
            .session
            .exec("tar --help 2>&1 | grep -E 'zstd|--zstd' || which zstd 2>/dev/null")?;
        let use_zstd = code == 0;

        let ext = if use_zstd { "tar.zst" } else { "tar.gz" };
        let archive_name = format!(
            "{}_migration_{}.{}",
            options.source_server,
            Utc::now().format("%Y%m%d_%H%M%S"),
            ext
        );

        let temp_dir = tempfile::tempdir().map_err(|e| {
            CraftError::Other(format!(
                "Failed to create temporary directory for migration: {}",
                e
            ))
        })?;
        let local_archive_path = temp_dir.path().join(&archive_name);

        progress(&format!(
            "Compressing server into atomic snapshot ({ext})..."
        ));
        let (archive_size_bytes, checksum) =
            pack_server_archive(&source_config.path, &local_archive_path, use_zstd)?;

        progress(&format!(
            "Uploading snapshot via SFTP ({:.2} MB)...",
            archive_size_bytes as f64 / (1024.0 * 1024.0)
        ));
        let _ = client.session.exec("mkdir -p ~/.craft/staging")?;
        let remote_staging_path = PathBuf::from(format!(".craft/staging/{}", archive_name));
        client
            .sftp()
            .upload_file(&local_archive_path, &remote_staging_path)?;

        progress("Verifying SHA-256 integrity checksum on remote host...");
        let verify_cmd = format!(
            "sha256sum ~/.craft/staging/{0} 2>/dev/null || shasum -a 256 ~/.craft/staging/{0} 2>/dev/null || openssl dgst -sha256 ~/.craft/staging/{0} 2>/dev/null",
            archive_name
        );
        let (vcode, vout, _) = client.session.exec(&verify_cmd)?;
        if vcode != 0 || vout.trim().is_empty() {
            let _ = client
                .session
                .exec(&format!("rm -f ~/.craft/staging/{}", archive_name));
            return Err(CraftError::Other(format!(
                "Failed to compute SHA-256 checksum on remote host: {}",
                vout
            )));
        }

        let remote_checksum = extract_sha256_from_output(&vout).ok_or_else(|| {
            let _ = client
                .session
                .exec(&format!("rm -f ~/.craft/staging/{}", archive_name));
            CraftError::Other(format!(
                "Could not extract a valid 64-char SHA-256 checksum from remote output: '{}'",
                vout.trim()
            ))
        })?;

        if !remote_checksum.eq_ignore_ascii_case(&checksum) {
            let _ = client
                .session
                .exec(&format!("rm -f ~/.craft/staging/{}", archive_name));
            return Err(CraftError::Other(format!(
                "Integrity verification failed! Remote SHA-256 '{}' does not match local SHA-256 '{}'. Aborting migration.",
                remote_checksum, checksum
            )));
        }

        progress("Extracting server archive on remote host...");
        let extract_cmd = format!(
            "mkdir -p ~/.craft/servers/{0} && ( if [ \"{2}\" = \"zstd\" ]; then if command -v zstd >/dev/null 2>&1; then zstd -dc ~/.craft/staging/{1} | tar -xf - -C ~/.craft/servers/{0}; else tar --zstd -xf ~/.craft/staging/{1} -C ~/.craft/servers/{0}; fi; else tar -xzf ~/.craft/staging/{1} -C ~/.craft/servers/{0}; fi )",
            target_name,
            archive_name,
            if use_zstd { "zstd" } else { "gzip" }
        );
        let (ecode, eout, eerr) = client.session.exec(&extract_cmd)?;
        if ecode != 0 {
            let _ = client
                .session
                .exec(&format!("rm -f ~/.craft/staging/{}", archive_name));
            return Err(CraftError::Other(format!(
                "Failed to extract server on remote host: {} {}",
                eout, eerr
            )));
        }

        progress("Cleaning up staging archive on remote host...");
        let _ = client
            .session
            .exec(&format!("rm -f ~/.craft/staging/{}", archive_name));

        if let Some(port) = options.remote_port {
            progress(&format!("Updating server network port to {} on remote...", port));
            let port_cmd = format!(
                "if [ -f ~/.craft/servers/{0}/server.properties ]; then sed -i 's/^server-port=.*/server-port={1}/' ~/.craft/servers/{0}/server.properties 2>/dev/null || sed -i '' 's/^server-port=.*/server-port={1}/' ~/.craft/servers/{0}/server.properties 2>/dev/null; fi",
                target_name, port
            );
            let _ = client.session.exec(&port_cmd);
        }

        progress("Registering server in remote Craft configuration...");
        let (hcode, hout, _) = client.session.exec("echo $HOME")?;
        let remote_home = if hcode == 0 && !hout.trim().is_empty() {
            hout.trim().to_string()
        } else {
            "~".to_string()
        };
        let remote_server_path = format!("{}/.craft/servers/{}", remote_home, target_name);

        let mut remote_registry = match client.sftp().read_file_to_string(Path::new(".craft/servers.toml")) {
            Ok(c) => toml::from_str::<craft_core::ServersRegistry>(&c).unwrap_or_default(),
            Err(_) => {
                let (cat_code, cat_out, _) = client.session.exec("cat ~/.craft/servers.toml 2>/dev/null")?;
                if cat_code == 0 && !cat_out.trim().is_empty() {
                    toml::from_str::<craft_core::ServersRegistry>(&cat_out).unwrap_or_default()
                } else {
                    craft_core::ServersRegistry::default()
                }
            }
        };

        let mut new_config = source_config.clone();
        new_config.name = target_name.clone();
        new_config.path = PathBuf::from(&remote_server_path);
        if let Some(port) = options.remote_port {
            new_config.port = Some(port);
        }
        remote_registry.servers.retain(|s| !s.name.eq_ignore_ascii_case(&target_name));
        remote_registry.servers.push(new_config);

        let serialized_reg = toml::to_string_pretty(&remote_registry)
            .map_err(|e| CraftError::Other(format!("Failed to serialize remote servers.toml: {}", e)))?;
        client
            .sftp()
            .write_file(Path::new(".craft/servers.toml"), serialized_reg.as_bytes())?;

        let mut started_remote = false;
        if options.start_remote {
            progress("Starting server on remote daemon supervisor...");
            if let Err(e) = client.start_server(&target_name) {
                progress(&format!("[WARN] Remote server start returned: {}", e));
            } else {
                started_remote = true;
            }
        }

        let mut trashed_locally = false;
        if options.trash_source {
            progress("Moving local source server directory to non-destructive trash bin...");
            let trash_mgr = TrashManager::new(paths);
            trash_mgr.trash_path(&source_config.path, Some(&options.source_server))?;
            let _ = ServersRegistry::modify(paths, |reg| {
                reg.remove(&source_config.path);
                Ok(())
            });
            trashed_locally = true;
        }

        Ok(MigrationResult {
            source_name: options.source_server.clone(),
            target_name,
            remote_alias: options.remote_alias.clone(),
            archive_size_bytes,
            checksum,
            remote_path: remote_server_path,
            remote_port: options.remote_port.or(source_config.port),
            trashed_locally,
            started_remote,
        })
    }
}

fn should_exclude_migration(rel: &Path) -> bool {
    let s = rel.to_string_lossy();
    s.ends_with(".server.lock")
        || s.ends_with(".server.pid")
        || s.ends_with("daemon.sock")
        || s.ends_with(".tmp")
}

pub fn pack_server_archive(
    source_dir: &Path,
    output_archive: &Path,
    use_zstd: bool,
) -> Result<(u64, String)> {
    let file = File::create(output_archive)?;
    if use_zstd {
        let enc = zstd::stream::write::Encoder::new(file, 3)
            .map_err(|e| CraftError::Other(format!("Failed to initialize zstd encoder: {}", e)))?;
        let mut tar = tar::Builder::new(enc);
        append_dir_recursive(&mut tar, source_dir, source_dir)?;
        let enc = tar
            .into_inner()
            .map_err(|e| CraftError::Other(format!("Failed to finalize tar archive: {}", e)))?;
        enc.finish()
            .map_err(|e| CraftError::Other(format!("Failed to finalize zstd stream: {}", e)))?;
    } else {
        let enc = GzEncoder::new(file, Compression::default());
        let mut tar = tar::Builder::new(enc);
        append_dir_recursive(&mut tar, source_dir, source_dir)?;
        let enc = tar
            .into_inner()
            .map_err(|e| CraftError::Other(format!("Failed to finalize tar archive: {}", e)))?;
        enc.finish()
            .map_err(|e| CraftError::Other(format!("Failed to finalize gzip stream: {}", e)))?;
    }

    let mut f = File::open(output_archive)?;
    let mut hasher = Sha256::new();
    let mut buffer = [0u8; 64 * 1024];
    let mut size_bytes = 0u64;
    loop {
        let count = f.read(&mut buffer)?;
        if count == 0 {
            break;
        }
        size_bytes += count as u64;
        hasher.update(&buffer[..count]);
    }

    let checksum = hex::encode(hasher.finalize());
    Ok((size_bytes, checksum))
}

fn append_dir_recursive<W: Write>(
    tar: &mut tar::Builder<W>,
    base_dir: &Path,
    current_dir: &Path,
) -> Result<()> {
    for entry in fs::read_dir(current_dir)? {
        let entry = entry?;
        let path = entry.path();
        let rel_path = path
            .strip_prefix(base_dir)
            .map_err(|e| CraftError::Other(e.to_string()))?;

        if should_exclude_migration(rel_path) {
            continue;
        }

        if path.is_dir() {
            tar.append_dir(rel_path, &path)
                .map_err(|e| CraftError::Other(format!("Failed to append dir: {}", e)))?;
            append_dir_recursive(tar, base_dir, &path)?;
        } else if path.is_file() {
            let mut f = File::open(&path)?;
            tar.append_file(rel_path, &mut f)
                .map_err(|e| CraftError::Other(format!("Failed to append file: {}", e)))?;
        }
    }
    Ok(())
}

pub fn extract_sha256_from_output(out: &str) -> Option<String> {
    for line in out.lines() {
        for part in line.split(|c: char| c.is_whitespace() || c == '=' || c == '(' || c == ')') {
            let trimmed = part.trim();
            if trimmed.len() == 64 && trimmed.chars().all(|c| c.is_ascii_hexdigit()) {
                return Some(trimmed.to_lowercase());
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_extract_sha256_from_various_formats() {
        let sha_linux = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  /tmp/test.tar.zst";
        assert_eq!(
            extract_sha256_from_output(sha_linux).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );

        let sha_macos = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855  test.tar.zst";
        assert_eq!(
            extract_sha256_from_output(sha_macos).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );

        let sha_bsd = "SHA256(test.tar.zst)= e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert_eq!(
            extract_sha256_from_output(sha_bsd).unwrap(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );

        let empty = "no hash here";
        assert!(extract_sha256_from_output(empty).is_none());
    }

    #[test]
    fn test_pack_server_archive_and_checksum() {
        let dir = tempdir().unwrap();
        let src_dir = dir.path().join("server");
        fs::create_dir_all(&src_dir).unwrap();
        fs::write(src_dir.join("server.properties"), "server-port=25565\n").unwrap();
        fs::write(src_dir.join(".server.lock"), "locked").unwrap();
        fs::create_dir_all(src_dir.join("world")).unwrap();
        fs::write(src_dir.join("world").join("level.dat"), "test world data").unwrap();

        let archive_path = dir.path().join("archive.tar.zst");
        let (size, checksum) = pack_server_archive(&src_dir, &archive_path, true).unwrap();
        assert!(size > 0);
        assert_eq!(checksum.len(), 64);
        assert!(archive_path.exists());

        // Unpack and verify excluded .server.lock is not present
        let unpack_dir = dir.path().join("unpacked");
        fs::create_dir_all(&unpack_dir).unwrap();
        let dec = zstd::stream::read::Decoder::new(File::open(&archive_path).unwrap()).unwrap();
        let mut tar = tar::Archive::new(dec);
        tar.unpack(&unpack_dir).unwrap();

        assert!(unpack_dir.join("server.properties").exists());
        assert!(unpack_dir.join("world").join("level.dat").exists());
        assert!(!unpack_dir.join(".server.lock").exists());
    }
}
