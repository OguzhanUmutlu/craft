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

        // 2. Poll for local keyboard input
        if event::poll(Duration::from_millis(15)).unwrap_or(false) {
            if let Ok(Event::Key(key)) = event::read() {
                // Check for Ctrl+B
                if key.modifiers.contains(KeyModifiers::CONTROL) && key.code == KeyCode::Char('b') {
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
                    ctrl_b_pressed = false;
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
                        let _ = channel.write_all(b"\x08");
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
                    KeyCode::Tab => {
                        let _ = channel.write_all(b"\t");
                    }
                    _ => {}
                }
                let _ = channel.flush();
            }
        }
    }

    Ok(())
}
