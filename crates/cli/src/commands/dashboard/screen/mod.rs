pub mod terminal;
pub mod theme;
pub mod frame;
pub mod keys;
pub mod modals;
pub mod menu;
pub mod input;
pub mod modal;
pub mod console;
pub mod nav;

#[allow(unused_imports)]
pub use terminal::*;
#[allow(unused_imports)]
pub use theme::*;
#[allow(unused_imports)]
pub use frame::*;
#[allow(unused_imports)]
pub use keys::*;
#[allow(unused_imports)]
pub use modals::*;
#[allow(unused_imports)]
pub use menu::*;
#[allow(unused_imports)]
pub use input::*;
#[allow(unused_imports)]
pub use modal::*;
#[allow(unused_imports)]
pub use console::*;
#[allow(unused_imports)]
pub use nav::*;

use std::io;
use std::sync::atomic::{AtomicUsize, Ordering};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use craft_core::Result;

static ALT_SCREEN_DEPTH: AtomicUsize = AtomicUsize::new(0);
static REMOTE_NODE_NAME: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

/// Sets the remote node presentation context (used when streamed from remote host).
pub fn set_remote_node(name: Option<String>) {
    if let Ok(mut rn) = REMOTE_NODE_NAME.lock() {
        *rn = name;
    }
}

/// Retrieves the active remote node alias, if running in remote node mode.
pub fn get_remote_node() -> Option<String> {
    REMOTE_NODE_NAME.lock().ok().and_then(|rn| rn.clone())
}

/// Returns true if the TUI is running in remote node presentation mode.
pub fn is_remote_node() -> bool {
    REMOTE_NODE_NAME.lock().ok().and_then(|rn| rn.clone()).is_some()
}

/// Cleanly resets the terminal out of alternate screen and raw mode, then exits the process.
pub fn clean_exit() -> ! {
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
    std::process::exit(0);
}

/// Re-entrant RAII guard for the terminal alternate screen.
/// Ensures nested submenus and dialogs do not exit alternate screen prematurely.
pub struct AltScreenGuard;

impl AltScreenGuard {
    pub fn enter() -> Self {
        init_terminal_panic_hook();
        if ALT_SCREEN_DEPTH.fetch_add(1, Ordering::SeqCst) == 0 {
            let _ = execute!(io::stdout(), EnterAlternateScreen, Hide);
        }
        AltScreenGuard
    }
}

impl Drop for AltScreenGuard {
    fn drop(&mut self) {
        if ALT_SCREEN_DEPTH.fetch_sub(1, Ordering::SeqCst) == 1 {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
        }
    }
}

/// Executes an interactive console action (e.g. foreground server, craft view) inside
/// the alternate screen TUI, disabling raw mode during execution and cleanly restoring
/// raw mode and cursor hiding when the session completes.
pub async fn exec_console_action<F, Fut>(action: F) -> Result<()>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    let mut stdout = io::stdout();
    let _ = execute!(stdout, Clear(ClearType::All), MoveTo(0, 0), Show);
    let _ = disable_raw_mode();

    let res = action().await;

    let _ = enable_raw_mode();
    let _ = execute!(io::stdout(), Hide);
    res
}
