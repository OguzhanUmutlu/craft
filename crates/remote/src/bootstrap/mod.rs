pub mod linux;
pub mod macos;
pub mod windows;

use crate::session::RemoteSession;
use craft_core::{RemoteOsType, Result};

pub fn run_bootstrap(session: &RemoteSession) -> Result<()> {
    run_bootstrap_with_progress(session, |msg| println!("{}", msg))
}

pub fn run_bootstrap_with_progress(
    session: &RemoteSession,
    mut on_progress: impl FnMut(&str),
) -> Result<()> {
    let os = session.probe_os()?;
    on_progress(&format!("Detected remote OS: {}", os));

    match os {
        RemoteOsType::Linux => linux::bootstrap_linux(session, &mut on_progress),
        RemoteOsType::MacOS => macos::bootstrap_macos(session, &mut on_progress),
        RemoteOsType::Windows => windows::bootstrap_windows(session, &mut on_progress),
    }
}
