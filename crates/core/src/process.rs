use crate::error::{CraftError, Result};
use fs2::FileExt;
use std::fs;
use std::path::{Path, PathBuf};
#[cfg(target_os = "windows")]
use sysinfo::{Pid, System};

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
        let status = cmd
            .status()
            .map_err(|e| CraftError::Process(format!("taskkill failed: {}", e)))?;
        if status.success() {
            Ok(())
        } else {
            Err(CraftError::Process(format!(
                "taskkill exited with non-zero status for PID {}",
                pid
            )))
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
    fs::write(path.as_ref(), pid.to_string()).map_err(CraftError::Io)
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
pub fn auto_heal_server_file<P: AsRef<Path>>(
    server_dir: P,
    target_filename: &str,
) -> Option<String> {
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
        if target.exists() {
            let _ = fs::remove_file(&target);
        }
        if fs::hard_link(best_path, &target).is_ok() || fs::copy(best_path, &target).is_ok() {
            return best_path
                .file_name()
                .and_then(|n| n.to_str())
                .map(|s| s.to_string());
        }
    }

    None
}

/// An RAII guard representing an exclusive process lock on a Minecraft server directory.
/// Ensures only one process (foreground or background) can run the server at any given time.
#[derive(Debug)]
pub struct ServerLockGuard {
    lock_file: fs::File,
    pid_path: PathBuf,
}

impl ServerLockGuard {
    /// Attempts to acquire an exclusive lock for the server at `server_path`.
    /// If the server is already locked by an active process, returns CraftError::Other with the active PID.
    pub fn acquire<P: AsRef<Path>>(server_path: P) -> Result<Self> {
        let dir = server_path.as_ref();
        let lock_path = dir.join(".server.lock");
        let pid_path = dir.join(".server.pid");

        // 1. Open or create .server.lock
        let file = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&lock_path)
            .map_err(CraftError::Io)?;

        // 2. Attempt non-blocking exclusive file lock
        if file.try_lock_exclusive().is_err() {
            let pid_info = read_pid_file(&pid_path)
                .map(|p| format!(" (PID: {})", p))
                .unwrap_or_default();
            return Err(CraftError::Other(format!(
                "Server at '{}' is already running{}. Only one process can run the server at a time.",
                dir.display(),
                pid_info
            )));
        }

        // 3. We acquired the OS lock. Now check if an orphaned process from a dead parent is still alive
        if let Some(existing_pid) = read_pid_file(&pid_path) {
            if is_process_running(existing_pid) {
                return Err(CraftError::Other(format!(
                    "Server at '{}' is already running (PID: {}). Only one process can run the server at a time.",
                    dir.display(),
                    existing_pid
                )));
            } else {
                // Stale pid file from an unclean shutdown or killed process
                remove_pid_file(&pid_path);
            }
        }

        let guard = Self {
            lock_file: file,
            pid_path,
        };
        let _ = guard.record_pid(std::process::id());
        Ok(guard)
    }

    /// Records the actual child server PID into the lock file
    pub fn record_pid(&self, pid: u32) -> Result<()> {
        write_pid_file(&self.pid_path, pid)
    }
}

impl Drop for ServerLockGuard {
    fn drop(&mut self) {
        remove_pid_file(&self.pid_path);
        let _ = self.lock_file.unlock();
    }
}

/// Returns the running PID of a server if it is active, or None.
pub fn get_server_running_pid<P: AsRef<Path>>(server_path: P) -> Option<u32> {
    let dir = server_path.as_ref();
    let pid_path = dir.join(".server.pid");
    if let Some(pid) = read_pid_file(&pid_path) {
        if is_process_running(pid) {
            return Some(pid);
        }
    }
    None
}

/// Checks whether a server at `server_path` is currently running by inspecting PID and lock file.
pub fn is_server_locked<P: AsRef<Path>>(server_path: P) -> bool {
    let dir = server_path.as_ref();
    if get_server_running_pid(dir).is_some() {
        return true;
    }

    let lock_path = dir.join(".server.lock");
    if lock_path.exists() {
        if let Ok(file) = fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&lock_path)
        {
            if file.try_lock_exclusive().is_err() {
                return true;
            }
        }
    }
    false
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

    #[test]
    fn test_server_lock_mutual_exclusion() {
        let dir = tempdir().unwrap();
        let lock1 = ServerLockGuard::acquire(dir.path()).expect("first lock should succeed");
        lock1.record_pid(999999).unwrap();

        // Second lock attempt in same directory while lock1 is held must fail
        let lock2 = ServerLockGuard::acquire(dir.path());
        assert!(lock2.is_err());
        let err_msg = lock2.unwrap_err().to_string();
        assert!(err_msg.contains("already running"));
        assert!(err_msg.contains("999999"));

        // Dropping lock1 releases the lock
        drop(lock1);

        // Now lock can be acquired again
        let lock3 = ServerLockGuard::acquire(dir.path());
        assert!(lock3.is_ok());
    }

    #[test]
    fn test_server_lock_cleans_stale_pid() {
        let dir = tempdir().unwrap();
        let pid_file = dir.path().join(".server.pid");
        // PID 99999999 is extraordinarily unlikely to be running
        fs::write(&pid_file, "99999999").unwrap();

        let lock = ServerLockGuard::acquire(dir.path());
        assert!(lock.is_ok());
        // Stale pid was replaced with current process PID
        assert_eq!(read_pid_file(&pid_file), Some(std::process::id()));
        drop(lock);
        // Cleaned on drop
        assert!(!pid_file.exists());
    }
}
