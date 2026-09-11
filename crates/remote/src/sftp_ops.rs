use std::fs::File;
use std::io::{Read, Write};
use std::path::Path;
use indicatif::{ProgressBar, ProgressStyle};
use craft_core::{CraftError, Result};
use crate::session::RemoteSession;

pub struct SftpOps<'a> {
    pub session: &'a RemoteSession,
}

impl<'a> SftpOps<'a> {
    pub fn new(session: &'a RemoteSession) -> Self {
        Self { session }
    }

    pub fn upload_file(&self, local_path: &Path, remote_path: &Path) -> Result<()> {
        let sftp = self.session.sftp()?;
        let mut local_file = File::open(local_path)
            .map_err(CraftError::Io)?;

        let meta = local_file.metadata().map_err(CraftError::Io)?;
        let total_size = meta.len();

        let pb = ProgressBar::new(total_size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, ETA: {eta})")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.set_message(format!("Uploading {}", local_path.file_name().unwrap_or_default().to_string_lossy()));

        let mut remote_file = sftp.create(remote_path)
            .map_err(|e| CraftError::Other(format!("Failed to create remote file '{}': {}", remote_path.display(), e)))?;

        let mut buf = [0u8; 64 * 1024];
        loop {
            let count = local_file.read(&mut buf).map_err(CraftError::Io)?;
            if count == 0 {
                break;
            }
            remote_file.write_all(&buf[..count])
                .map_err(|e| CraftError::Other(format!("SFTP write error: {}", e)))?;
            pb.inc(count as u64);
        }

        pb.finish_with_message("Upload complete");
        Ok(())
    }

    pub fn download_file(&self, remote_path: &Path, local_path: &Path) -> Result<()> {
        let sftp = self.session.sftp()?;
        let mut remote_file = sftp.open(remote_path)
            .map_err(|e| CraftError::Other(format!("Failed to open remote file '{}': {}", remote_path.display(), e)))?;

        let stat = remote_file.stat()
            .map_err(|e| CraftError::Other(format!("Failed to stat remote file: {}", e)))?;
        let total_size = stat.size.unwrap_or(0);

        let pb = ProgressBar::new(total_size);
        pb.set_style(
            ProgressStyle::default_bar()
                .template("{msg} [{bar:40.cyan/blue}] {bytes}/{total_bytes} ({bytes_per_sec}, ETA: {eta})")
                .unwrap()
                .progress_chars("#>-"),
        );
        pb.set_message(format!("Downloading {}", remote_path.file_name().unwrap_or_default().to_string_lossy()));

        if let Some(parent) = local_path.parent() {
            std::fs::create_dir_all(parent).map_err(CraftError::Io)?;
        }

        let mut local_file = File::create(local_path).map_err(CraftError::Io)?;
        let mut buf = [0u8; 64 * 1024];

        loop {
            let count = remote_file.read(&mut buf)
                .map_err(|e| CraftError::Other(format!("SFTP read error: {}", e)))?;
            if count == 0 {
                break;
            }
            local_file.write_all(&buf[..count]).map_err(CraftError::Io)?;
            pb.inc(count as u64);
        }

        pb.finish_with_message("Download complete");
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
}
