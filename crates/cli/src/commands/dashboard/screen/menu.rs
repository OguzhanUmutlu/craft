use craft_core::Result;

pub use modalx::{EventDecision, MenuEntry};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MenuAction {
    Select(usize),
    Space(usize),
    ItemAction(char, usize),
    Back,
}

/// Runs a high-level titled menu modal with automatic navigation bar and metadata header rows.
#[allow(dead_code)]
pub fn run_titled_menu(
    title: impl Into<String>,
    header_rows: &[impl AsRef<str>],
    entries: &[MenuEntry],
    selected_idx: &mut usize,
) -> Result<Option<usize>> {
    let mut modal = super::modals::SelectModal::menu(title)
        .with_allow_quit_on_q(super::NavGuard::depth() <= 1)
        .with_entries(entries.to_vec());

    for row in header_rows {
        modal = modal.with_header_row(row.as_ref());
    }

    match modal.run(selected_idx)? {
        super::modals::SelectOutcome::Selected(idx) => Ok(Some(idx)),
        super::modals::SelectOutcome::Toggled(idx) => Ok(Some(idx)),
        _ => Ok(None),
    }
}

/// Runs a high-level titled menu modal with custom footer shortcuts.
#[allow(dead_code)]
pub fn run_titled_menu_with_shortcuts(
    title: impl Into<String>,
    header_rows: &[impl AsRef<str>],
    entries: &[MenuEntry],
    selected_idx: &mut usize,
    shortcuts: impl Into<modalx::Shortcuts>,
) -> Result<Option<usize>> {
    let mut modal = super::modals::SelectModal::menu(title)
        .with_allow_quit_on_q(super::NavGuard::depth() <= 1)
        .with_entries(entries.to_vec())
        .with_shortcuts(shortcuts);

    for row in header_rows {
        modal = modal.with_header_row(row.as_ref());
    }

    match modal.run(selected_idx)? {
        super::modals::SelectOutcome::Selected(idx) => Ok(Some(idx)),
        super::modals::SelectOutcome::Toggled(idx) => Ok(Some(idx)),
        _ => Ok(None),
    }
}

/// Runs the main dashboard menu with the standard navigation footer bar.
pub fn run_main_menu(
    header: &str,
    entries: &[MenuEntry],
    selected_idx: &mut usize,
) -> Result<Option<usize>> {
    match run_menu_impl(header, entries, selected_idx, false, true)? {
        MenuAction::Select(idx) => Ok(Some(idx)),
        MenuAction::Space(idx) => Ok(Some(idx)),
        _ => Ok(None),
    }
}

/// Runs an in-place alternate screen menu loop with responsive virtual scrolling, arrow keys, and hotkeys.
pub fn run_menu(
    header: &str,
    entries: &[MenuEntry],
    selected_idx: &mut usize,
) -> Result<Option<usize>> {
    match run_menu_impl(header, entries, selected_idx, false, false)? {
        MenuAction::Select(idx) => Ok(Some(idx)),
        MenuAction::Space(idx) => Ok(Some(idx)),
        _ => Ok(None),
    }
}

/// Runs a menu with an in-place action/event handler callback that can cancel/intercept actions.
pub fn run_menu_with_handler<F>(
    header: &str,
    entries: &[MenuEntry],
    selected_idx: &mut usize,
    handler: F,
) -> Result<Option<usize>>
where
    F: FnMut(char, usize, &mut [modalx::SelectItem], &mut Vec<String>) -> modalx::EventDecision,
{
    let modal = super::modals::SelectModal::from_legacy(header, entries)
        .with_allow_quit_on_q(super::NavGuard::depth() <= 1);

    match modal.run_with_handler(selected_idx, handler)? {
        super::modals::SelectOutcome::Selected(idx) => Ok(Some(idx)),
        super::modals::SelectOutcome::Toggled(idx) => Ok(Some(idx)),
        _ => Ok(None),
    }
}

pub fn run_menu_with_space(
    header: &str,
    entries: &[MenuEntry],
    selected_idx: &mut usize,
) -> Result<MenuAction> {
    run_menu_impl(header, entries, selected_idx, true, false)
}

#[allow(dead_code)]
pub fn run_menu_ext(
    header: &str,
    entries: &[MenuEntry],
    selected_idx: &mut usize,
    allow_space: bool,
) -> Result<MenuAction> {
    run_menu_impl(header, entries, selected_idx, allow_space, false)
}

pub fn run_menu_impl(
    header: &str,
    entries: &[MenuEntry],
    selected_idx: &mut usize,
    allow_space: bool,
    is_main: bool,
) -> Result<MenuAction> {
    run_menu_custom(
        header,
        entries,
        selected_idx,
        allow_space,
        is_main,
        &[],
        None,
    )
}

pub fn run_menu_custom(
    header: &str,
    entries: &[MenuEntry],
    selected_idx: &mut usize,
    allow_space: bool,
    is_main: bool,
    item_actions: &[char],
    footer_help: Option<&str>,
) -> Result<MenuAction> {
    let mut modal = super::modals::SelectModal::from_legacy(header, entries)
        .with_allow_toggle(allow_space)
        .with_allow_quit_on_q(is_main || super::NavGuard::depth() <= 1)
        .with_item_actions(item_actions.iter().copied());

    if let Some(footer) = footer_help {
        modal = modal.with_footer_help(footer);
    }

    match modal.run(selected_idx)? {
        super::modals::SelectOutcome::Selected(idx) => Ok(MenuAction::Select(idx)),
        super::modals::SelectOutcome::Toggled(idx) => Ok(MenuAction::Space(idx)),
        super::modals::SelectOutcome::ItemAction(c, idx) => Ok(MenuAction::ItemAction(c, idx)),
        super::modals::SelectOutcome::Cancelled => Ok(MenuAction::Back),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PagedMenuAction {
    Select(usize),
    Space(usize),
    Action(String),
    ItemAction(char, usize),
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
    run_paged_list_menu_ext(
        items,
        current_page,
        page_size,
        header_builder,
        render_item,
        action_entries,
        allow_space,
        &[],
        None,
    )
}

/// Runs an interactive paged menu with configurable item-level action hotkeys and custom footer help text.
#[allow(clippy::too_many_arguments)]
pub fn run_paged_list_menu_ext<T, H, R>(
    items: &[T],
    current_page: &mut usize,
    page_size: usize,
    header_builder: H,
    render_item: R,
    action_entries: &[MenuEntry],
    allow_space: bool,
    item_actions: &[char],
    footer_help: Option<&str>,
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
                MenuEntry::new(&action.hotkey, &action.label).with_aliases(
                    &action
                        .aliases
                        .iter()
                        .map(|s| s.as_str())
                        .collect::<Vec<_>>(),
                ),
            );
        }

        entries.push(MenuEntry::new("0", "Back").with_aliases(&["b"]));

        let action = run_menu_custom(
            &header,
            &entries,
            &mut menu_selected,
            allow_space,
            false,
            item_actions,
            footer_help,
        )?;
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
            MenuAction::ItemAction(c, idx) => {
                if idx < page_count {
                    return Ok(PagedMenuAction::ItemAction(c, start_idx + idx));
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
