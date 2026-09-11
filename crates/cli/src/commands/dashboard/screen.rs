use std::io::{self, Write};
use std::sync::atomic::{AtomicUsize, Ordering};
use colored::Colorize;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};

use craft_core::Result;

static ALT_SCREEN_DEPTH: AtomicUsize = AtomicUsize::new(0);

/// Re-entrant RAII guard for the terminal alternate screen.
/// Ensures nested submenus and dialogs do not exit alternate screen prematurely.
pub struct AltScreenGuard;

impl AltScreenGuard {
    pub fn enter() -> Self {
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

/// Represents an item in a menu with a primary hotkey and optional mnemonic aliases.
pub struct MenuEntry {
    pub hotkey: String,
    pub label: String,
    pub aliases: Vec<String>,
}

impl MenuEntry {
    pub fn new(hotkey: impl Into<String>, label: impl Into<String>) -> Self {
        Self {
            hotkey: hotkey.into(),
            label: label.into(),
            aliases: Vec::new(),
        }
    }

    pub fn with_aliases(mut self, aliases: &[&str]) -> Self {
        self.aliases = aliases
            .iter()
            .map(|s| s.to_string())
            .filter(|s| {
                // Defensive rule: If an alias is a single ASCII digit, it must match self.hotkey.
                // This prevents cross-digit collisions (e.g. key '2' activating option '1').
                if s.len() == 1 && s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                    s == &self.hotkey
                } else {
                    true
                }
            })
            .collect();
        self
    }
}

/// Runs an in-place alternate screen menu loop with arrow keys and direct hotkeys.
pub fn run_menu(
    header: &str,
    entries: &[MenuEntry],
    selected_idx: &mut usize,
) -> Result<Option<usize>> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    let _ = execute!(stdout, Hide);

    if *selected_idx >= entries.len() {
        *selected_idx = 0;
    }

    let result = (|| -> Result<Option<usize>> {
        loop {
            execute!(stdout, MoveTo(0, 0))?;

            for line in header.lines() {
                print!("{}\x1B[K\r\n", line);
            }
            print!("\x1B[K\r\n");

            for (i, entry) in entries.iter().enumerate() {
                let badge = format!("[{}]", entry.hotkey);
                if i == *selected_idx {
                    print!(
                        "  \x1B[1;36m>\x1B[0m \x1B[1;36m{:<5}\x1B[0m \x1B[1;37m{}\x1B[0m\x1B[K\r\n",
                        badge,
                        entry.label
                    );
                } else {
                    print!(
                        "    \x1B[36m{:<5}\x1B[0m {}\x1B[K\r\n",
                        badge,
                        entry.label
                    );
                }
            }

            print!("\x1B[K\r\n\x1B[2m--------------------------------------------------------------------------------\x1B[K\r\n");
            print!(" [HOTKEYS] Press key directly (0-9)  |  [↑/↓/j/k] Move  |  [Enter] Select  |  [q] Exit\x1B[0m\x1B[K\r\n");

            execute!(stdout, Clear(ClearType::FromCursorDown))?;
            stdout.flush()?;

            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                match key.code {
                    KeyCode::Up | KeyCode::Char('k') => {
                        if *selected_idx > 0 {
                            *selected_idx -= 1;
                        } else {
                            *selected_idx = entries.len().saturating_sub(1);
                        }
                    }
                    KeyCode::Down | KeyCode::Char('j') => {
                        if *selected_idx + 1 < entries.len() {
                            *selected_idx += 1;
                        } else {
                            *selected_idx = 0;
                        }
                    }
                    KeyCode::Home => {
                        *selected_idx = 0;
                    }
                    KeyCode::End => {
                        *selected_idx = entries.len().saturating_sub(1);
                    }
                    KeyCode::Enter => {
                        return Ok(Some(*selected_idx));
                    }
                    KeyCode::Esc => {
                        return Ok(None);
                    }
                    KeyCode::Char(c) => {
                        let c_lower = c.to_ascii_lowercase();
                        if c_lower == 'q' {
                            if let Some(pos) = entries.iter().position(|e| {
                                e.hotkey.eq_ignore_ascii_case("q")
                                    || e.aliases.iter().any(|a| a.eq_ignore_ascii_case("q"))
                            }) {
                                *selected_idx = pos;
                                return Ok(Some(pos));
                            }
                            return Ok(None);
                        }

                        let c_str = c_lower.to_string();
                        if let Some(pos) = entries.iter().position(|e| {
                            e.hotkey.eq_ignore_ascii_case(&c_str)
                                || e.aliases.iter().any(|a| a.eq_ignore_ascii_case(&c_str))
                        }) {
                            *selected_idx = pos;
                            return Ok(Some(pos));
                        }
                    }
                    _ => {}
                }
            }
        }
    })();

    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), Show);
    result
}

/// Displays an in-place modal dialog box without leaving the alternate screen.
pub fn show_modal_message<S: AsRef<str>>(title: &str, lines: &[S], is_error: bool) -> Result<()> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    let _ = execute!(stdout, Hide);

    let result = (|| -> Result<()> {
        execute!(stdout, MoveTo(0, 0))?;
        let sep = "================================================================================";
        let div = "--------------------------------------------------------------------------------";

        if is_error {
            print!("{}\x1B[K\r\n", sep.red().bold());
            print!("{:^80}\x1B[K\r\n", title.red().bold());
            print!("{}\x1B[K\r\n", sep.red().bold());
        } else {
            print!("{}\x1B[K\r\n", sep.cyan().bold());
            print!("{:^80}\x1B[K\r\n", title.cyan().bold());
            print!("{}\x1B[K\r\n", sep.cyan().bold());
        }
        print!("\x1B[K\r\n");

        for line in lines {
            print!("  {}\x1B[K\r\n", line.as_ref());
        }

        print!("\x1B[K\r\n{}\x1B[K\r\n", div.dimmed());
        print!("  \x1B[2m[Press Enter, Space, Esc, or 'q' to return]\x1B[0m\x1B[K\r\n");

        execute!(stdout, Clear(ClearType::FromCursorDown))?;
        stdout.flush()?;

        loop {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press {
                    match key.code {
                        KeyCode::Enter | KeyCode::Esc | KeyCode::Char(' ') | KeyCode::Char('q') => break,
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

/// Displays an in-place single-line input prompt with full editing support.
pub fn run_input_prompt(
    header_title: &str,
    prompt_label: &str,
    default_val: Option<&str>,
) -> Result<Option<String>> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;

    let mut input_buffer = default_val.unwrap_or("").to_string();
    let mut cursor_pos = input_buffer.len();

    let result = (|| -> Result<Option<String>> {
        loop {
            execute!(stdout, MoveTo(0, 0))?;
            let mut current_y = 0u16;

            print!("{}\x1B[K\r\n", "================================================================================".cyan().bold());
            current_y += 1;
            print!("{:^80}\x1B[K\r\n", header_title.cyan().bold());
            current_y += 1;
            print!("{}\x1B[K\r\n", "================================================================================".cyan().bold());
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
            let display_text = match default_val {
                Some(def) if input_buffer.is_empty() => format!("\x1B[2m{}\x1B[0m", def),
                _ => input_buffer.clone(),
            };
            print!("{}{}\x1B[K\r\n", prefix.cyan().bold(), display_text);

            print!("\x1B[K\r\n{}\x1B[K\r\n", "--------------------------------------------------------------------------------".dimmed());
            print!("  \x1B[2m[Enter] Confirm  |  [Esc] Cancel  |  [Backspace] Delete\x1B[0m\x1B[K\r\n");

            execute!(stdout, Clear(ClearType::FromCursorDown))?;

            let cursor_x = (prefix.len() + cursor_pos) as u16;
            execute!(stdout, MoveTo(cursor_x, cursor_y), Show)?;
            stdout.flush()?;

            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
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

/// Renders in-place progress information pinned at top of alternate screen.
pub fn print_in_place_status<S: AsRef<str>>(title: &str, lines: &[S]) -> Result<()> {
    let mut stdout = io::stdout();
    execute!(stdout, MoveTo(0, 0))?;
    print!("{}\x1B[K\r\n", "================================================================================".cyan().bold());
    print!("{:^80}\x1B[K\r\n", title.cyan().bold());
    print!("{}\x1B[K\r\n", "================================================================================".cyan().bold());
    print!("\x1B[K\r\n");
    for line in lines {
        print!("  {}\x1B[K\r\n", line.as_ref());
    }
    print!("\x1B[K\r\n{}\x1B[K\r\n", "--------------------------------------------------------------------------------".dimmed());
    execute!(stdout, Clear(ClearType::FromCursorDown))?;
    stdout.flush()?;
    Ok(())
}

/// Temporarily leaves alternate screen to execute an interactive console (e.g. craft view),
/// automatically restoring the alternate screen and raw mode when the session finishes.
pub async fn exec_console_action<F, Fut>(action: F) -> Result<()>
where
    F: FnOnce() -> Fut,
    Fut: std::future::Future<Output = Result<()>>,
{
    let _ = disable_raw_mode();
    let _ = execute!(io::stdout(), LeaveAlternateScreen, Show);

    let res = action().await;

    let _ = execute!(io::stdout(), EnterAlternateScreen, Hide);
    let _ = enable_raw_mode();
    res
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_menu_entry_defensive_alias_filtering() {
        let entry = MenuEntry::new("1", "Test Option").with_aliases(&["2", "3", "c", "n"]);
        // "2" and "3" should be filtered out because they are digits != "1"
        assert_eq!(entry.aliases, vec!["c".to_string(), "n".to_string()]);

        let entry2 = MenuEntry::new("0", "Back").with_aliases(&["b", "q", "0"]);
        // "0" is allowed because it matches hotkey "0"
        assert_eq!(entry2.aliases, vec!["b".to_string(), "q".to_string(), "0".to_string()]);
    }
}
