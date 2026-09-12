use std::fs;
use std::path::Path;
#[cfg(target_os = "windows")]
use sysinfo::{Pid, System};
use crate::error::{CraftError, Result};

pub fn is_process_running(pid: u32) -> bool {
    #[cfg(target_os = "windows")]
    {
        let s = System::new_all();
        s.process(Pid::from_u32(pid)).is_some()
    }

    #[cfg(not(target_os = "windows"))]
    {
        unsafe {
            // kill(pid, 0) checks if process exists without sending a signal
            libc::kill(pid as i32, 0) == 0
        }
    }
}

pub fn kill_process(pid: u32, force: bool) -> Result<()> {
    #[cfg(target_os = "windows")]
    {
        let mut cmd = std::process::Command::new("taskkill");
        cmd.arg("/PID").arg(pid.to_string());
        if force {
            cmd.arg("/F");
        }
        cmd.arg("/T"); // kill child processes too
        let status = cmd.status().map_err(|e| CraftError::Process(format!("taskkill failed: {}", e)))?;
        if status.success() {
            Ok(())
        } else {
            Err(CraftError::Process(format!("taskkill exited with non-zero status for PID {}", pid)))
        }
    }

    #[cfg(not(target_os = "windows"))]
    {
        let signal = if force { libc::SIGKILL } else { libc::SIGTERM };
        let ret = unsafe { libc::kill(pid as i32, signal) };
        if ret == 0 {
            Ok(())
        } else {
            Err(CraftError::Process(format!(
                "Failed to send signal {} to PID {}: errno {}",
                signal,
                pid,
                std::io::Error::last_os_error()
            )))
        }
    }
}

pub fn read_pid_file<P: AsRef<Path>>(path: P) -> Option<u32> {
    let content = fs::read_to_string(path.as_ref()).ok()?;
    content.trim().parse::<u32>().ok()
}

pub fn write_pid_file<P: AsRef<Path>>(path: P, pid: u32) -> Result<()> {
    fs::write(path.as_ref(), pid.to_string())
        .map_err(CraftError::Io)
}

pub fn remove_pid_file<P: AsRef<Path>>(path: P) {
    let _ = fs::remove_file(path.as_ref());
}

/// If the expected server jar (defaulting to "server.jar") does not exist in `server_dir`,
/// searches for any matching jar file (e.g. `paper-*.jar`, `purpur-*.jar`, etc.)
/// and copies it to the expected jar filename.
/// Returns Some(source_filename) if a repair was performed.
pub fn auto_heal_server_jar<P: AsRef<Path>>(server_dir: P) -> Option<String> {
    auto_heal_server_file(server_dir, "server.jar")
}

/// If `target_filename` does not exist in `server_dir`,
/// searches for files with the same extension (excluding installer jars / wrappers)
/// and copies the largest matching file to `target_filename`.
/// Returns Some(source_filename) if a repair was performed.
pub fn auto_heal_server_file<P: AsRef<Path>>(server_dir: P, target_filename: &str) -> Option<String> {
    let dir = server_dir.as_ref();
    let target = dir.join(target_filename);
    if target.exists() {
        return None;
    }

    let ext = Path::new(target_filename)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("jar");

    let entries = fs::read_dir(dir).ok()?;
    let mut candidates: Vec<(std::path::PathBuf, u64)> = Vec::new();

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let file_name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n,
            None => continue,
        };

        if file_name.eq_ignore_ascii_case(target_filename) {
            continue;
        }

        // Must match extension
        if let Some(entry_ext) = path.extension().and_then(|e| e.to_str()) {
            if !entry_ext.eq_ignore_ascii_case(ext) {
                continue;
            }
        } else {
            continue;
        }

        let lower = file_name.to_ascii_lowercase();
        // Exclude installer and start wrappers
        if lower.contains("installer") || lower.starts_with("start.") {
            continue;
        }

        let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
        candidates.push((path, size));
    }

    // Pick candidate with largest file size (actual server archive is largest)
    candidates.sort_by_key(|(_, size)| std::cmp::Reverse(*size));

    if let Some((best_path, _)) = candidates.first() {
        if fs::copy(best_path, &target).is_ok() {
            return best_path.file_name().and_then(|n| n.to_str()).map(|s| s.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_auto_heal_server_jar() {
        let dir = tempdir().unwrap();
        let paper_jar = dir.path().join("paper-1.9.4-775.jar");
        fs::write(&paper_jar, b"PK0304fakejar").unwrap();

        assert!(!dir.path().join("server.jar").exists());
        let restored = auto_heal_server_jar(dir.path());
        assert_eq!(restored, Some("paper-1.9.4-775.jar".to_string()));
        assert!(dir.path().join("server.jar").exists());

        // Second call should do nothing since server.jar already exists
        assert_eq!(auto_heal_server_jar(dir.path()), None);
    }

    #[test]
    fn test_auto_heal_excludes_installers() {
        let dir = tempdir().unwrap();
        let installer = dir.path().join("neoforge-installer.jar");
        fs::write(&installer, b"installer_content").unwrap();

        assert_eq!(auto_heal_server_jar(dir.path()), None);
        assert!(!dir.path().join("server.jar").exists());
    }
}
