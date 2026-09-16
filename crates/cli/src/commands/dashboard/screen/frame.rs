use colored::Colorize;
use crossterm::{
    cursor::MoveTo,
    execute,
    terminal::{Clear, ClearType},
};
use std::io::{self, Write};

use craft_core::Result;
use super::theme::{box_bottom, box_divider, box_title, box_top, is_utf8_supported, strip_ansi, BORDER_COLOR, RESET};

/// Core layout engine that encapsulates all visual TUI components inside an airtight dynamic box.
#[derive(Debug, Clone)]
pub struct BoxFrame {
    pub width: usize,
    pub title: Option<(String, bool)>,
    pub breadcrumbs: Option<String>,
    pub lines: Vec<FrameLine>,
    pub footer_help: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Clone)]
pub enum FrameLine {
    Row(String),
    RawRow(String),
    Empty,
    Divider,
    DividerDimmed,
}

#[allow(dead_code)]
impl BoxFrame {
    /// Creates a new box frame with a specified desired width (clamped to terminal bounds).
    pub fn new(width: usize) -> Self {
        Self {
            width,
            title: None,
            breadcrumbs: None,
            lines: Vec::new(),
            footer_help: None,
        }
    }

    /// Sets the modal frame title.
    pub fn with_title(mut self, title: impl Into<String>, is_error: bool) -> Self {
        self.title = Some((title.into(), is_error));
        self
    }

    /// Sets explicit navigation breadcrumbs.
    pub fn with_breadcrumbs(mut self, breadcrumbs: impl Into<String>) -> Self {
        self.breadcrumbs = Some(breadcrumbs.into());
        self
    }

    /// Adds a standardized content row enclosed in vertical side borders `│ ... │`.
    pub fn row(&mut self, content: impl AsRef<str>) -> &mut Self {
        self.lines.push(FrameLine::Row(content.as_ref().to_string()));
        self
    }

    /// Adds an empty padded row `│       │`.
    pub fn empty_row(&mut self) -> &mut Self {
        self.lines.push(FrameLine::Empty);
        self
    }

    /// Adds an internal divider `├───────┤`.
    pub fn divider(&mut self) -> &mut Self {
        self.lines.push(FrameLine::Divider);
        self
    }

    /// Adds a dimmed internal divider.
    pub fn divider_dimmed(&mut self) -> &mut Self {
        self.lines.push(FrameLine::DividerDimmed);
        self
    }

    /// Sets the bottom footer shortcut help bar.
    pub fn footer(&mut self, help_text: impl Into<String>) -> &mut Self {
        self.footer_help = Some(help_text.into());
        self
    }

    /// Formats a single content string into a framed row strictly bounded by side borders `│ ... │`.
    pub fn format_row(content: &str, width: usize) -> String {
        let border_char = if is_utf8_supported() { "│" } else { "|" };
        let inner_width = width.saturating_sub(2); // subtract 2 for border chars
        if inner_width == 0 {
            return String::new();
        }

        let clean = strip_ansi(content);
        let char_count = clean.chars().count();

        if char_count >= inner_width {
            // Content fills or exceeds inner width: truncate to fit inside inner_width
            let keep = inner_width.saturating_sub(3);
            let truncated = craft_core::truncate_ellipsis(&clean, keep);
            format!(
                "{}{}{}{}{}{}{}",
                BORDER_COLOR, border_char, RESET,
                truncated,
                BORDER_COLOR, border_char, RESET
            )
        } else {
            let padding = inner_width.saturating_sub(char_count);
            format!(
                "{}{}{} {}{}{}{}{}",
                BORDER_COLOR, border_char, RESET,
                content,
                " ".repeat(padding.saturating_sub(1)),
                BORDER_COLOR, border_char, RESET
            )
        }
    }

    /// Formats a row with custom internal padding (e.g. 2 spaces on left and right).
    pub fn format_padded_row(content: &str, width: usize, indent: usize) -> String {
        let border_char = if is_utf8_supported() { "│" } else { "|" };
        let inner_width = width.saturating_sub(2);
        if inner_width == 0 {
            return String::new();
        }

        let clean = strip_ansi(content);
        let total_content_len = clean.chars().count() + indent;

        if total_content_len >= inner_width {
            let available = inner_width.saturating_sub(indent + 3);
            let truncated = craft_core::truncate_ellipsis(&clean, available);
            let pad = inner_width.saturating_sub(indent + truncated.chars().count());
            format!(
                "{}{}{}{}{}{}{}{}{}",
                BORDER_COLOR, border_char, RESET,
                " ".repeat(indent),
                truncated,
                " ".repeat(pad),
                BORDER_COLOR, border_char, RESET
            )
        } else {
            let pad = inner_width.saturating_sub(total_content_len);
            format!(
                "{}{}{}{}{}{}{}{}{}",
                BORDER_COLOR, border_char, RESET,
                " ".repeat(indent),
                content,
                " ".repeat(pad),
                BORDER_COLOR, border_char, RESET
            )
        }
    }

    /// Renders the entire encapsulated frame to a string with CRLF line endings.
    pub fn render_to_string(&self) -> String {
        let mut out = String::new();
        let w = self.width;

        // 1. Top border
        out.push_str(&box_top(w));
        out.push_str("\r\n");

        // 2. Title & Breadcrumbs
        if let Some((ref title, is_error)) = self.title {
            out.push_str(&box_title(title, w, is_error));
            out.push_str("\r\n");
            out.push_str(&box_divider(w));
            out.push_str("\r\n");
        }

        // 3. Body lines
        for line in &self.lines {
            match line {
                FrameLine::Row(content) => {
                    out.push_str(&Self::format_padded_row(content, w, 2));
                    out.push_str("\r\n");
                }
                FrameLine::RawRow(content) => {
                    out.push_str(&Self::format_row(content, w));
                    out.push_str("\r\n");
                }
                FrameLine::Empty => {
                    out.push_str(&Self::format_row("", w));
                    out.push_str("\r\n");
                }
                FrameLine::Divider => {
                    out.push_str(&box_divider(w));
                    out.push_str("\r\n");
                }
                FrameLine::DividerDimmed => {
                    out.push_str(&format!("{}\r\n", box_divider(w).dimmed()));
                }
            }
        }

        // 4. Footer & Bottom border
        if let Some(ref help) = self.footer_help {
            out.push_str(&box_divider(w));
            out.push_str("\r\n");
            let styled_help = format!("{}{}{}", "\x1B[2m", help, RESET);
            out.push_str(&Self::format_padded_row(&styled_help, w, 2));
            out.push_str("\r\n");
        }

        out.push_str(&box_bottom(w));
        out.push_str("\r\n");

        out
    }

    /// Renders this frame directly into stdout at position (0, 0), clearing remaining lines below.
    pub fn render(&self, stdout: &mut io::Stdout) -> Result<()> {
        execute!(stdout, MoveTo(0, 0))?;
        let output = self.render_to_string();
        for line in output.lines() {
            print!("{}\x1B[K\r\n", line);
        }
        execute!(stdout, Clear(ClearType::FromCursorDown))?;
        stdout.flush()?;
        Ok(())
    }
}

/// Renders a responsive, centered warning box when terminal dimensions fall below the minimum threshold.
pub fn render_too_small(term_cols: u16, term_rows: u16, min_cols: u16, min_rows: u16) -> String {
    let card_width = (term_cols as usize).saturating_sub(4).clamp(28, 52);
    let border_char = if is_utf8_supported() { "│" } else { "|" };

    let mut out = String::new();

    // Compute top vertical padding to center the card
    let card_height = 8usize;
    let v_pad = (term_rows as usize).saturating_sub(card_height) / 2;
    for _ in 0..v_pad {
        out.push_str("\r\n");
    }

    let h_pad = (term_cols as usize).saturating_sub(card_width) / 2;
    let h_space = " ".repeat(h_pad);

    // Card Top
    out.push_str(&h_space);
    out.push_str(&box_top(card_width));
    out.push_str("\r\n");

    // Card Title
    let title = "TERMINAL TOO SMALL".red().bold().to_string();
    let clean_title = "TERMINAL TOO SMALL";
    let title_pad = card_width.saturating_sub(2 + clean_title.len()) / 2;
    let title_pad_right = card_width.saturating_sub(2 + clean_title.len() + title_pad);
    out.push_str(&h_space);
    out.push_str(&format!(
        "{}{}{}{}{}{}{}{}{}\r\n",
        BORDER_COLOR, border_char, RESET,
        " ".repeat(title_pad),
        title,
        " ".repeat(title_pad_right),
        BORDER_COLOR, border_char, RESET
    ));

    // Card Divider
    out.push_str(&h_space);
    out.push_str(&box_divider(card_width));
    out.push_str("\r\n");

    // Card Body
    let line1 = format!("Current Window:    {} x {} cols/rows", term_cols, term_rows);
    let line2 = format!("Minimum Required:  {} x {} cols/rows", min_cols, min_rows);
    let line3 = "Please resize your terminal window";
    let line4 = "or zoom out to continue.";

    for line in &[line1.as_str(), line2.as_str(), "", line3, line4] {
        out.push_str(&h_space);
        out.push_str(&BoxFrame::format_padded_row(line, card_width, 2));
        out.push_str("\r\n");
    }

    // Card Bottom
    out.push_str(&h_space);
    out.push_str(&box_bottom(card_width));
    out.push_str("\r\n");

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_box_frame_format_row_padding() {
        let width = 40;
        let formatted = BoxFrame::format_row("Hello World", width);
        let clean = strip_ansi(&formatted);
        assert_eq!(clean.chars().count(), width);
        assert!(clean.starts_with('│') || clean.starts_with('|'));
        assert!(clean.ends_with('│') || clean.ends_with('|'));
    }

    #[test]
    fn test_box_frame_render_contains_borders() {
        let mut frame = BoxFrame::new(60).with_title("TEST TITLE", false);
        frame.row("Row content 1");
        frame.empty_row();
        frame.row("Row content 2");
        frame.footer("Footer help shortcuts");

        let rendered = frame.render_to_string();
        assert!(rendered.contains("TEST TITLE"));
        assert!(rendered.contains("Row content 1"));
        assert!(rendered.contains("Footer help shortcuts"));
    }

    #[test]
    fn test_render_too_small_dimensions() {
        let small_view = render_too_small(45, 10, 60, 14);
        assert!(small_view.contains("TERMINAL TOO SMALL"));
        assert!(small_view.contains("45 x 10"));
        assert!(small_view.contains("60 x 14"));
    }
}
