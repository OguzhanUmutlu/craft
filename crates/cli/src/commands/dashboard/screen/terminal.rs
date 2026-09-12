use crossterm::{cursor::Show, execute, terminal::{disable_raw_mode, size, LeaveAlternateScreen}};
use std::io;
use std::sync::Once;

/// Returns current terminal dimensions (width, height), defaulting to (80, 24) on error.
pub fn get_terminal_size() -> (u16, u16) {
    size().unwrap_or((80, 24))
}

/// Calculates a responsive content width clamped between a minimum of 40 and `max_desired`.
pub fn get_content_width(max_desired: u16) -> usize {
    let (term_w, _) = get_terminal_size();
    if term_w < 44 {
        term_w.saturating_sub(2).max(20) as usize
    } else {
        let available = term_w.saturating_sub(2);
        available.min(max_desired) as usize
    }
}

/// Registers a process-wide panic hook ensuring raw mode is disabled and the cursor is restored
/// if an unhandled panic occurs while inside the alternate screen.
pub fn init_terminal_panic_hook() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        let original_hook = std::panic::take_hook();
        std::panic::set_hook(Box::new(move |panic_info| {
            let _ = disable_raw_mode();
            let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);
            original_hook(panic_info);
        }));
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_width_clamping() {
        let width = get_content_width(80);
        assert!(width >= 20);
        assert!(width <= 80);
    }
}
