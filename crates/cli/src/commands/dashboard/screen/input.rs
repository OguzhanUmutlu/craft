use std::io::{self, Write};
use colored::Colorize;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};

use craft_core::Result;
use super::terminal::{get_content_width, get_terminal_size};
use super::theme::{box_bottom, box_divider, box_title_simple, box_top, DIM, RESET};
use super::clean_exit;

/// Displays an in-place single-line input prompt with full readline editing support.
pub fn run_input_prompt(
    header_title: &str,
    prompt_label: &str,
    default_val: Option<&str>,
) -> Result<Option<String>> {
    prompt_internal(header_title, prompt_label, default_val, false)
}

/// Displays an in-place single-line password input prompt where characters are masked as `*`.
#[allow(dead_code)]
pub fn run_password_prompt(
    header_title: &str,
    prompt_label: &str,
) -> Result<Option<String>> {
    prompt_internal(header_title, prompt_label, None, true)
}

fn prompt_internal(
    header_title: &str,
    prompt_label: &str,
    default_val: Option<&str>,
    is_password: bool,
) -> Result<Option<String>> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;

    let mut input_buffer = if is_password {
        String::new()
    } else {
        default_val.unwrap_or("").to_string()
    };
    let mut cursor_pos = input_buffer.len();

    let result = (|| -> Result<Option<String>> {
        loop {
            let (term_w, _) = get_terminal_size();
            let width = get_content_width(80);

            execute!(stdout, MoveTo(0, 0))?;
            let mut current_y = 0u16;

            print!("{}\x1B[K\r\n", box_top(width));
            current_y += 1;
            print!("{}\x1B[K\r\n", box_title_simple(header_title, width, false));
            current_y += 1;
            print!("{}\x1B[K\r\n", box_divider(width));
            current_y += 1;
            print!("\x1B[K\r\n");
            current_y += 1;

            for line in prompt_label.lines() {
                print!("  {}\x1B[K\r\n", line.white().bold());
                current_y += 1;
            }
            print!("\x1B[K\r\n");
            current_y += 1;

            let cursor_y = current_y;
            let prefix = "  > ";
            let prefix_len = prefix.len();
            let max_display_len = width.saturating_sub(prefix_len + 4);

            let raw_display = if is_password {
                "*".repeat(input_buffer.len())
            } else if input_buffer.is_empty() {
                default_val.map(|d| format!("{}{}{}", DIM, d, RESET)).unwrap_or_default()
            } else {
                input_buffer.clone()
            };

            // Calculate horizontal scrolling slice if input exceeds box width
            let display_str = if input_buffer.len() > max_display_len && !input_buffer.is_empty() {
                let start = cursor_pos.saturating_sub(max_display_len.saturating_sub(4));
                let end = (start + max_display_len).min(input_buffer.len());
                if is_password {
                    "*".repeat(end - start)
                } else {
                    input_buffer[start..end].to_string()
                }
            } else {
                raw_display
            };

            print!("{}{}\x1B[K\r\n", prefix.cyan().bold(), display_str);

            print!("\x1B[K\r\n{}\x1B[K\r\n", box_bottom(width));
            if term_w < 70 {
                print!("  \x1B[2m[Enter] Confirm  |  [Esc] Cancel  |  [Ctrl+C] Quit\x1B[0m\x1B[K\r\n");
            } else {
                print!("  \x1B[2m[Enter] Confirm  |  [Esc] Cancel  |  [Ctrl+A/E] Home/End  |  [Ctrl+U/W] Clear  |  [Ctrl+C] Quit\x1B[0m\x1B[K\r\n");
            }

            execute!(stdout, Clear(ClearType::FromCursorDown))?;

            // Compute visual cursor_x
            let visual_cursor_offset = if input_buffer.len() > max_display_len {
                let start = cursor_pos.saturating_sub(max_display_len.saturating_sub(4));
                cursor_pos.saturating_sub(start)
            } else {
                cursor_pos
            };
            let cursor_x = (prefix_len + visual_cursor_offset) as u16;

            execute!(stdout, MoveTo(cursor_x, cursor_y), Show)?;
            stdout.flush()?;

            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                // Global abort: Ctrl+C
                if (key.modifiers.contains(KeyModifiers::CONTROL)
                    && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('C')))
                    || key.code == KeyCode::Char('\x03')
                {
                    clean_exit();
                }

                match key.code {
                    KeyCode::Enter => {
                        let trimmed = input_buffer.trim();
                        if trimmed.is_empty() {
                            if let Some(def) = default_val {
                                return Ok(Some(def.to_string()));
                            }
                        }
                        return Ok(Some(trimmed.to_string()));
                    }
                    KeyCode::Esc => {
                        return Ok(None);
                    }
                    KeyCode::Backspace => {
                        if cursor_pos > 0 && !input_buffer.is_empty() {
                            input_buffer.remove(cursor_pos - 1);
                            cursor_pos -= 1;
                        }
                    }
                    KeyCode::Delete => {
                        if cursor_pos < input_buffer.len() {
                            input_buffer.remove(cursor_pos);
                        }
                    }
                    KeyCode::Left => {
                        cursor_pos = cursor_pos.saturating_sub(1);
                    }
                    KeyCode::Right => {
                        if cursor_pos < input_buffer.len() {
                            cursor_pos += 1;
                        }
                    }
                    KeyCode::Home => {
                        cursor_pos = 0;
                    }
                    KeyCode::End => {
                        cursor_pos = input_buffer.len();
                    }
                    // Readline keybindings:
                    KeyCode::Char('a') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        cursor_pos = 0;
                    }
                    KeyCode::Char('e') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        cursor_pos = input_buffer.len();
                    }
                    KeyCode::Char('u') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        input_buffer.clear();
                        cursor_pos = 0;
                    }
                    KeyCode::Char('k') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        input_buffer.truncate(cursor_pos);
                    }
                    KeyCode::Char('w') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                        // Delete word backward
                        let before = &input_buffer[..cursor_pos];
                        let trimmed = before.trim_end();
                        let word_start = trimmed.rfind(' ').map(|idx| idx + 1).unwrap_or(0);
                        let after = input_buffer[cursor_pos..].to_string();
                        input_buffer = format!("{}{}", &input_buffer[..word_start], after);
                        cursor_pos = word_start;
                    }
                    KeyCode::Char(c) if !c.is_control() => {
                        input_buffer.insert(cursor_pos, c);
                        cursor_pos += 1;
                    }
                    _ => {}
                }
            }
        }
    })();

    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), Hide);
    result
}

#[cfg(test)]
mod tests {
    #[test]
    fn test_readline_word_deletion_logic() {
        let mut buf = "hello world test".to_string();
        let mut pos = buf.len();
        // simulate ctrl+w
        let before = &buf[..pos];
        let trimmed = before.trim_end();
        let word_start = trimmed.rfind(' ').map(|idx| idx + 1).unwrap_or(0);
        let after = buf[pos..].to_string();
        buf = format!("{}{}", &buf[..word_start], after);
        pos = word_start;

        assert_eq!(buf, "hello world ");
        assert_eq!(pos, 12);
    }
}
