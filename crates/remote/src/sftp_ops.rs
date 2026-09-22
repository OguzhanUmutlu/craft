use crate::session::RemoteSession;
use craft_core::{CraftError, Result};
use modalx::modals::ProgressModal;
use std::fs::File;
use std::io::{self, Read, Write};
use std::path::Path;

pub struct SftpOps<'a> {
    pub session: &'a RemoteSession,
}

impl<'a> SftpOps<'a> {
    pub fn new(session: &'a RemoteSession) -> Self {
        Self { session }
    }

    pub fn upload_file(&self, local_path: &Path, remote_path: &Path) -> Result<()> {
        let sftp = self.session.sftp()?;
        let mut local_file = File::open(local_path).map_err(CraftError::Io)?;

        let meta = local_file.metadata().map_err(CraftError::Io)?;
        let total_size = meta.len();

        let mut stdout = io::stdout();
        let name = local_path.file_name().unwrap_or_default().to_string_lossy();
        let mut modal = ProgressModal::new(
            "UPLOADING FILE",
            format!("Uploading {} to remote host...", name),
            total_size,
        );
        let _ = modal.render_forced(&mut stdout);

        let mut remote_file = sftp.create(remote_path).map_err(|e| {
            CraftError::Other(format!(
                "Failed to create remote file '{}': {}",
                remote_path.display(),
                e
            ))
        })?;

        let mut buf = [0u8; 64 * 1024];
        loop {
            let count = local_file.read(&mut buf).map_err(CraftError::Io)?;
            if count == 0 {
                break;
            }
            remote_file
                .write_all(&buf[..count])
                .map_err(|e| CraftError::Other(format!("SFTP write error: {}", e)))?;
            modal.inc(count as u64);
            let _ = modal.render(&mut stdout);
        }

        let _ = modal.finish("Upload complete", &mut stdout);
        Ok(())
    }

    pub fn download_file(&self, remote_path: &Path, local_path: &Path) -> Result<()> {
        let sftp = self.session.sftp()?;
        let mut remote_file = sftp.open(remote_path).map_err(|e| {
            CraftError::Other(format!(
                "Failed to open remote file '{}': {}",
                remote_path.display(),
                e
            ))
        })?;

        let stat = remote_file
            .stat()
            .map_err(|e| CraftError::Other(format!("Failed to stat remote file: {}", e)))?;
        let total_size = stat.size.unwrap_or(0);

        let mut stdout = io::stdout();
        let name = remote_path
            .file_name()
            .unwrap_or_default()
            .to_string_lossy();
        let mut modal = if total_size > 0 {
            ProgressModal::new(
                "DOWNLOADING FILE",
                format!("Downloading {} from remote host...", name),
                total_size,
            )
        } else {
            ProgressModal::indeterminate(
                "DOWNLOADING FILE",
                format!("Downloading {} from remote host...", name),
            )
        };
        let _ = modal.render_forced(&mut stdout);

        if let Some(parent) = local_path.parent() {
            std::fs::create_dir_all(parent).map_err(CraftError::Io)?;
        }

        let mut local_file = File::create(local_path).map_err(CraftError::Io)?;
        let mut buf = [0u8; 64 * 1024];

        loop {
            let count = remote_file
                .read(&mut buf)
                .map_err(|e| CraftError::Other(format!("SFTP read error: {}", e)))?;
            if count == 0 {
                break;
            }
            local_file
                .write_all(&buf[..count])
                .map_err(CraftError::Io)?;
            modal.inc(count as u64);
            let _ = modal.render(&mut stdout);
        }

        let _ = modal.finish("Download complete", &mut stdout);
        Ok(())
    }

    pub fn create_remote_dir_all(&self, remote_path: &Path) -> Result<()> {
        let sftp = self.session.sftp()?;
        let mut current = std::path::PathBuf::new();
        for component in remote_path.components() {
            current.push(component);
            let _ = sftp.mkdir(&current, 0o755);
        }
        Ok(())
    }

    pub fn remote_exists(&self, remote_path: &Path) -> bool {
        if let Ok(sftp) = self.session.sftp() {
            sftp.stat(remote_path).is_ok()
        } else {
            false
        }
    }

    pub fn read_file_to_string(&self, remote_path: &Path) -> Result<String> {
        let sftp = self.session.sftp()?;
        let mut file = sftp.open(remote_path).map_err(|e| {
            CraftError::Other(format!(
                "Failed to open remote file '{}': {}",
                remote_path.display(),
                e
            ))
        })?;
        let mut content = String::new();
        file.read_to_string(&mut content)
            .map_err(|e| CraftError::Other(format!("Failed to read remote file: {}", e)))?;
        Ok(content)
    }

    pub fn write_file(&self, remote_path: &Path, data: &[u8]) -> Result<()> {
        let sftp = self.session.sftp()?;
        let mut file = sftp.create(remote_path).map_err(|e| {
            CraftError::Other(format!(
                "Failed to create remote file '{}': {}",
                remote_path.display(),
                e
            ))
        })?;
        file.write_all(data)
            .map_err(|e| CraftError::Other(format!("Failed to write remote file: {}", e)))?;
        Ok(())
    }

    pub fn list_dir(&self, remote_path: &Path) -> Result<Vec<(String, ssh2::FileStat)>> {
        let sftp = self.session.sftp()?;
        let entries = sftp.readdir(remote_path).map_err(|e| {
            CraftError::Other(format!(
                "Failed to readdir '{}': {}",
                remote_path.display(),
                e
            ))
        })?;
        let mut result = Vec::new();
        for (path, stat) in entries {
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name != "." && name != ".." {
                    result.push((name.to_string(), stat));
                }
            }
        }
        Ok(result)
    }
}
