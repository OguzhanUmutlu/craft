use std::io::{Read, Write};
use std::time::Duration;
use crossterm::event::{self, Event, KeyCode, KeyModifiers};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode};
use craft_core::{CraftError, Result};
use crate::session::RemoteSession;

pub fn run_remote_pty_session(session: &RemoteSession, remote_command: &str) -> Result<()> {
    let mut channel = session.session.channel_session()
        .map_err(|e| CraftError::Other(format!("Failed to open channel session: {}", e)))?;

    let (cols, rows) = crossterm::terminal::size().unwrap_or((80, 24));
    channel.request_pty("xterm-256color", None, Some((cols as u32, rows as u32, 0, 0)))
        .map_err(|e| CraftError::Other(format!("Failed to request remote PTY: {}", e)))?;

    channel.exec(remote_command)
        .map_err(|e| CraftError::Other(format!("Failed to execute remote command '{}': {}", remote_command, e)))?;

    enable_raw_mode().map_err(|e| CraftError::Other(format!("Failed to enable local raw mode: {}", e)))?;

    session.session.set_blocking(false);
    let res = pump_pty(&mut channel);
    session.session.set_blocking(true);

    let _ = disable_raw_mode();
    res
}

fn pump_pty(channel: &mut ssh2::Channel) -> Result<()> {
    let mut stdout = std::io::stdout();
    let mut buf = [0u8; 4096];
    let mut ctrl_b_pressed = false;

    let (mut last_cols, mut last_rows) = crossterm::terminal::size().unwrap_or((80, 24));
    let mut last_size_check = std::time::Instant::now();

    loop {
        // 1. Read from remote channel and output to local terminal
        match channel.read(&mut buf) {
            Ok(count) if count > 0 => {
                let _ = stdout.write_all(&buf[..count]);
                let _ = stdout.flush();
            }
            Ok(_) => {}
            Err(ref e) if e.kind() == std::io::ErrorKind::WouldBlock => {}
            Err(_) => break,
        }

        if channel.eof() {
            break;
        }

        // 2. Periodic terminal resize check (every 250ms)
        if last_size_check.elapsed() > Duration::from_millis(250) {
            last_size_check = std::time::Instant::now();
            if let Ok((cur_cols, cur_rows)) = crossterm::terminal::size() {
                if cur_cols != last_cols || cur_rows != last_rows {
                    last_cols = cur_cols;
                    last_rows = cur_rows;
                    let _ = channel.request_pty_size(cur_cols as u32, cur_rows as u32, None, None);
                }
            }
        }

        // 3. Poll for local keyboard input
        if event::poll(Duration::from_millis(15)).unwrap_or(false) {
            match event::read() {
                Ok(Event::Key(key)) => {
                    // Check for Ctrl+B
                    if key.modifiers.contains(KeyModifiers::CONTROL) && (key.code == KeyCode::Char('b') || key.code == KeyCode::Char('B')) {
                        ctrl_b_pressed = true;
                        continue;
                    }

                    // If Ctrl+B was pressed, check for 'd' (detach)
                    if ctrl_b_pressed {
                        if key.code == KeyCode::Char('d') || key.code == KeyCode::Char('D') {
                            println!("\r\n\x1b[33m--- Detached from remote console session ---\x1b[0m\r\n");
                            let _ = channel.close();
                            return Ok(());
                        }
                        // If not 'd', send both Ctrl+B (\x02) and continue processing current key
                        let _ = channel.write_all(b"\x02");
                        ctrl_b_pressed = false;
                    }

                    // Handle control modifiers on characters
                    if key.modifiers.contains(KeyModifiers::CONTROL) {
                        if let KeyCode::Char(c) = key.code {
                            if c.is_ascii_alphabetic() {
                                let ctrl_byte = (c.to_ascii_lowercase() as u8) - b'a' + 1;
                                let _ = channel.write_all(&[ctrl_byte]);
                                let _ = channel.flush();
                                continue;
                            }
                        }
                    }

                    // Send key sequence to remote channel
                    match key.code {
                        KeyCode::Char(c) => {
                            let mut b = [0u8; 4];
                            let s = c.encode_utf8(&mut b);
                            let _ = channel.write_all(s.as_bytes());
                        }
                        KeyCode::Enter => {
                            let _ = channel.write_all(b"\r");
                        }
                        KeyCode::Backspace => {
                            let _ = channel.write_all(b"\x7f");
                        }
                        KeyCode::Tab => {
                            let _ = channel.write_all(b"\t");
                        }
                        KeyCode::BackTab => {
                            let _ = channel.write_all(b"\x1b[Z");
                        }
                        KeyCode::Esc => {
                            let _ = channel.write_all(b"\x1b");
                        }
                        KeyCode::Up => {
                            let _ = channel.write_all(b"\x1b[A");
                        }
                        KeyCode::Down => {
                            let _ = channel.write_all(b"\x1b[B");
                        }
                        KeyCode::Right => {
                            let _ = channel.write_all(b"\x1b[C");
                        }
                        KeyCode::Left => {
                            let _ = channel.write_all(b"\x1b[D");
                        }
                        KeyCode::PageUp => {
                            let _ = channel.write_all(b"\x1b[5~");
                        }
                        KeyCode::PageDown => {
                            let _ = channel.write_all(b"\x1b[6~");
                        }
                        KeyCode::Home => {
                            let _ = channel.write_all(b"\x1b[H");
                        }
                        KeyCode::End => {
                            let _ = channel.write_all(b"\x1b[F");
                        }
                        KeyCode::Insert => {
                            let _ = channel.write_all(b"\x1b[2~");
                        }
                        KeyCode::Delete => {
                            let _ = channel.write_all(b"\x1b[3~");
                        }
                        KeyCode::F(1) => { let _ = channel.write_all(b"\x1bOP"); }
                        KeyCode::F(2) => { let _ = channel.write_all(b"\x1bOQ"); }
                        KeyCode::F(3) => { let _ = channel.write_all(b"\x1bOR"); }
                        KeyCode::F(4) => { let _ = channel.write_all(b"\x1bOS"); }
                        KeyCode::F(5) => { let _ = channel.write_all(b"\x1b[15~"); }
                        KeyCode::F(6) => { let _ = channel.write_all(b"\x1b[17~"); }
                        KeyCode::F(7) => { let _ = channel.write_all(b"\x1b[18~"); }
                        KeyCode::F(8) => { let _ = channel.write_all(b"\x1b[19~"); }
                        KeyCode::F(9) => { let _ = channel.write_all(b"\x1b[20~"); }
                        KeyCode::F(10) => { let _ = channel.write_all(b"\x1b[21~"); }
                        KeyCode::F(11) => { let _ = channel.write_all(b"\x1b[23~"); }
                        KeyCode::F(12) => { let _ = channel.write_all(b"\x1b[24~"); }
                        _ => {}
                    }
                    let _ = channel.flush();
                }
                Ok(Event::Resize(cols, rows)) => {
                    last_cols = cols;
                    last_rows = rows;
                    let _ = channel.request_pty_size(cols as u32, rows as u32, None, None);
                }
                _ => {}
            }
        }
    }

    let _ = crossterm::execute!(stdout, crossterm::cursor::Show);
    Ok(())
}
