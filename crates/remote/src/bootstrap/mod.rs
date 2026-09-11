pub mod linux;
pub mod macos;
pub mod windows;

use colored::Colorize;
use craft_core::{RemoteOsType, Result};
use crate::session::RemoteSession;

pub fn run_bootstrap(session: &RemoteSession) -> Result<()> {
    let os = session.probe_os()?;
    println!("{}", format!("Detected remote OS: {}", os).green().bold());

    match os {
        RemoteOsType::Linux => linux::bootstrap_linux(session),
        RemoteOsType::MacOS => macos::bootstrap_macos(session),
        RemoteOsType::Windows => windows::bootstrap_windows(session),
    }
}
