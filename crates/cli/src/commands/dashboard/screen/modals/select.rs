use colored::Colorize;
use crossterm::{
    cursor::Hide,
    event::{self, Event},
    execute,
    terminal::enable_raw_mode,
};
use std::io;

use craft_core::Result;
use super::super::clean_exit;
use super::super::frame::BoxFrame;
use super::super::keys::{KeyAction, KeyHelpMode, KeyMap};
use super::super::menu::MenuEntry;
use super::super::terminal::{get_content_width, get_terminal_size, is_terminal_too_small, wait_for_valid_size};
use super::super::theme::strip_ansi;

/// Outcome returned by running a `SelectModal`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SelectOutcome {
    Selected(usize),
    Toggled(usize),
    Cancelled,
}

/// A fully boxed, responsive menu/button selection modal.
#[derive(Debug, Clone)]
pub struct SelectModal {
    pub title: Option<(String, bool)>,
    pub breadcrumbs: Option<String>,
    pub header_rows: Vec<String>,
    pub entries: Vec<MenuEntry>,
    pub allow_toggle: bool,
    pub allow_quit_on_q: bool,
    pub keymap: KeyMap,
    pub footer_help: Option<String>,
    pub max_width: u16,
}

impl Default for SelectModal {
    fn default() -> Self {
        Self::new()
    }
}

#[allow(dead_code)]
impl SelectModal {
    /// Creates a new empty `SelectModal`.
    pub fn new() -> Self {
        Self {
            title: None,
            breadcrumbs: None,
            header_rows: Vec::new(),
            entries: Vec::new(),
            allow_toggle: false,
            allow_quit_on_q: true,
            keymap: KeyMap::menu_default(true),
            footer_help: None,
            max_width: 80,
        }
    }

    /// Creates a `SelectModal` by parsing a legacy dashboard header string and list of menu entries.
    pub fn from_legacy(header: &str, entries: &[MenuEntry]) -> Self {
        let (parsed_title, parsed_rows) = parse_header_lines(header);
        let mut modal = Self::new().with_entries(entries.to_vec());
        if let Some(title) = parsed_title {
            modal = modal.with_title(title, false);
        }
        modal.header_rows = parsed_rows;
        modal
    }

    /// Sets modal title.
    pub fn with_title(mut self, title: impl Into<String>, is_error: bool) -> Self {
        self.title = Some((title.into(), is_error));
        self
    }

    /// Sets navigation breadcrumbs.
    pub fn with_breadcrumbs(mut self, breadcrumbs: impl Into<String>) -> Self {
        self.breadcrumbs = Some(breadcrumbs.into());
        self
    }

    /// Adds a metadata content row inside the box above the menu entries.
    pub fn with_header_row(mut self, row: impl Into<String>) -> Self {
        self.header_rows.push(row.into());
        self
    }

    /// Sets all metadata header rows.
    pub fn with_header_rows(mut self, rows: impl IntoIterator<Item = impl Into<String>>) -> Self {
        self.header_rows = rows.into_iter().map(Into::into).collect();
        self
    }

    /// Sets menu entries.
    pub fn with_entries(mut self, entries: Vec<MenuEntry>) -> Self {
        self.entries = entries;
        self
    }

    /// Adds a single menu entry.
    pub fn with_entry(mut self, entry: MenuEntry) -> Self {
        self.entries.push(entry);
        self
    }

    /// Enables or disables Spacebar toggle behavior.
    pub fn with_allow_toggle(mut self, allow: bool) -> Self {
        self.allow_toggle = allow;
        self
    }

    /// Configures whether pressing 'q' quits the application.
    pub fn with_allow_quit_on_q(mut self, allow: bool) -> Self {
        self.allow_quit_on_q = allow;
        self.keymap.allow_quit_on_q = allow;
        self
    }

    /// Sets a custom `KeyMap`.
    pub fn with_keymap(mut self, keymap: KeyMap) -> Self {
        self.keymap = keymap;
        self
    }

    /// Sets custom bottom footer help text.
    pub fn with_footer_help(mut self, help: impl Into<String>) -> Self {
        self.footer_help = Some(help.into());
        self
    }

    /// Sets maximum desired box content width (defaults to 80).
    pub fn with_max_width(mut self, width: u16) -> Self {
        self.max_width = width;
        self
    }

    /// Executes the interactive menu event loop, updating `selected_idx`.
    pub fn run(&self, selected_idx: &mut usize) -> Result<SelectOutcome> {
        let mut stdout = io::stdout();
        enable_raw_mode()?;
        let _ = execute!(stdout, Hide);

        if self.entries.is_empty() || *selected_idx >= self.entries.len() {
            *selected_idx = 0;
        }

        let mut scroll_offset: usize = 0;

        loop {
            if is_terminal_too_small() {
                wait_for_valid_size(&mut stdout)?;
                continue;
            }

            let (_, term_h) = get_terminal_size();
            let width = get_content_width(self.max_width);

            // Overhead: top border (1) + title/divider (2 if present) + breadcrumbs (2 if present)
            // + header rows + divider (1 if header rows present) + footer/bottom (3)
            let overhead = 1
                + if self.title.is_some() { 2 } else { 0 }
                + if self.breadcrumbs.is_some() { 2 } else { 0 }
                + self.header_rows.len()
                + if !self.header_rows.is_empty() { 1 } else { 0 }
                + 3;

            let viewport_size = (term_h as usize).saturating_sub(overhead).max(3);

            // Keep selected index visible
            if *selected_idx < scroll_offset {
                scroll_offset = *selected_idx;
            } else if *selected_idx >= scroll_offset + viewport_size {
                scroll_offset = *selected_idx - viewport_size + 1;
            }
            if scroll_offset + viewport_size > self.entries.len() {
                scroll_offset = self.entries.len().saturating_sub(viewport_size);
            }

            // Build BoxFrame
            let mut frame = BoxFrame::new(width);
            if let Some((ref title, is_err)) = self.title {
                frame.title = Some((title.clone(), is_err));
            }
            if let Some(ref bc) = self.breadcrumbs {
                frame.breadcrumbs = Some(bc.clone());
            }

            for row in &self.header_rows {
                frame.row(row);
            }
            if !self.header_rows.is_empty() {
                frame.divider();
            }

            if scroll_offset > 0 {
                frame.row(format!("[▲ {} more items above]", scroll_offset).dimmed().to_string());
            }

            let end_idx = self.entries.len().min(scroll_offset + viewport_size);
            for (local_i, entry) in self.entries[scroll_offset..end_idx].iter().enumerate() {
                let abs_i = scroll_offset + local_i;
                let badge = format!("[{}]", entry.hotkey);
                let row_str = if abs_i == *selected_idx {
                    format!(
                        "> {:<5} {}",
                        badge.cyan().bold(),
                        entry.label.white().bold()
                    )
                } else {
                    format!(
                        "  {:<5} {}",
                        badge.cyan(),
                        entry.label
                    )
                };
                frame.row(row_str);
            }

            if end_idx < self.entries.len() {
                frame.row(format!("[▼ {} more items below]", self.entries.len() - end_idx).dimmed().to_string());
            }

            let help = self.footer_help.clone().unwrap_or_else(|| {
                let mode = if self.allow_toggle {
                    KeyHelpMode::MenuWithToggle
                } else {
                    KeyHelpMode::Menu
                };
                self.keymap.footer_help_text(mode)
            });
            frame.footer(help);

            frame.render(&mut stdout)?;

            match event::read()? {
                Event::Resize(..) => continue,
                Event::Key(key) => {
                    let action = self.keymap.resolve(&key);
                    match action {
                        KeyAction::Quit => {
                            clean_exit();
                        }
                        KeyAction::Up => {
                            if !self.entries.is_empty() {
                                if *selected_idx > 0 {
                                    *selected_idx -= 1;
                                } else {
                                    *selected_idx = self.entries.len().saturating_sub(1);
                                }
                            }
                        }
                        KeyAction::Down => {
                            if !self.entries.is_empty() {
                                if *selected_idx + 1 < self.entries.len() {
                                    *selected_idx += 1;
                                } else {
                                    *selected_idx = 0;
                                }
                            }
                        }
                        KeyAction::PageUp => {
                            *selected_idx = selected_idx.saturating_sub(viewport_size);
                        }
                        KeyAction::PageDown => {
                            *selected_idx = (*selected_idx + viewport_size).min(self.entries.len().saturating_sub(1));
                        }
                        KeyAction::Home => {
                            *selected_idx = 0;
                        }
                        KeyAction::End => {
                            *selected_idx = self.entries.len().saturating_sub(1);
                        }
                        KeyAction::Submit => {
                            if !self.entries.is_empty() {
                                return Ok(SelectOutcome::Selected(*selected_idx));
                            }
                        }
                        KeyAction::Toggle if self.allow_toggle => {
                            if !self.entries.is_empty() {
                                return Ok(SelectOutcome::Toggled(*selected_idx));
                            }
                        }
                        KeyAction::Cancel => {
                            return Ok(SelectOutcome::Cancelled);
                        }
                        KeyAction::Hotkey(c) => {
                            let c_str = c.to_ascii_lowercase().to_string();
                            for (idx, entry) in self.entries.iter().enumerate() {
                                if entry.hotkey.eq_ignore_ascii_case(&c_str)
                                    || entry.aliases.iter().any(|a| a.eq_ignore_ascii_case(&c_str))
                                {
                                    *selected_idx = idx;
                                    return Ok(SelectOutcome::Selected(idx));
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ => {}
            }
        }
    }
}

/// Parses legacy header strings into a clean title and individual metadata rows.
pub fn parse_header_lines(header: &str) -> (Option<String>, Vec<String>) {
    let mut title = None;
    let mut rows = Vec::new();

    let lines: Vec<&str> = header.lines().collect();
    let total_lines = lines.len();

    for raw_line in lines {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let clean = strip_ansi(trimmed);
        let clean_trimmed = clean.trim();

        // 1. Skip top borders
        if (clean_trimmed.starts_with('╭') || clean_trimmed.starts_with('┌') || clean_trimmed.starts_with('+'))
            && (clean_trimmed.ends_with('╮') || clean_trimmed.ends_with('┐') || clean_trimmed.ends_with('+'))
            && clean_trimmed.chars().all(|c| c == '╭' || c == '╮' || c == '┌' || c == '┐' || c == '─' || c == '-' || c == '+')
        {
            continue;
        }

        // 2. Skip internal dividers
        if (clean_trimmed.starts_with('├') || clean_trimmed.starts_with('+'))
            && (clean_trimmed.ends_with('┤') || clean_trimmed.ends_with('+'))
            && clean_trimmed.chars().all(|c| c == '├' || c == '┤' || c == '─' || c == '-' || c == '+')
        {
            continue;
        }

        // 3. Skip bottom borders
        if (clean_trimmed.starts_with('╰') || clean_trimmed.starts_with('└') || clean_trimmed.starts_with('+'))
            && (clean_trimmed.ends_with('╯') || clean_trimmed.ends_with('┘') || clean_trimmed.ends_with('+'))
            && clean_trimmed.chars().all(|c| c == '╰' || c == '╯' || c == '└' || c == '┘' || c == '─' || c == '-' || c == '+')
        {
            continue;
        }

        // 4. Check for framed title: starts and ends with │ or |
        if (clean_trimmed.starts_with('│') || clean_trimmed.starts_with('|'))
            && (clean_trimmed.ends_with('│') || clean_trimmed.ends_with('|'))
        {
            let inner = clean_trimmed.trim_matches(|c| c == '│' || c == '|').trim();
            if title.is_none() && !inner.is_empty() {
                title = Some(inner.to_string());
                continue;
            }
        }

        // 5. If single line header with no borders, treat it as the title
        if title.is_none() && total_lines == 1 {
            title = Some(clean_trimmed.to_string());
            continue;
        }

        // 6. Otherwise, treat as content row
        let content = if (clean_trimmed.starts_with('│') || clean_trimmed.starts_with('|'))
            && (clean_trimmed.ends_with('│') || clean_trimmed.ends_with('|'))
        {
            let s = raw_line.trim();
            s.trim_matches(|c| c == '│' || c == '|').trim().to_string()
        } else {
            raw_line.trim().to_string()
        };

        if !content.is_empty() {
            rows.push(content);
        }
    }

    (title, rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_header_lines_legacy() {
        let header = "╭───╮\r\n│ DASHBOARD │\r\n├───┤\r\n Host: Ubuntu | RAM: 8GB\r\n Registered: 1\r\n├───┤";
        let (title, rows) = parse_header_lines(header);
        assert_eq!(title.as_deref(), Some("DASHBOARD"));
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0], "Host: Ubuntu | RAM: 8GB");
        assert_eq!(rows[1], "Registered: 1");
    }

    #[test]
    fn test_parse_single_line_title() {
        let header = "Select Server Difficulty:";
        let (title, rows) = parse_header_lines(header);
        assert_eq!(title.as_deref(), Some("Select Server Difficulty:"));
        assert!(rows.is_empty());
    }
}
