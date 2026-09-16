use crossterm::{cursor::Show, execute, terminal::{disable_raw_mode, size, LeaveAlternateScreen}};
use std::io;
use std::sync::Once;

pub const MIN_TERM_WIDTH: u16 = 60;
pub const MIN_TERM_HEIGHT: u16 = 14;

/// Returns current terminal dimensions (width, height), defaulting to (80, 24) on error.
pub fn get_terminal_size() -> (u16, u16) {
    size().unwrap_or((80, 24))
}

/// Returns true if the terminal window is smaller than the minimum dimensions needed for Craft TUI.
pub fn is_terminal_too_small() -> bool {
    let (w, h) = get_terminal_size();
    w < MIN_TERM_WIDTH || h < MIN_TERM_HEIGHT
}

/// If the terminal window is smaller than MIN_TERM_WIDTH x MIN_TERM_HEIGHT,
/// enters a blocking event loop rendering the centered "TERMINAL TOO SMALL" screen
/// until the user resizes the terminal to valid dimensions or presses 'q'/Ctrl+C to quit.
pub fn wait_for_valid_size(stdout: &mut io::Stdout) -> craft_core::Result<(u16, u16)> {
    use std::io::Write;
    let (mut w, mut h) = get_terminal_size();
    if w >= MIN_TERM_WIDTH && h >= MIN_TERM_HEIGHT {
        return Ok((w, h));
    }

    loop {
        execute!(stdout, crossterm::cursor::MoveTo(0, 0))?;
        let output = super::frame::render_too_small(w, h, MIN_TERM_WIDTH, MIN_TERM_HEIGHT);
        for line in output.lines() {
            print!("{}\x1B[K\r\n", line);
        }
        execute!(stdout, crossterm::terminal::Clear(crossterm::terminal::ClearType::FromCursorDown))?;
        stdout.flush()?;

        match crossterm::event::read()? {
            crossterm::event::Event::Resize(new_w, new_h) => {
                w = new_w;
                h = new_h;
                if w >= MIN_TERM_WIDTH && h >= MIN_TERM_HEIGHT {
                    return Ok((w, h));
                }
            }
            crossterm::event::Event::Key(key) => {
                if key.kind == crossterm::event::KeyEventKind::Press
                    && ((key.modifiers.contains(crossterm::event::KeyModifiers::CONTROL)
                        && (key.code == crossterm::event::KeyCode::Char('c') || key.code == crossterm::event::KeyCode::Char('C')))
                        || key.code == crossterm::event::KeyCode::Char('\x03')
                        || key.code == crossterm::event::KeyCode::Char('q')
                        || key.code == crossterm::event::KeyCode::Char('Q'))
                {
                    super::clean_exit();
                }
                let (cur_w, cur_h) = get_terminal_size();
                w = cur_w;
                h = cur_h;
                if w >= MIN_TERM_WIDTH && h >= MIN_TERM_HEIGHT {
                    return Ok((w, h));
                }
            }
            _ => {}
        }
    }
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
