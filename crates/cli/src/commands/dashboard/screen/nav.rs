use std::sync::Mutex;
use colored::Colorize;
use super::theme::{is_utf8_supported, strip_ansi, BORDER_COLOR, RESET};

static NAV_STACK: Mutex<Vec<String>> = Mutex::new(Vec::new());

/// RAII Guard that manages navigation breadcrumbs on screen headers.
pub struct NavGuard;

impl NavGuard {
    pub fn enter(title: impl Into<String>) -> Self {
        if let Ok(mut stack) = NAV_STACK.lock() {
            stack.push(title.into());
        }
        NavGuard
    }
}

impl Drop for NavGuard {
    fn drop(&mut self) {
        if let Ok(mut stack) = NAV_STACK.lock() {
            stack.pop();
        }
    }
}

/// Returns a copy of the current navigation path.
pub fn get_breadcrumbs() -> Vec<String> {
    NAV_STACK.lock().map(|s| s.clone()).unwrap_or_default()
}

/// Formats breadcrumb items into a single line, squeezing intermediate items if needed.
pub fn format_breadcrumbs(crumbs: &[String], max_width: usize) -> String {
    if crumbs.is_empty() {
        return String::new();
    }

    let sep = if is_utf8_supported() { " › " } else { " > " };

    // Full path
    let full = crumbs.join(sep);
    if full.chars().count() <= max_width {
        return full;
    }

    // Try keeping first, ellipsis, and last two
    if crumbs.len() >= 4 {
        let candidate = format!(
            "{}{}{}{}{}{}{}",
            crumbs[0],
            sep,
            "...",
            sep,
            crumbs[crumbs.len() - 2],
            sep,
            crumbs[crumbs.len() - 1]
        );
        if candidate.chars().count() <= max_width {
            return candidate;
        }
    }

    // Try keeping first, ellipsis, and last one
    if crumbs.len() >= 3 {
        let candidate = format!(
            "{}{}{}{}{}",
            crumbs[0],
            sep,
            "...",
            sep,
            crumbs[crumbs.len() - 1]
        );
        if candidate.chars().count() <= max_width {
            return candidate;
        }
    }

    // Try keeping ellipsis and last one
    let last = crumbs.last().map(|s| s.as_str()).unwrap_or("");
    let last_candidate = format!("...{}{}", sep, last);
    if last_candidate.chars().count() <= max_width {
        return last_candidate;
    }

    // Truncate the last item if even that doesn't fit
    if max_width > 3 {
        let truncated: String = last.chars().take(max_width.saturating_sub(3)).collect();
        format!("{}...", truncated)
    } else {
        last.chars().take(max_width).collect()
    }
}

/// Generates a bordered breadcrumb line if more than 1 navigation level exists.
pub fn box_breadcrumbs(width: usize) -> Option<String> {
    let crumbs = get_breadcrumbs();
    if crumbs.len() <= 1 {
        return None;
    }

    let inner_width = width.saturating_sub(2);
    let raw_formatted = format_breadcrumbs(&crumbs, inner_width.saturating_sub(4));
    let clean = strip_ansi(&raw_formatted);
    let len = clean.chars().count();

    let (pad_left, pad_right) = if len < inner_width {
        let rem = inner_width - len;
        (rem / 2, rem - rem / 2)
    } else {
        (0, 0)
    };

    let border_char = if is_utf8_supported() { "│" } else { "|" };
    let styled_text = raw_formatted.dimmed().to_string();

    Some(format!(
        "{}{}{}{}{}{}{}{}{}",
        BORDER_COLOR, border_char, RESET,
        " ".repeat(pad_left),
        styled_text,
        " ".repeat(pad_right),
        BORDER_COLOR, border_char, RESET,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_nav_guard_stack() {
        {
            let _n1 = NavGuard::enter("Dashboard");
            assert_eq!(get_breadcrumbs(), vec!["Dashboard"]);
            {
                let _n2 = NavGuard::enter("Local Servers");
                assert_eq!(get_breadcrumbs(), vec!["Dashboard", "Local Servers"]);
                {
                    let _n3 = NavGuard::enter("my-server");
                    assert_eq!(get_breadcrumbs(), vec!["Dashboard", "Local Servers", "my-server"]);
                }
                assert_eq!(get_breadcrumbs(), vec!["Dashboard", "Local Servers"]);
            }
            assert_eq!(get_breadcrumbs(), vec!["Dashboard"]);
        }
        assert!(get_breadcrumbs().is_empty());
    }

    #[test]
    fn test_format_breadcrumbs_squeezing() {
        let crumbs = vec![
            "Dashboard".to_string(),
            "Backup Systems".to_string(),
            "Local Storage".to_string(),
            "Default Storage".to_string(),
        ];

        // Plenty of width
        let full = format_breadcrumbs(&crumbs, 100);
        assert!(full.contains("Backup Systems"));
        assert!(full.contains("Default Storage"));

        // Moderate width: should squeeze middle
        let squeezed = format_breadcrumbs(&crumbs, 45);
        assert!(squeezed.contains("..."));
        assert!(squeezed.contains("Dashboard"));
        assert!(squeezed.contains("Default Storage"));
        assert!(squeezed.chars().count() <= 45);

        // Very narrow width
        let narrow = format_breadcrumbs(&crumbs, 20);
        assert!(narrow.chars().count() <= 20);
    }
}
