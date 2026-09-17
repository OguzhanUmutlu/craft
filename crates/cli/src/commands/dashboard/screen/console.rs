use colored::Colorize;
use craft_core::{CraftPaths, Result};
use craft_daemon::DaemonClient;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{Event, KeyCode, KeyEventKind, KeyModifiers, MouseEvent, MouseEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode},
};
use std::fs::File;
use std::io::{self, Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};

use super::terminal::get_terminal_size;
use super::theme::strip_ansi;
use super::AltScreenGuard;

/// Runs a responsive, virtual-scrolling live console for an active server.
/// - Keeps a persistent `> ` command prompt at the bottom.
/// - Redraws incoming logs smoothly without clobbering or shifting user input.
/// - Supports readline editing (Ctrl+Backspace, Ctrl+W, Ctrl+A/E/U/K, Backspace, Delete).
/// - Supports history navigation and scrolling via Up/Down and PageUp/PageDown.
/// - Supports chunked seek-based log file reading for virtual scrolling without RAM buildup.
/// - Cleanly detaches on Ctrl+C or Esc.
pub async fn run_virtual_console(
    server_name: &str,
    server_path: &Path,
    paths: &CraftPaths,
) -> Result<()> {
    if !DaemonClient::is_daemon_running(paths) {
        return Err(craft_core::CraftError::Other(
            "Craft daemon is not running. Start the server first.".to_string(),
        ));
    }

    let client = DaemonClient::connect(paths).await?;
    let (backlog, tx_to_daemon, mut rx_from_daemon) =
        client.attach_console_stream(server_path).await?;

    let _alt = AltScreenGuard::enter();
    enable_raw_mode()?;
    let mut stdout = io::stdout();

    let mut lines: Vec<String> = Vec::with_capacity(200);
    for raw_line in backlog.split('\n') {
        let trimmed = raw_line.trim_end_matches('\r');
        if !trimmed.trim().is_empty() {
            lines.push(trimmed.to_string());
            if lines.len() > 200 {
                lines.remove(0);
            }
        }
    }

    let mut input_buffer = String::new();
    let mut cursor_pos = 0usize;
    let mut history: Vec<String> = Vec::new();
    let mut history_idx: Option<usize> = None;
    let mut scroll_offset = 0usize;
    let mut partial_chunk = String::new();

    // Channel for asynchronous event reception
    let (event_tx, mut event_rx) = tokio::sync::mpsc::channel::<Event>(64);
    let _event_task = tokio::task::spawn_blocking(move || {
        while crossterm::event::poll(std::time::Duration::from_millis(50)).is_ok() {
            if let Ok(event) = crossterm::event::read() {
                if event_tx.blocking_send(event).is_err() {
                    break;
                }
            }
        }
    });

    render(
        &mut stdout,
        server_name,
        server_path,
        &lines,
        scroll_offset,
        &input_buffer,
        cursor_pos,
    )?;

    loop {
        tokio::select! {
            Some(chunk) = rx_from_daemon.recv() => {
                partial_chunk.push_str(&chunk);
                while let Some(idx) = partial_chunk.find('\n') {
                    let line = partial_chunk[..idx].trim_end_matches('\r').to_string();
                    partial_chunk = partial_chunk[idx + 1..].to_string();
                    if !line.trim().is_empty() {
                        lines.push(line);
                        if lines.len() > 200 {
                            lines.remove(0);
                        }
                    }
                }
                render(
                    &mut stdout,
                    server_name,
                    server_path,
                    &lines,
                    scroll_offset,
                    &input_buffer,
                    cursor_pos,
                )?;
            }

            Some(event) = event_rx.recv() => {
                match event {
                    Event::Key(key) => {
                        if key.kind != KeyEventKind::Press {
                            continue;
                        }

                        // Detach console: Ctrl+C or Esc
                        if (key.modifiers.contains(KeyModifiers::CONTROL)
                            && (key.code == KeyCode::Char('c') || key.code == KeyCode::Char('C')))
                            || key.code == KeyCode::Char('\x03')
                            || key.code == KeyCode::Esc
                        {
                            break;
                        }

                        let (term_w, term_h) = get_terminal_size();
                        let _ = term_w;
                        let log_area_height = (term_h.saturating_sub(5)).max(1) as usize;

                        // Check for Ctrl+Backspace / Ctrl+W / Backspace variants
                        let is_ctrl_backspace = (key.modifiers.contains(KeyModifiers::CONTROL)
                            && (key.code == KeyCode::Backspace
                                || key.code == KeyCode::Char('h')
                                || key.code == KeyCode::Char('w')
                                || key.code == KeyCode::Char('W')))
                            || key.code == KeyCode::Char('\x08')
                            || key.code == KeyCode::Char('\x17')
                            || (key.code == KeyCode::Char('\x7f') && key.modifiers.contains(KeyModifiers::CONTROL));

                        if is_ctrl_backspace {
                            // Delete word backward
                            delete_word_backward(&mut input_buffer, &mut cursor_pos);
                        } else {
                            match key.code {
                                KeyCode::Enter => {
                                    let trimmed = input_buffer.trim().to_string();
                                    if !trimmed.is_empty() {
                                        let _ = tx_to_daemon.send(format!("{}\n", trimmed)).await;
                                        history.push(trimmed.clone());
                                        lines.push(format!("> {}", trimmed));
                                        if lines.len() > 200 {
                                            lines.remove(0);
                                        }
                                        input_buffer.clear();
                                        cursor_pos = 0;
                                        history_idx = None;
                                        scroll_offset = 0;
                                    }
                                }

                                KeyCode::Backspace => {
                                    if cursor_pos > 0 && !input_buffer.is_empty() {
                                        if let Some((prev_idx, _)) = input_buffer[..cursor_pos].char_indices().last() {
                                            input_buffer.remove(prev_idx);
                                            cursor_pos = prev_idx;
                                        }
                                    }
                                }

                                KeyCode::Delete => {
                                    if cursor_pos < input_buffer.len() {
                                        input_buffer.remove(cursor_pos);
                                    }
                                }

                                KeyCode::Left => {
                                    cursor_pos = input_buffer[..cursor_pos]
                                        .char_indices()
                                        .last()
                                        .map(|(idx, _)| idx)
                                        .unwrap_or(0);
                                }

                                KeyCode::Right => {
                                    cursor_pos = input_buffer[cursor_pos..]
                                        .chars()
                                        .next()
                                        .map(|ch| cursor_pos + ch.len_utf8())
                                        .unwrap_or(input_buffer.len());
                                }

                                KeyCode::Home => {
                                    if input_buffer.is_empty() {
                                        // Scroll backward into history
                                        scroll_offset = scroll_offset.saturating_add(200);
                                    } else {
                                        cursor_pos = 0;
                                    }
                                }

                                KeyCode::End => {
                                    if scroll_offset > 0 {
                                        // Return to live output
                                        scroll_offset = 0;
                                    } else {
                                        cursor_pos = input_buffer.len();
                                    }
                                }

                                KeyCode::PageUp => {
                                    let step = (log_area_height / 2).max(1);
                                    scroll_offset = scroll_offset.saturating_add(step);
                                }

                                KeyCode::PageDown => {
                                    let step = (log_area_height / 2).max(1);
                                    scroll_offset = scroll_offset.saturating_sub(step);
                                }

                                KeyCode::Up => {
                                    let is_nav_mod = key.modifiers.contains(KeyModifiers::SHIFT)
                                        || key.modifiers.contains(KeyModifiers::CONTROL)
                                        || key.modifiers.contains(KeyModifiers::ALT);
                                    if is_nav_mod || scroll_offset > 0 || (input_buffer.is_empty() && history.is_empty()) {
                                        scroll_offset = scroll_offset.saturating_add(1);
                                    } else if !history.is_empty() {
                                        let next_idx = match history_idx {
                                            None => history.len() - 1,
                                            Some(idx) => idx.saturating_sub(1),
                                        };
                                        history_idx = Some(next_idx);
                                        input_buffer = history[next_idx].clone();
                                        cursor_pos = input_buffer.len();
                                    }
                                }

                                KeyCode::Down => {
                                    let is_nav_mod = key.modifiers.contains(KeyModifiers::SHIFT)
                                        || key.modifiers.contains(KeyModifiers::CONTROL)
                                        || key.modifiers.contains(KeyModifiers::ALT);
                                    if is_nav_mod || scroll_offset > 0 {
                                        scroll_offset = scroll_offset.saturating_sub(1);
                                    } else if let Some(idx) = history_idx {
                                        if idx + 1 < history.len() {
                                            let next_idx = idx + 1;
                                            history_idx = Some(next_idx);
                                            input_buffer = history[next_idx].clone();
                                            cursor_pos = input_buffer.len();
                                        } else {
                                            history_idx = None;
                                            input_buffer.clear();
                                            cursor_pos = 0;
                                        }
                                    }
                                }

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

                                KeyCode::Char(c) if !c.is_control() => {
                                    input_buffer.insert(cursor_pos, c);
                                    cursor_pos += c.len_utf8();
                                }

                                _ => {}
                            }
                        }

                        render(
                            &mut stdout,
                            server_name,
                            server_path,
                            &lines,
                            scroll_offset,
                            &input_buffer,
                            cursor_pos,
                        )?;
                    }

                    Event::Mouse(MouseEvent { kind, .. }) => {
                        match kind {
                            MouseEventKind::ScrollUp => {
                                scroll_offset = scroll_offset.saturating_add(3);
                                render(
                                    &mut stdout,
                                    server_name,
                                    server_path,
                                    &lines,
                                    scroll_offset,
                                    &input_buffer,
                                    cursor_pos,
                                )?;
                            }
                            MouseEventKind::ScrollDown => {
                                scroll_offset = scroll_offset.saturating_sub(3);
                                render(
                                    &mut stdout,
                                    server_name,
                                    server_path,
                                    &lines,
                                    scroll_offset,
                                    &input_buffer,
                                    cursor_pos,
                                )?;
                            }
                            _ => {}
                        }
                    }

                    Event::Resize(_, _) => {
                        render(
                            &mut stdout,
                            server_name,
                            server_path,
                            &lines,
                            scroll_offset,
                            &input_buffer,
                            cursor_pos,
                        )?;
                    }

                    Event::Paste(text) => {
                        for c in text.chars() {
                            if !c.is_control() {
                                input_buffer.insert(cursor_pos, c);
                                cursor_pos += c.len_utf8();
                            }
                        }
                        render(
                            &mut stdout,
                            server_name,
                            server_path,
                            &lines,
                            scroll_offset,
                            &input_buffer,
                            cursor_pos,
                        )?;
                    }

                    _ => {}
                }
            }
        }
    }

    let _ = disable_raw_mode();
    let _ = execute!(stdout, Hide);
    Ok(())
}

fn delete_word_backward(buffer: &mut String, cursor_pos: &mut usize) {
    if *cursor_pos == 0 || buffer.is_empty() {
        return;
    }
    let before = &buffer[..*cursor_pos];
    let trimmed = before.trim_end();
    let word_start = trimmed.rfind(' ').map(|idx| idx + 1).unwrap_or(0);
    let after = buffer[*cursor_pos..].to_string();
    *buffer = format!("{}{}", &buffer[..word_start], after);
    *cursor_pos = word_start;
}

fn find_log_file(server_path: &Path) -> Option<PathBuf> {
    let candidates = [
        server_path.join("logs").join("latest.log"),
        server_path.join("logs").join("console.log"),
        server_path.join("server.log"),
    ];
    candidates.into_iter().find(|c| c.exists() && c.is_file())
}

/// Reads a window of lines from the end of the log file using backward chunks.
/// Never loads the entire file into memory.
fn read_log_window_from_file(
    file_path: &Path,
    scroll_offset: usize,
    count: usize,
) -> io::Result<Vec<String>> {
    let mut file = File::open(file_path)?;
    let file_len = file.metadata()?.len();
    if file_len == 0 {
        return Ok(Vec::new());
    }

    const CHUNK_SIZE: usize = 16 * 1024;
    let mut current_pos = file_len;
    let mut buffer = Vec::new();
    let target_lines = scroll_offset + count;

    while current_pos > 0 {
        let read_pos = current_pos.saturating_sub(CHUNK_SIZE as u64);
        let bytes_to_read = (current_pos - read_pos) as usize;
        file.seek(SeekFrom::Start(read_pos))?;

        let mut chunk = vec![0u8; bytes_to_read];
        file.read_exact(&mut chunk)?;

        chunk.extend_from_slice(&buffer);
        buffer = chunk;
        current_pos = read_pos;

        let newline_count = buffer.iter().filter(|&&b| b == b'\n').count();
        if newline_count >= target_lines + 20 {
            break;
        }
    }

    let text = String::from_utf8_lossy(&buffer);
    let mut lines: Vec<String> = Vec::new();
    for line in text.split('\n') {
        let trimmed = line.trim_end_matches('\r').to_string();
        if !trimmed.trim().is_empty() {
            lines.push(trimmed);
        }
    }

    let total = lines.len();
    let end = total.saturating_sub(scroll_offset);
    let start = end.saturating_sub(count);
    Ok(lines[start..end].to_vec())
}

fn render<W: Write>(
    out: &mut W,
    server_name: &str,
    server_path: &Path,
    lines: &[String],
    scroll_offset: usize,
    input_buffer: &str,
    cursor_pos: usize,
) -> Result<()> {
    let (term_w, term_h) = get_terminal_size();
    let width = (term_w as usize).max(40);
    let inner_width = width.saturating_sub(2);
    let log_area_height = (term_h.saturating_sub(5)).max(1) as usize;

    execute!(out, MoveTo(0, 0))?;

    // Row 0: Header top
    let title = format!(" LIVE CONSOLE: {} ", server_name);
    let dash_count = inner_width.saturating_sub(title.len());
    let top_border = format!(
        "╭{}{}{}╮\x1B[K\r\n",
        "─".repeat(2),
        title.cyan().bold(),
        "─".repeat(dash_count.saturating_sub(2))
    );
    out.write_all(top_border.as_bytes())?;

    // Row 1: Subtitle
    let sub = " [↑/↓/PgUp/PgDn] Scroll  |  [Ctrl+Backspace] Delete Word  |  [Ctrl+C/Esc] Detach ";
    let sub_len = strip_ansi(sub).len();
    let sub_pad = inner_width.saturating_sub(sub_len);
    let sub_line = format!("│{}{}{}│\x1B[K\r\n", sub.dimmed(), " ".repeat(sub_pad), "");
    out.write_all(sub_line.as_bytes())?;

    // Row 2: Divider
    let div_line = format!("├{}┤\x1B[K\r\n", "─".repeat(inner_width));
    out.write_all(div_line.as_bytes())?;

    // Rows 3 .. (term_h - 2): Log viewport
    let visible_lines: Vec<String> = if scroll_offset == 0 {
        let total = lines.len();
        let start = total.saturating_sub(log_area_height);
        lines[start..total].to_vec()
    } else if let Some(log_file) = find_log_file(server_path) {
        if let Ok(file_lines) = read_log_window_from_file(&log_file, scroll_offset, log_area_height)
        {
            if !file_lines.is_empty() {
                file_lines
            } else {
                let total = lines.len();
                let end = total.saturating_sub(scroll_offset);
                let start = end.saturating_sub(log_area_height);
                lines[start..end].to_vec()
            }
        } else {
            let total = lines.len();
            let end = total.saturating_sub(scroll_offset);
            let start = end.saturating_sub(log_area_height);
            lines[start..end].to_vec()
        }
    } else {
        let total = lines.len();
        let end = total.saturating_sub(scroll_offset);
        let start = end.saturating_sub(log_area_height);
        lines[start..end].to_vec()
    };

    for i in 0..log_area_height {
        if let Some(log_line) = visible_lines.get(i) {
            let stripped = strip_ansi(log_line);
            let vis_len = stripped.chars().count();
            let truncated = if vis_len > inner_width {
                craft_core::truncate_str(&stripped, inner_width).to_string()
            } else {
                log_line.clone()
            };
            let pad = inner_width.saturating_sub(strip_ansi(&truncated).chars().count());
            let row = format!(
                "│ {}{} │\x1B[K\r\n",
                truncated,
                " ".repeat(pad.saturating_sub(2))
            );
            out.write_all(row.as_bytes())?;
        } else {
            let row = format!("│{}│\x1B[K\r\n", " ".repeat(inner_width));
            out.write_all(row.as_bytes())?;
        }
    }

    // Row term_h - 2: Bottom border / scroll indicator
    let bottom_line = if scroll_offset > 0 {
        let badge = format!(
            " [▲ SCROLLED +{} LINES | PRESS END TO RETURN] ",
            scroll_offset
        );
        let badge_len = strip_ansi(&badge).chars().count();
        let b_pad = inner_width.saturating_sub(badge_len);
        format!(
            "├{}{}{}┤\x1B[K\r\n",
            "─".repeat(2),
            badge.yellow().bold(),
            "─".repeat(b_pad.saturating_sub(2))
        )
    } else {
        format!("╰{}╯\x1B[K\r\n", "─".repeat(inner_width))
    };
    out.write_all(bottom_line.as_bytes())?;

    // Row term_h - 1: Persistent Command Prompt
    let prefix = "> ";
    let prefix_len = prefix.len();
    let max_display_len = width.saturating_sub(prefix_len + 4);

    let (display_str, visual_cursor_offset) = if input_buffer.chars().count() > max_display_len {
        let chars: Vec<char> = input_buffer.chars().collect();
        let char_cursor = input_buffer[..cursor_pos].chars().count();
        let start = char_cursor.saturating_sub(max_display_len.saturating_sub(4));
        let end = (start + max_display_len).min(chars.len());
        let disp: String = chars[start..end].iter().collect();
        let offset = char_cursor.saturating_sub(start);
        (disp, offset)
    } else {
        (
            input_buffer.to_string(),
            input_buffer[..cursor_pos].chars().count(),
        )
    };

    let prompt_row = format!("{}{}\x1B[K", prefix.cyan().bold(), display_str);
    out.write_all(prompt_row.as_bytes())?;

    // Position hardware cursor directly on active edit character
    let cursor_x = (prefix_len + visual_cursor_offset) as u16;
    let cursor_y = term_h.saturating_sub(1);
    execute!(out, MoveTo(cursor_x, cursor_y), Show)?;
    out.flush()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_console_word_deletion() {
        let mut buf = "stop confirm now".to_string();
        let mut pos = buf.len();
        delete_word_backward(&mut buf, &mut pos);
        assert_eq!(buf, "stop confirm ");
        assert_eq!(pos, 13);

        delete_word_backward(&mut buf, &mut pos);
        assert_eq!(buf, "stop ");
        assert_eq!(pos, 5);
    }

    #[test]
    fn test_read_log_window_from_file() {
        let temp_dir = std::env::temp_dir().join(format!("craft_test_log_{}", std::process::id()));
        let _ = std::fs::create_dir_all(&temp_dir);
        let log_file = temp_dir.join("latest.log");
        let sample = "\n\nline 1\n\nline 2\nline 3\n\nline 4\nline 5\n";
        std::fs::write(&log_file, sample).unwrap();

        // Reading last 3 lines (offset 0)
        let lines = read_log_window_from_file(&log_file, 0, 3).unwrap();
        assert_eq!(lines, vec!["line 3", "line 4", "line 5"]);

        // Reading with offset 2
        let lines_offset = read_log_window_from_file(&log_file, 2, 3).unwrap();
        assert_eq!(lines_offset, vec!["line 1", "line 2", "line 3"]);

        let _ = std::fs::remove_dir_all(&temp_dir);
    }
}
