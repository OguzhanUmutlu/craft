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
        .map_err(|e| CraftError::Io(e))
}

pub fn remove_pid_file<P: AsRef<Path>>(path: P) {
    let _ = fs::remove_file(path.as_ref());
}
