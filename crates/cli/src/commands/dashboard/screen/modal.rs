use std::io::{self, Write};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};

use craft_core::Result;
use super::terminal::{get_content_width, get_terminal_size};
use super::theme::{box_bottom, box_divider, box_title, box_top, DIM, RESET};
use super::clean_exit;

/// Displays an in-place modal dialog box without leaving the alternate screen.
/// Supports scrolling for messages with many lines.
pub fn show_modal_message<S: AsRef<str>>(title: &str, lines: &[S], is_error: bool) -> Result<()> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    let _ = execute!(stdout, Hide);

    let mut scroll_offset = 0usize;

    let result = (|| -> Result<()> {
        loop {
            let (_, term_h) = get_terminal_size();
            let width = get_content_width(80);

            // Compute available viewport lines for text lines
            let viewport_size = (term_h as usize).saturating_sub(8).max(4);

            if scroll_offset + viewport_size > lines.len() {
                scroll_offset = lines.len().saturating_sub(viewport_size);
            }

            execute!(stdout, MoveTo(0, 0))?;

            print!("{}\x1B[K\r\n", box_top(width));
            print!("{}\x1B[K\r\n", box_title(title, width, is_error));
            print!("{}\x1B[K\r\n", box_divider(width));
            print!("\x1B[K\r\n");

            if scroll_offset > 0 {
                print!("  {}[▲ {} lines above]{}\x1B[K\r\n", DIM, scroll_offset, RESET);
            }

            let end_idx = lines.len().min(scroll_offset + viewport_size);
            for line in &lines[scroll_offset..end_idx] {
                print!("  {}\x1B[K\r\n", line.as_ref());
            }

            if end_idx < lines.len() {
                print!("  {}[▼ {} lines below]{}\x1B[K\r\n", DIM, lines.len() - end_idx, RESET);
            }

            print!("\x1B[K\r\n{}\x1B[K\r\n", box_bottom(width));
            if lines.len() > viewport_size {
                print!("  \x1B[2m[Enter / Space / Esc] Dismiss  |  [↑/↓/PgUp/PgDn] Scroll  |  [q] Quit\x1B[0m\x1B[K\r\n");
            } else {
                print!("  \x1B[2m[Enter / Space / Esc / ← / →] Dismiss  |  [q] Quit\x1B[0m\x1B[K\r\n");
            }

            execute!(stdout, Clear(ClearType::FromCursorDown))?;
            stdout.flush()?;

            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    if (key.modifiers.contains(event::KeyModifiers::CONTROL)
                        && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('C')))
                        || key.code == KeyCode::Char('\x03')
                        || key.code == KeyCode::Char('q')
                        || key.code == KeyCode::Char('Q')
                    {
                        clean_exit();
                    }

                    match key.code {
                        KeyCode::Up | KeyCode::Char('k') => {
                            scroll_offset = scroll_offset.saturating_sub(1);
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if scroll_offset + viewport_size < lines.len() {
                                scroll_offset += 1;
                            }
                        }
                        KeyCode::PageUp => {
                            scroll_offset = scroll_offset.saturating_sub(viewport_size);
                        }
                        KeyCode::PageDown => {
                            scroll_offset = (scroll_offset + viewport_size).min(lines.len().saturating_sub(viewport_size));
                        }
                        KeyCode::Enter
                        | KeyCode::Esc
                        | KeyCode::Char(' ')
                        | KeyCode::Left
                        | KeyCode::Right => break,
                        _ => {}
                    }
                }
            }
        }
        Ok(())
    })();

    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), Show);
    result
}

/// Renders in-place progress information pinned at top of alternate screen.
pub fn print_in_place_status<S: AsRef<str>>(title: &str, lines: &[S]) -> Result<()> {
    let mut stdout = io::stdout();
    let width = get_content_width(80);

    execute!(stdout, MoveTo(0, 0))?;
    print!("{}\x1B[K\r\n", box_top(width));
    print!("{}\x1B[K\r\n", box_title(title, width, false));
    print!("{}\x1B[K\r\n", box_divider(width));
    print!("\x1B[K\r\n");
    for line in lines {
        print!("  {}\x1B[K\r\n", line.as_ref());
    }
    print!("\x1B[K\r\n{}\x1B[K\r\n", box_bottom(width));
    execute!(stdout, Clear(ClearType::FromCursorDown))?;
    stdout.flush()?;
    Ok(())
}
