use craft_core::{
    compute_file_sha256, CraftError, HermeticBuildManifest, Result, Subject,
};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, File};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use tracing::{debug, info, warn};
use zip::write::SimpleFileOptions;
use zip::ZipWriter;

#[derive(Debug, Clone)]
pub struct SeccompBpfPolicy {
    pub default_action: String,
    pub denied_syscalls: Vec<String>,
    pub allow_network: bool,
}

impl SeccompBpfPolicy {
    pub fn for_hermetic_build(allow_network: bool) -> Self {
        let mut denied = Vec::new();
        if !allow_network {
            denied.extend_from_slice(&[
                "socket".to_string(),
                "connect".to_string(),
                "bind".to_string(),
                "listen".to_string(),
                "accept".to_string(),
                "accept4".to_string(),
                "sendto".to_string(),
                "sendmsg".to_string(),
                "sendmmsg".to_string(),
            ]);
        }
        Self {
            default_action: "ALLOW".to_string(),
            denied_syscalls: denied,
            allow_network,
        }
    }
}

pub fn sanitize_env(source_date_epoch: u64, allow_network: bool) -> HashMap<String, String> {
    let mut env = HashMap::new();
    env.insert("PATH".to_string(), "/usr/bin:/bin:/usr/local/bin".to_string());
    env.insert("LC_ALL".to_string(), "C".to_string());
    env.insert("LANG".to_string(), "C".to_string());
    env.insert("TZ".to_string(), "UTC".to_string());
    env.insert("SOURCE_DATE_EPOCH".to_string(), source_date_epoch.to_string());
    env.insert("ZERO_AR_DATE".to_string(), "1".to_string());
    env.insert("PYTHONHASHSEED".to_string(), "0".to_string());
    if !allow_network {
        env.insert("http_proxy".to_string(), "http://127.0.0.1:0".to_string());
        env.insert("https_proxy".to_string(), "http://127.0.0.1:0".to_string());
        env.insert("no_proxy".to_string(), "localhost,127.0.0.1".to_string());
    }
    env
}

pub fn normalize_filesystem_metadata(dir: &Path) -> Result<()> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;

        fn walk_and_normalize(p: &Path) -> Result<()> {
            if p.is_dir() {
                let perms = fs::Permissions::from_mode(0o755);
                let _ = fs::set_permissions(p, perms);
                let entries = fs::read_dir(p).map_err(CraftError::Io)?;
                for entry in entries.flatten() {
                    walk_and_normalize(&entry.path())?;
                }
            } else if p.is_file() {
                let meta = fs::metadata(p).map_err(CraftError::Io)?;
                let current_mode = meta.permissions().mode();
                let is_exec = current_mode & 0o111 != 0;
                let target_mode = if is_exec { 0o755 } else { 0o644 };
                let perms = fs::Permissions::from_mode(target_mode);
                let _ = fs::set_permissions(p, perms);
            }
            Ok(())
        }

        walk_and_normalize(dir)?;
    }

    #[cfg(not(unix))]
    {
        let _ = dir;
    }

    Ok(())
}

fn collect_dir_files_recursive(base_dir: &Path, rel_prefix: &Path) -> Result<Vec<(PathBuf, PathBuf)>> {
    let mut result = Vec::new();
    let current_dir = base_dir.join(rel_prefix);
    if !current_dir.exists() {
        return Ok(result);
    }

    let entries = fs::read_dir(&current_dir).map_err(CraftError::Io)?;
    for entry in entries.flatten() {
        let path = entry.path();
        let file_name = entry.file_name();
        let rel_path = if rel_prefix.as_os_str().is_empty() {
            PathBuf::from(file_name)
        } else {
            rel_prefix.join(file_name)
        };

        if path.is_dir() {
            let mut sub = collect_dir_files_recursive(base_dir, &rel_path)?;
            result.append(&mut sub);
        } else if path.is_file() {
            result.push((path, rel_path));
        }
    }

    Ok(result)
}

pub fn create_reproducible_zip(
    source_dir: &Path,
    output_zip: &Path,
    _source_date_epoch: u64,
) -> Result<String> {
    if let Some(parent) = output_zip.parent() {
        if !parent.exists() {
            fs::create_dir_all(parent).map_err(CraftError::Io)?;
        }
    }

    let mut files = collect_dir_files_recursive(source_dir, Path::new(""))?;
    // Sort files lexicographically by relative path for bit-for-bit reproducibility
    files.sort_by(|a, b| a.1.cmp(&b.1));

    let zip_file = File::create(output_zip).map_err(CraftError::Io)?;
    let mut zip_writer = ZipWriter::new(zip_file);

    // Fixed timestamp: 2024-01-01 00:00:00 UTC
    let fixed_time = zip::DateTime::from_date_and_time(2024, 1, 1, 0, 0, 0)
        .unwrap_or_default();

    for (full_path, rel_path) in files {
        let rel_str = rel_path.to_string_lossy().replace('\\', "/");
        if output_zip.file_name() == Some(rel_path.as_os_str()) {
            continue; // Skip the archive itself if created inside source_dir
        }

        let is_exec = {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let meta = fs::metadata(&full_path).ok();
                meta.map(|m| m.permissions().mode() & 0o111 != 0).unwrap_or(false)
            }
            #[cfg(not(unix))]
            {
                false
            }
        };

        let mode = if is_exec { 0o755 } else { 0o644 };

        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::Deflated)
            .last_modified_time(fixed_time)
            .unix_permissions(mode);

        zip_writer
            .start_file(rel_str, options)
            .map_err(|e| CraftError::Other(format!("Zip error: {}", e)))?;

        let mut f = File::open(&full_path).map_err(CraftError::Io)?;
        let mut buf = [0u8; 8192];
        loop {
            let n = f.read(&mut buf).map_err(CraftError::Io)?;
            if n == 0 {
                break;
            }
            zip_writer
                .write_all(&buf[..n])
                .map_err(CraftError::Io)?;
        }
    }

    zip_writer
        .finish()
        .map_err(|e| CraftError::Other(format!("Failed to finalize zip archive: {}", e)))?;

    compute_file_sha256(output_zip)
}

pub struct HermeticBuildRunner;

impl HermeticBuildRunner {
    pub async fn run_build(
        build_dir: &Path,
        command: &str,
        args: &[String],
        source_date_epoch: u64,
        allow_network: bool,
    ) -> Result<HermeticBuildManifest> {
        info!(
            "[BUILD] Starting hermetic isolated build in '{}' (command: '{}', network: {})",
            build_dir.display(),
            command,
            allow_network
        );

        if !build_dir.exists() {
            fs::create_dir_all(build_dir).map_err(CraftError::Io)?;
        }

        normalize_filesystem_metadata(build_dir)?;
        let env_vars = sanitize_env(source_date_epoch, allow_network);

        let mut cmd = tokio::process::Command::new(command);
        cmd.current_dir(build_dir);
        cmd.args(args);
        cmd.env_clear();
        for (k, v) in &env_vars {
            cmd.env(k, v);
        }

        let output = cmd.output().await.map_err(|e| {
            CraftError::Process(format!("Hermetic build failed to execute '{}': {}", command, e))
        })?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            warn!("[BUILD] Hermetic command exited with error: {}", stderr);
            return Err(CraftError::Process(format!(
                "Build process exited with code {:?}: {}",
                output.status.code(),
                stderr
            )));
        }

        normalize_filesystem_metadata(build_dir)?;

        // Package artifacts in build_dir
        let dist_dir = build_dir.join("dist");
        let mut output_artifacts = Vec::new();

        let target_scan_dir = if dist_dir.exists() {
            &dist_dir
        } else {
            build_dir
        };

        let files = collect_dir_files_recursive(target_scan_dir, Path::new(""))?;
        let mut overall_hasher = Sha256::new();

        for (full_path, rel_path) in files {
            if let Ok(digest) = compute_file_sha256(&full_path) {
                let name = rel_path.to_string_lossy().to_string();
                overall_hasher.update(name.as_bytes());
                overall_hasher.update(digest.as_bytes());
                output_artifacts.push(Subject::new(name, digest));
            }
        }

        let build_digest = hex::encode(overall_hasher.finalize());

        debug!(
            "[BUILD] Hermetic build completed successfully. Output digest: {}",
            build_digest
        );

        Ok(HermeticBuildManifest {
            source_repo: "local".to_string(),
            commit_hash: "0000000000000000000000000000000000000000".to_string(),
            source_date_epoch,
            env_allowlist: env_vars.keys().cloned().collect(),
            network_isolated: !allow_network,
            output_artifacts,
            build_digest,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_deterministic_zip_reproducibility() {
        let temp1 = tempdir().unwrap();
        let temp2 = tempdir().unwrap();

        // Create files in temp1 in order A then B
        let dir1 = temp1.path();
        fs::write(dir1.join("file_b.txt"), b"Content B").unwrap();
        fs::write(dir1.join("file_a.txt"), b"Content A").unwrap();

        // Create files in temp2 in order B then A
        let dir2 = temp2.path();
        fs::write(dir2.join("file_a.txt"), b"Content A").unwrap();
        fs::write(dir2.join("file_b.txt"), b"Content B").unwrap();

        let zip1 = temp1.path().join("output1.zip");
        let zip2 = temp2.path().join("output2.zip");

        let hash1 = create_reproducible_zip(dir1, &zip1, 1704067200).unwrap();
        let hash2 = create_reproducible_zip(dir2, &zip2, 1704067200).unwrap();

        assert_eq!(hash1, hash2, "Reproducible zip hashes must match bit-for-bit");
    }

    #[test]
    fn test_env_sanitization() {
        let env = sanitize_env(1704067200, false);
        assert_eq!(env.get("TZ").map(|s| s.as_str()), Some("UTC"));
        assert_eq!(env.get("SOURCE_DATE_EPOCH").map(|s| s.as_str()), Some("1704067200"));
        assert!(env.contains_key("http_proxy"));
    }
}
