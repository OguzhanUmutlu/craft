pub mod console;
pub mod frame;
pub mod input;
pub mod keys;
pub mod menu;
pub mod modal;
pub mod modals;
pub mod nav;
pub mod section;
pub mod terminal;
pub mod text_flow;
pub mod theme;

#[allow(unused_imports)]
pub use modalx::*;

#[allow(unused_imports)]
pub use console::*;
#[allow(unused_imports)]
pub use frame::*;
#[allow(unused_imports)]
pub use input::*;
#[allow(unused_imports)]
pub use keys::*;
#[allow(unused_imports)]
pub use menu::*;
#[allow(unused_imports)]
pub use modal::*;
#[allow(unused_imports)]
pub use modals::*;
#[allow(unused_imports)]
pub use nav::*;
#[allow(unused_imports)]
pub use section::*;
#[allow(unused_imports)]
pub use terminal::*;
#[allow(unused_imports)]
pub use text_flow::*;
#[allow(unused_imports)]
pub use theme::*;

use crossterm::{
    cursor::{Hide, MoveTo, Show},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType, LeaveAlternateScreen},
};
use std::io;

use craft_core::Result;

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
    REMOTE_NODE_NAME
        .lock()
        .ok()
        .and_then(|rn| rn.clone())
        .is_some()
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

    if is_alt_screen_active() {
        restore_alt_screen_if_active();
    } else {
        let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
    }
    let _ = enable_raw_mode();
    let _ = execute!(io::stdout(), Hide);
    res
}
