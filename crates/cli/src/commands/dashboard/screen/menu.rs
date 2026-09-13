use std::io::{self, Write};
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType},
};

use craft_core::Result;
use super::terminal::{get_content_width, get_terminal_size};
use super::theme::{box_divider, DIM, RESET};
use super::clean_exit;

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
                // Defensive rule 1: If an alias is a single ASCII digit, it must match self.hotkey.
                // This prevents cross-digit collisions (e.g. key '2' activating option '1').
                if s.len() == 1
                    && s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false)
                    && s != &self.hotkey
                {
                    return false;
                }
                // Defensive rule 2: 'q' and 'Q' are reserved globally for quitting the program completely.
                if s.eq_ignore_ascii_case("q") && !self.hotkey.eq_ignore_ascii_case("q") {
                    return false;
                }
                true
            })
            .collect();
        self
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Select(usize),
    Space(usize),
    Back,
}

/// Runs an in-place alternate screen menu loop with responsive virtual scrolling, arrow keys, and hotkeys.
pub fn run_menu(
    header: &str,
    entries: &[MenuEntry],
    selected_idx: &mut usize,
) -> Result<Option<usize>> {
    match run_menu_ext(header, entries, selected_idx, false)? {
        MenuAction::Select(idx) => Ok(Some(idx)),
        MenuAction::Space(idx) => Ok(Some(idx)),
        MenuAction::Back => Ok(None),
    }
}

pub fn run_menu_with_space(
    header: &str,
    entries: &[MenuEntry],
    selected_idx: &mut usize,
) -> Result<MenuAction> {
    run_menu_ext(header, entries, selected_idx, true)
}

pub fn run_menu_ext(
    header: &str,
    entries: &[MenuEntry],
    selected_idx: &mut usize,
    allow_space: bool,
) -> Result<MenuAction> {
    let mut stdout = io::stdout();
    enable_raw_mode()?;
    let _ = execute!(stdout, Hide);

    if *selected_idx >= entries.len() {
        *selected_idx = 0;
    }

    let mut scroll_offset: usize = 0;

    let result = (|| -> Result<MenuAction> {
        loop {
            let (term_w, term_h) = get_terminal_size();
            let width = get_content_width(80);

            // Compute available viewport lines for entries
            let header_line_count = header.lines().count();
            let footer_reserve = 4; // divider + hotkeys + margins
            let viewport_size = (term_h as usize)
                .saturating_sub(header_line_count + footer_reserve)
                .max(4);

            // Keep selected index in scroll view
            if *selected_idx < scroll_offset {
                scroll_offset = *selected_idx;
            } else if *selected_idx >= scroll_offset + viewport_size {
                scroll_offset = *selected_idx - viewport_size + 1;
            }
            if scroll_offset + viewport_size > entries.len() {
                scroll_offset = entries.len().saturating_sub(viewport_size);
            }

            execute!(stdout, MoveTo(0, 0))?;

            for line in header.lines() {
                print!("{}\x1B[K\r\n", line);
            }
            print!("\x1B[K\r\n");

            // Scroll indicator above
            if scroll_offset > 0 {
                print!("    {}[▲ {} more items above]{}\x1B[K\r\n", DIM, scroll_offset, RESET);
            }

            let end_idx = entries.len().min(scroll_offset + viewport_size);
            let visible_entries = &entries[scroll_offset..end_idx];

            for (local_i, entry) in visible_entries.iter().enumerate() {
                let abs_i = scroll_offset + local_i;
                let badge = format!("[{}]", entry.hotkey);
                if abs_i == *selected_idx {
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

            // Scroll indicator below
            if end_idx < entries.len() {
                print!("    {}[▼ {} more items below]{}\x1B[K\r\n", DIM, entries.len() - end_idx, RESET);
            }

            print!("\x1B[K\r\n{}\x1B[K\r\n", box_divider(width));
            if allow_space {
                if term_w < 70 {
                    print!(" [HOTKEYS] (0-9) | [Space] Toggle | [Enter] Select | [Esc] Back | [q] Exit\x1B[0m\x1B[K\r\n");
                } else {
                    print!(" [HOTKEYS] (0-9)  |  [↑/↓/j/k] Move  |  [PgUp/PgDn] Page  |  [Space] Toggle  |  [Enter/→] Select  |  [Esc/←] Back  |  [q] Exit\x1B[0m\x1B[K\r\n");
                }
            } else if term_w < 70 {
                print!(" [HOTKEYS] (0-9) | [↑/↓] Move | [Enter] Select | [Esc] Back | [q] Exit\x1B[0m\x1B[K\r\n");
            } else {
                print!(" [HOTKEYS] (0-9)  |  [↑/↓/j/k] Move  |  [PgUp/PgDn] Page  |  [Enter/→] Select  |  [Esc/←] Back  |  [q] Exit\x1B[0m\x1B[K\r\n");
            }

            execute!(stdout, Clear(ClearType::FromCursorDown))?;
            stdout.flush()?;

            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }

                // Global abort: Ctrl+C
                if (key.modifiers.contains(event::KeyModifiers::CONTROL)
                    && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('C')))
                    || key.code == KeyCode::Char('\x03')
                {
                    clean_exit();
                }

                // Global quit: 'q' or 'Q'
                if key.code == KeyCode::Char('q') || key.code == KeyCode::Char('Q') {
                    clean_exit();
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
                    KeyCode::PageUp => {
                        *selected_idx = selected_idx.saturating_sub(viewport_size);
                    }
                    KeyCode::PageDown => {
                        *selected_idx = (*selected_idx + viewport_size).min(entries.len().saturating_sub(1));
                    }
                    KeyCode::Home => {
                        *selected_idx = 0;
                    }
                    KeyCode::End => {
                        *selected_idx = entries.len().saturating_sub(1);
                    }
                    KeyCode::Char(' ') if allow_space => {
                        return Ok(MenuAction::Space(*selected_idx));
                    }
                    KeyCode::Enter | KeyCode::Right => {
                        return Ok(MenuAction::Select(*selected_idx));
                    }
                    KeyCode::Esc | KeyCode::Left => {
                        return Ok(MenuAction::Back);
                    }
                    KeyCode::Char(c) => {
                        let c_str = c.to_ascii_lowercase().to_string();
                        if let Some(pos) = entries.iter().position(|e| {
                            e.hotkey.eq_ignore_ascii_case(&c_str)
                                || e.aliases.iter().any(|a| a.eq_ignore_ascii_case(&c_str))
                        }) {
                            *selected_idx = pos;
                            return Ok(MenuAction::Select(pos));
                        } else if c == '<' || c == ',' || c == '[' {
                            *selected_idx = selected_idx.saturating_sub(viewport_size);
                        } else if c == '>' || c == '.' || c == ']' {
                            *selected_idx = (*selected_idx + viewport_size).min(entries.len().saturating_sub(1));
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PagedMenuAction {
    Select(usize),
    Space(usize),
    Action(String),
    Back,
}

/// Runs an interactive menu with paginated list items, per-page hotkeys, and dynamic navigation
pub fn run_paged_list_menu<T, H, R>(
    items: &[T],
    current_page: &mut usize,
    page_size: usize,
    header_builder: H,
    render_item: R,
    action_entries: &[MenuEntry],
    allow_space: bool,
) -> Result<PagedMenuAction>
where
    H: Fn(usize, usize, usize) -> String,
    R: Fn(usize, usize, &T) -> String,
{
    let page_size = page_size.max(1);
    let mut menu_selected = 0;

    loop {
        let total_items = items.len();
        let total_pages = if total_items == 0 {
            1
        } else {
            total_items.div_ceil(page_size)
        };
        if *current_page >= total_pages {
            *current_page = total_pages.saturating_sub(1);
        }

        let start_idx = *current_page * page_size;
        let end_idx = total_items.min(start_idx + page_size);
        let page_count = end_idx.saturating_sub(start_idx);

        let header = header_builder(*current_page + 1, total_pages, total_items);
        let mut entries = Vec::new();

        for local_idx in 0..page_count {
            let global_idx = start_idx + local_idx;
            let hotkey = (local_idx + 1).to_string();
            let label = render_item(local_idx, global_idx, &items[global_idx]);
            entries.push(MenuEntry::new(hotkey, label));
        }

        // Page navigation controls if multiple pages
        let mut prev_page_idx = None;
        let mut next_page_idx = None;
        if total_pages > 1 {
            if *current_page > 0 {
                prev_page_idx = Some(entries.len());
                entries.push(MenuEntry::new("<", "Previous Page").with_aliases(&["p", "["]));
            }
            if *current_page + 1 < total_pages {
                next_page_idx = Some(entries.len());
                entries.push(MenuEntry::new(">", "Next Page").with_aliases(&["]"]));
            }
        }

        let action_start_idx = entries.len();
        for action in action_entries {
            entries.push(
                MenuEntry::new(&action.hotkey, &action.label)
                    .with_aliases(&action.aliases.iter().map(|s| s.as_str()).collect::<Vec<_>>()),
            );
        }

        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b"]));

        let action = run_menu_ext(&header, &entries, &mut menu_selected, allow_space)?;
        match action {
            MenuAction::Select(idx) => {
                if idx < page_count {
                    return Ok(PagedMenuAction::Select(start_idx + idx));
                } else if Some(idx) == prev_page_idx {
                    if *current_page > 0 {
                        *current_page -= 1;
                        menu_selected = 0;
                    }
                } else if Some(idx) == next_page_idx {
                    if *current_page + 1 < total_pages {
                        *current_page += 1;
                        menu_selected = 0;
                    }
                } else if idx >= action_start_idx && idx < action_start_idx + action_entries.len() {
                    let chosen_action = &action_entries[idx - action_start_idx];
                    return Ok(PagedMenuAction::Action(chosen_action.hotkey.clone()));
                } else {
                    return Ok(PagedMenuAction::Back);
                }
            }
            MenuAction::Space(idx) => {
                if idx < page_count {
                    return Ok(PagedMenuAction::Space(start_idx + idx));
                }
            }
            MenuAction::Back => {
                return Ok(PagedMenuAction::Back);
            }
        }
    }
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
        // "0" is allowed because it matches hotkey "0", but "q" is filtered out because it is reserved for quitting completely
        assert_eq!(entry2.aliases, vec!["b".to_string(), "0".to_string()]);
    }

    #[test]
    fn test_pagination_bounds() {
        let total_items: usize = 15;
        let page_size: usize = 6;
        let total_pages = total_items.div_ceil(page_size);
        assert_eq!(total_pages, 3);

        // Page 0: items 0..6 (count 6)
        let page0_start = 0;
        let page0_end = total_items.min(page0_start + page_size);
        assert_eq!(page0_end - page0_start, 6);

        // Page 2: items 12..15 (count 3)
        let page2_start = 2 * page_size;
        let page2_end = total_items.min(page2_start + page_size);
        assert_eq!(page2_end - page2_start, 3);
    }
}
