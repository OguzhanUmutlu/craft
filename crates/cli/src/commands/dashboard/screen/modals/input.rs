use colored::Colorize;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyModifiers},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};
use std::io::{self, Write};

use craft_core::Result;
use super::super::clean_exit;
use super::super::frame::BoxFrame;
use super::super::keys::{KeyAction, KeyHelpMode, KeyMap};
use super::super::terminal::{get_content_width, is_terminal_too_small, wait_for_valid_size};
use super::super::theme::{DIM, RESET};

/// Outcome returned by running an `InputModal`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InputOutcome {
    Submitted(String),
    Cancelled,
}

pub type InputValidator = Box<dyn Fn(&str) -> std::result::Result<(), String>>;

/// A fully boxed, responsive readline input modal.
pub struct InputModal {
    pub title: String,
    pub prompt: String,
    pub default_val: Option<String>,
    pub placeholder: Option<String>,
    pub is_password: bool,
    pub keymap: KeyMap,
    pub max_width: u16,
    pub validator: Option<InputValidator>,
}

#[allow(dead_code)]
impl InputModal {
    /// Creates a new `InputModal`.
    pub fn new(title: impl Into<String>, prompt: impl Into<String>) -> Self {
        Self {
            title: title.into(),
            prompt: prompt.into(),
            default_val: None,
            placeholder: None,
            is_password: false,
            keymap: KeyMap::default(),
            max_width: 80,
            validator: None,
        }
    }

    /// Sets default value for input.
    pub fn with_default(mut self, val: impl Into<String>) -> Self {
        self.default_val = Some(val.into());
        self
    }

    /// Sets placeholder shown when input buffer is empty.
    pub fn with_placeholder(mut self, placeholder: impl Into<String>) -> Self {
        self.placeholder = Some(placeholder.into());
        self
    }

    /// Sets whether input characters should be masked with `*`.
    pub fn with_password(mut self, is_pw: bool) -> Self {
        self.is_password = is_pw;
        self
    }

    /// Sets an input validator function.
    pub fn with_validator(mut self, v: impl Fn(&str) -> std::result::Result<(), String> + 'static) -> Self {
        self.validator = Some(Box::new(v));
        self
    }

    /// Sets maximum desired box content width.
    pub fn with_max_width(mut self, width: u16) -> Self {
        self.max_width = width;
        self
    }

    /// Runs the interactive input modal loop.
    pub fn run(&self) -> Result<InputOutcome> {
        let mut stdout = io::stdout();
        enable_raw_mode()?;

        let mut buffer = if self.is_password {
            String::new()
        } else {
            self.default_val.clone().unwrap_or_default()
        };
        let mut cursor_pos = buffer.len();
        let mut validation_error: Option<String> = None;

        let res = (|| -> Result<InputOutcome> {
            loop {
                if is_terminal_too_small() {
                    wait_for_valid_size(&mut stdout)?;
                    continue;
                }

                let width = get_content_width(self.max_width);
                let inner_width = width.saturating_sub(6);

                let mut frame = BoxFrame::new(width);
                frame.title = Some((self.title.clone(), false));

                for line in self.prompt.lines() {
                    frame.row(line.white().bold().to_string());
                }
                frame.empty_row();

                // Compute horizontal scroll and visual display string
                let char_count = buffer.chars().count();
                let char_cursor = buffer[..cursor_pos].chars().count();
                let max_display_len = inner_width.saturating_sub(4);

                let (display_str, visual_cursor_offset) = if self.is_password {
                    let masked = "*".repeat(char_count);
                    if char_count > max_display_len {
                        let start = char_cursor.saturating_sub(max_display_len.saturating_sub(4));
                        let end = (start + max_display_len).min(char_count);
                        ("*".repeat(end - start), char_cursor.saturating_sub(start))
                    } else {
                        (masked, char_cursor)
                    }
                } else if buffer.is_empty() {
                    let placeholder = self.placeholder.as_deref()
                        .or(self.default_val.as_deref())
                        .unwrap_or("");
                    (format!("{}{}{}", DIM, placeholder, RESET), 0)
                } else if char_count > max_display_len {
                    let chars: Vec<char> = buffer.chars().collect();
                    let start = char_cursor.saturating_sub(max_display_len.saturating_sub(4));
                    let end = (start + max_display_len).min(chars.len());
                    let slice: String = chars[start..end].iter().collect();
                    (slice, char_cursor.saturating_sub(start))
                } else {
                    (buffer.clone(), char_cursor)
                };

                let input_row = format!("> {}", display_str);
                frame.row(input_row);

                if let Some(ref err) = validation_error {
                    frame.row(format!("! {}", err).red().bold().to_string());
                }

                frame.footer(self.keymap.footer_help_text(KeyHelpMode::Input));
                frame.render(&mut stdout)?;

                // Position cursor precisely on the input line
                // Overhead lines before input row:
                // Box top (1) + title & divider (2) + prompt lines count + empty row (1)
                let prompt_lines_count = self.prompt.lines().count();
                let cursor_y = 1 + 2 + prompt_lines_count as u16 + 1;
                // Column: Border (1) + 2 spaces padding + "> " (2) = 5
                let cursor_x = 5 + visual_cursor_offset as u16;

                execute!(stdout, MoveTo(cursor_x, cursor_y), Show)?;
                stdout.flush()?;

                match event::read()? {
                    Event::Resize(..) => continue,
                    Event::Key(key) => {
                        let action = self.keymap.resolve(&key);
                        match action {
                            KeyAction::Quit => {
                                clean_exit();
                            }
                            KeyAction::Cancel => {
                                return Ok(InputOutcome::Cancelled);
                            }
                            KeyAction::Submit => {
                                let trimmed = buffer.trim();
                                let final_val = if trimmed.is_empty() {
                                    if let Some(ref def) = self.default_val {
                                        def.clone()
                                    } else {
                                        trimmed.to_string()
                                    }
                                } else {
                                    trimmed.to_string()
                                };

                                if let Some(ref validator) = self.validator {
                                    if let Err(e) = validator(&final_val) {
                                        validation_error = Some(e);
                                        continue;
                                    }
                                }
                                return Ok(InputOutcome::Submitted(final_val));
                            }
                            KeyAction::Backspace => {
                                validation_error = None;
                                if cursor_pos > 0 && !buffer.is_empty() {
                                    if let Some((prev_idx, _)) = buffer[..cursor_pos].char_indices().last() {
                                        buffer.remove(prev_idx);
                                        cursor_pos = prev_idx;
                                    }
                                }
                            }
                            KeyAction::Delete => {
                                validation_error = None;
                                if cursor_pos < buffer.len() {
                                    buffer.remove(cursor_pos);
                                }
                            }
                            KeyAction::Left => {
                                cursor_pos = buffer[..cursor_pos]
                                    .char_indices()
                                    .last()
                                    .map(|(idx, _)| idx)
                                    .unwrap_or(0);
                            }
                            KeyAction::Right => {
                                cursor_pos = buffer[cursor_pos..]
                                    .chars()
                                    .next()
                                    .map(|ch| cursor_pos + ch.len_utf8())
                                    .unwrap_or(buffer.len());
                            }
                            KeyAction::Home => {
                                cursor_pos = 0;
                            }
                            KeyAction::End => {
                                cursor_pos = buffer.len();
                            }
                            KeyAction::DeleteWord => {
                                validation_error = None;
                                let before = &buffer[..cursor_pos];
                                let trimmed = before.trim_end();
                                let word_start = trimmed.rfind(' ').map(|idx| idx + 1).unwrap_or(0);
                                let after = buffer[cursor_pos..].to_string();
                                buffer = format!("{}{}", &buffer[..word_start], after);
                                cursor_pos = word_start;
                            }
                            KeyAction::ClearInput => {
                                validation_error = None;
                                buffer.clear();
                                cursor_pos = 0;
                            }
                            _ => {
                                if let KeyCode::Char(c) = key.code {
                                    if !c.is_control() && (!key.modifiers.contains(KeyModifiers::CONTROL) && !key.modifiers.contains(KeyModifiers::ALT)) {
                                        validation_error = None;
                                        buffer.insert(cursor_pos, c);
                                        cursor_pos += c.len_utf8();
                                    }
                                }
                            }
                        }
                    }
                    _ => {}
                }
            }
        })();

        let _ = execute!(stdout, Hide);
        let _ = disable_raw_mode();
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_input_modal_builder() {
        let modal = InputModal::new("INPUT PROMPT", "Enter value:")
            .with_default("default123")
            .with_placeholder("e.g. 25565")
            .with_password(true)
            .with_validator(|v| {
                if v.len() < 3 {
                    Err("Must be at least 3 characters".to_string())
                } else {
                    Ok(())
                }
            });

        assert_eq!(modal.title, "INPUT PROMPT");
        assert_eq!(modal.prompt, "Enter value:");
        assert_eq!(modal.default_val.as_deref(), Some("default123"));
        assert_eq!(modal.placeholder.as_deref(), Some("e.g. 25565"));
        assert!(modal.is_password);
        assert!(modal.validator.is_some());
    }
}
