use std::io::{self, IsTerminal, Write};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use colored::Colorize;
use crossterm::{
    cursor::{Hide, MoveTo, Show},
    event::{self, Event, KeyCode, KeyEventKind},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, Clear, ClearType, EnterAlternateScreen, LeaveAlternateScreen},
};
use sysinfo::System;

use craft_backup::BackupEngine;
use craft_core::{kill_process, read_pid_file, CraftError, CraftPaths, RemoteAuthType, RemoteHostConfig, RemotesRegistry, Result, ServersRegistry};
use craft_daemon::DaemonClient;
use craft_net::{ping_bedrock_server, ping_java_server};
use craft_plugins::PluginManager;
use craft_providers::{get_all_softwares, CacheManager};

use crate::commands::{
    new::handle_new,
    remote::parse_connection_string,
    run::run_foreground_server,
    view::handle_view,
};

static ALT_SCREEN_DEPTH: AtomicUsize = AtomicUsize::new(0);

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
        self.aliases = aliases.iter().map(|s| s.to_string()).collect();
        self
    }
}

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

            for (idx, entry) in entries.iter().enumerate() {
                if idx == *selected_idx {
                    print!(
                        "  \x1B[1;36m>\x1B[0m \x1B[1;97;44m{:<4} {:<70}\x1B[0m\x1B[K\r\n",
                        format!("[{}]", entry.hotkey),
                        entry.label
                    );
                } else {
                    print!(
                        "    \x1B[1;36m{:<4}\x1B[0m {:<70}\x1B[K\r\n",
                        format!("[{}]", entry.hotkey),
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
            print!("{}\x1B[K\r\n", "================================================================================".cyan().bold());
            print!("{:^80}\x1B[K\r\n", header_title.cyan().bold());
            print!("{}\x1B[K\r\n", "================================================================================".cyan().bold());
            print!("\x1B[K\r\n");
            print!("  {}\x1B[K\r\n", prompt_label.white().bold());
            print!("\x1B[K\r\n");

            let prefix = "  > ";
            let display_text = if input_buffer.is_empty() && default_val.is_some() {
                format!("\x1B[2m{}\x1B[0m", default_val.unwrap())
            } else {
                input_buffer.clone()
            };
            print!("{}{}\x1B[K\r\n", prefix.cyan().bold(), display_text);

            print!("\x1B[K\r\n{}\x1B[K\r\n", "--------------------------------------------------------------------------------".dimmed());
            print!("  \x1B[2m[Enter] Confirm  |  [Esc] Cancel  |  [Backspace] Delete\x1B[0m\x1B[K\r\n");

            execute!(stdout, Clear(ClearType::FromCursorDown))?;

            let cursor_x = (prefix.len() + cursor_pos) as u16;
            let cursor_y = 5;
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
                        if cursor_pos > 0 {
                            cursor_pos -= 1;
                        }
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
                    KeyCode::Char(c) => {
                        if !c.is_control() {
                            input_buffer.insert(cursor_pos, c);
                            cursor_pos += 1;
                        }
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

pub async fn show_empty_servers_modal(paths: &CraftPaths) -> Result<bool> {
    let header = format!(
        "{}\r\n{}\r\n{}\r\n No servers are currently registered on this machine.\r\n Create your first Minecraft server to get started.\r\n{}",
        "================================================================================".cyan().bold(),
        "                               NO SERVERS REGISTERED                            ".cyan().bold(),
        "================================================================================".cyan().bold(),
        "--------------------------------------------------------------------------------".dimmed()
    );

    let entries = vec![
        MenuEntry::new("1", "Create Your First Server (Setup Wizard)").with_aliases(&["2", "c", "n"]),
        MenuEntry::new("0", "Back to Dashboard").with_aliases(&["b", "q"]),
    ];

    let mut selected = 0;
    let choice = run_menu(&header, &entries, &mut selected)?;
    match choice {
        Some(0) => {
            gui_create_server_wizard(paths).await?;
            Ok(true)
        }
        _ => Ok(false),
    }
}

async fn exec_console_action<F, Fut>(action: F) -> Result<()>
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

fn get_system_summary() -> (String, f64, f64, f64) {
    let mut sys = System::new();
    sys.refresh_memory();
    let total_gb = sys.total_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
    let used_gb = sys.used_memory() as f64 / (1024.0 * 1024.0 * 1024.0);
    let pct = if total_gb > 0.0 { (used_gb / total_gb) * 100.0 } else { 0.0 };
    let os_name = System::name().unwrap_or_else(|| "Linux".to_string());
    (os_name, total_gb, used_gb, pct)
}

fn build_dashboard_header(
    os: &str,
    total_ram: f64,
    used_ram: f64,
    ram_pct: f64,
    daemon_online: bool,
    registered_count: usize,
    running_count: usize,
) -> String {
    let daemon_badge = if daemon_online {
        "[ONLINE]".green().bold()
    } else {
        "[OFFLINE]".yellow().bold()
    };

    format!(
        "{}\r\n{}\r\n{}\r\n Host: {:<16} | RAM: {:.1} / {:.1} GB ({:.1}%) | Daemon: {}\r\n Registered Servers: {:<4} | Active Running: {:<4}\r\n{}",
        "================================================================================".cyan().bold(),
        "                         CRAFT SERVER MANAGER DASHBOARD                         ".cyan().bold(),
        "================================================================================".cyan().bold(),
        os.white().bold(),
        used_ram,
        total_ram,
        ram_pct,
        daemon_badge,
        registered_count.to_string().cyan().bold(),
        running_count.to_string().green().bold(),
        "--------------------------------------------------------------------------------".dimmed()
    )
}

pub async fn handle_dashboard(paths: &CraftPaths) -> Result<()> {
    if !io::stdin().is_terminal() {
        println!("{}", "Craft dashboard requires an interactive terminal (TTY).".yellow());
        return Ok(());
    }

    let _guard = AltScreenGuard::enter();
    let mut selected_main = 0;

    loop {
        let registry = ServersRegistry::load(paths)?;
        let total_servers = registry.servers.len();

        let daemon_running = DaemonClient::is_daemon_running(paths);
        let running_paths = if daemon_running {
            if let Ok(mut client) = DaemonClient::connect(paths).await {
                client.get_running().await.unwrap_or_default()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        let running_count = registry
            .servers
            .iter()
            .filter(|s| running_paths.contains(&s.path) || s.path.canonicalize().map(|p| running_paths.contains(&p)).unwrap_or(false))
            .count();

        let (os_name, total_ram, used_ram, ram_pct) = get_system_summary();
        let header = build_dashboard_header(
            &os_name,
            total_ram,
            used_ram,
            ram_pct,
            daemon_running,
            total_servers,
            running_count,
        );

        let entries = vec![
            MenuEntry::new("1", "Manage Servers (Start, Stop, Restart, Console, Delete)"),
            MenuEntry::new("2", "Create New Server (Interactive Wizard)"),
            MenuEntry::new("3", "Quick Start Server"),
            MenuEntry::new("4", "Stop Running Server"),
            MenuEntry::new("5", "Restart Server"),
            MenuEntry::new("6", "Attach Live Console (craft view)"),
            MenuEntry::new("7", "Server Network Ping (Java SLP & Bedrock)"),
            MenuEntry::new("8", "World Snapshots & Backup Manager"),
            MenuEntry::new("9", "Browse & Install Plugins (Modrinth / Hangar)"),
            MenuEntry::new("r", "Remote VPS Hosts (SSH Management)"),
            MenuEntry::new("d", "Service Daemon Control (Start / Stop / Restart)"),
            MenuEntry::new("c", "Cache & Storage Management"),
            MenuEntry::new("0", "Exit Craft"),
        ];

        let selection = run_menu(&header, &entries, &mut selected_main)?;

        match selection {
            Some(0) => {
                manage_servers_menu(paths).await?;
            }
            Some(1) => {
                gui_create_server_wizard(paths).await?;
            }
            Some(2) => {
                quick_start_menu(paths).await?;
            }
            Some(3) => {
                stop_servers_menu(paths).await?;
            }
            Some(4) => {
                restart_servers_menu(paths).await?;
            }
            Some(5) => {
                view_servers_menu(paths).await?;
            }
            Some(6) => {
                ping_menu().await?;
            }
            Some(7) => {
                backups_menu(paths).await?;
            }
            Some(8) => {
                plugins_menu(paths).await?;
            }
            Some(9) => {
                remotes_menu(paths).await?;
            }
            Some(10) => {
                daemon_menu(paths).await?;
            }
            Some(11) => {
                cache_menu(paths)?;
            }
            Some(12) | None => {
                break;
            }
            _ => break,
        }
    }

    Ok(())
}

pub async fn gui_create_server_wizard(paths: &CraftPaths) -> Result<()> {
    gui_create_server_wizard_with_name("", paths).await
}

pub async fn gui_create_server_wizard_with_name(name_override: &str, paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();

    // Step 1: Server Name
    let server_name = if !name_override.trim().is_empty() {
        name_override.trim().to_string()
    } else {
        match run_input_prompt(
            "SERVER SETUP WIZARD (STEP 1/6)",
            "Enter server name:",
            Some("my-server"),
        )? {
            Some(n) if !n.trim().is_empty() => n.trim().to_string(),
            _ => return Ok(()),
        }
    };

    let registry = ServersRegistry::load(paths)?;
    if registry.servers.iter().any(|s| s.name.eq_ignore_ascii_case(&server_name)) {
        show_modal_message(
            "NAME ALREADY REGISTERED",
            &[
                format!("A server named '{}' already exists on this machine.", server_name),
                "Please choose a different name for your new server.".to_string(),
            ],
            true,
        )?;
        return Ok(());
    }

    // Step 2: Software Category
    let cat_header = format!(
        "{}\r\n{}\r\n{}\r\n Choose platform category for server '{}':\r\n{}",
        "================================================================================".cyan().bold(),
        "                       STEP 2/6: SELECT PLATFORM CATEGORY                       ".cyan().bold(),
        "================================================================================".cyan().bold(),
        server_name,
        "--------------------------------------------------------------------------------".dimmed()
    );

    let cat_entries = vec![
        MenuEntry::new("1", "Java High-Performance (Paper, Purpur, Folia, Spigot, Vanilla Java)"),
        MenuEntry::new("2", "Modded & Hybrid (Fabric, Quilt, NeoForge)"),
        MenuEntry::new("3", "Network Proxies (Velocity, Waterfall, BungeeCord, GeyserMC, WaterdogPE)"),
        MenuEntry::new("4", "Bedrock Dedicated (Vanilla Bedrock BDS, PocketMine-MP, NukkitX)"),
        MenuEntry::new("5", "Browse All 16 Platforms"),
        MenuEntry::new("0", "Cancel").with_aliases(&["b", "q"]),
    ];

    let mut cat_sel = 0;
    let cat_choice = run_menu(&cat_header, &cat_entries, &mut cat_sel)?;
    let cat_idx = match cat_choice {
        Some(idx) if idx < 5 => idx,
        _ => return Ok(()),
    };

    let software_choices: Vec<(&'static str, &'static str, &'static str)> = match cat_idx {
        0 => vec![
            ("paper", "Paper", "High-performance Minecraft Java server (Standard)"),
            ("purpur", "Purpur", "Extreme customization & gameplay optimizations"),
            ("folia", "Folia", "Multi-threaded regional ticking for massive scale"),
            ("spigot", "Spigot", "Classic Bukkit / Spigot server"),
            ("vanilla_java", "Vanilla Java", "Official Mojang Minecraft Java server"),
        ],
        1 => vec![
            ("fabric", "Fabric", "Lightweight, modular modding toolchain"),
            ("quilt", "Quilt", "Next-gen community-driven modding ecosystem"),
            ("neoforge", "NeoForge", "Modern Forge-compatible high-power modded server"),
        ],
        2 => vec![
            ("velocity", "Velocity", "Next-generation ultra-fast proxy (Standard)"),
            ("waterfall", "Waterfall", "BungeeCord fork with improved performance"),
            ("bungeecord", "BungeeCord", "Classic multi-server network proxy"),
            ("geyser", "GeyserMC Standalone", "Bridge allowing Bedrock players on Java"),
            ("waterdog", "WaterdogPE", "Native Bedrock network proxy"),
        ],
        3 => vec![
            ("vanilla_bedrock", "Vanilla Bedrock BDS", "Official Mojang Bedrock Dedicated Server"),
            ("pocketmine", "PocketMine-MP", "High-performance C++ / PHP Bedrock server"),
            ("nukkit", "NukkitX", "Java-based multi-threaded Bedrock server"),
        ],
        _ => {
            get_all_softwares()
                .into_iter()
                .map(|s| (s.id(), s.name(), "Supported Minecraft Server Platform"))
                .collect()
        }
    };

    // Step 3: Specific Software
    let sw_header = format!(
        "{}\r\n{}\r\n{}\r\n Select the server software implementation:\r\n{}",
        "================================================================================".cyan().bold(),
        "                        STEP 3/6: SELECT SERVER SOFTWARE                        ".cyan().bold(),
        "================================================================================".cyan().bold(),
        "--------------------------------------------------------------------------------".dimmed()
    );

    let mut sw_entries: Vec<MenuEntry> = software_choices
        .iter()
        .enumerate()
        .map(|(i, (_id, name, desc))| {
            let hotkey = if i < 9 { (i + 1).to_string() } else { ((b'a' + (i - 9) as u8) as char).to_string() };
            MenuEntry::new(hotkey, format!("{:<22} - {}", name, desc))
        })
        .collect();
    sw_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b", "q"]));

    let mut sw_sel = 0;
    let sw_choice = run_menu(&sw_header, &sw_entries, &mut sw_sel)?;
    let (selected_sw_id, selected_sw_name) = match sw_choice {
        Some(idx) if idx < software_choices.len() => (software_choices[idx].0, software_choices[idx].1),
        _ => return Ok(()),
    };

    // Step 4: Version
    let ver_header = format!(
        "{}\r\n{}\r\n{}\r\n Select Minecraft release version for {}:\r\n{}",
        "================================================================================".cyan().bold(),
        "                        STEP 4/6: SELECT SERVER VERSION                         ".cyan().bold(),
        "================================================================================".cyan().bold(),
        selected_sw_name,
        "--------------------------------------------------------------------------------".dimmed()
    );

    let ver_entries = vec![
        MenuEntry::new("1", "latest (Recommended - Automatically resolves latest release)"),
        MenuEntry::new("2", "1.21.4 (Latest Stable Java Release)"),
        MenuEntry::new("3", "1.21.1"),
        MenuEntry::new("4", "1.20.4"),
        MenuEntry::new("5", "1.20.1"),
        MenuEntry::new("6", "1.19.4"),
        MenuEntry::new("7", "1.18.2"),
        MenuEntry::new("8", "1.16.5"),
        MenuEntry::new("c", "Custom Version (Type Manually)"),
        MenuEntry::new("0", "Cancel").with_aliases(&["b", "q"]),
    ];

    let mut ver_sel = 0;
    let ver_choice = run_menu(&ver_header, &ver_entries, &mut ver_sel)?;
    let version = match ver_choice {
        Some(0) => "latest".to_string(),
        Some(1) => "1.21.4".to_string(),
        Some(2) => "1.21.1".to_string(),
        Some(3) => "1.20.4".to_string(),
        Some(4) => "1.20.1".to_string(),
        Some(5) => "1.19.4".to_string(),
        Some(6) => "1.18.2".to_string(),
        Some(7) => "1.16.5".to_string(),
        Some(8) => {
            match run_input_prompt(
                "CUSTOM MINECRAFT VERSION",
                "Enter target version (e.g. 1.21.3, 1.20.2):",
                Some("1.21.4"),
            )? {
                Some(v) if !v.trim().is_empty() => v.trim().to_string(),
                _ => return Ok(()),
            }
        }
        _ => return Ok(()),
    };

    // Step 5: Memory Allocation
    let (_os, total_ram, used_ram, ram_pct) = get_system_summary();
    let mem_header = format!(
        "{}\r\n{}\r\n{}\r\n Host RAM: {:.1} / {:.1} GB ({:.1}%) | Choose memory allocation limit:\r\n{}",
        "================================================================================".cyan().bold(),
        "                       STEP 5/6: ALLOCATE SERVER MEMORY                         ".cyan().bold(),
        "================================================================================".cyan().bold(),
        used_ram,
        total_ram,
        ram_pct,
        "--------------------------------------------------------------------------------".dimmed()
    );

    let mem_entries = vec![
        MenuEntry::new("1", "2G  (Standard lightweight / proxy testing)"),
        MenuEntry::new("2", "4G  (Recommended standard survival server)"),
        MenuEntry::new("3", "8G  (Large player counts / heavy plugins / mods)"),
        MenuEntry::new("4", "16G (High-capacity multi-world or network hub)"),
        MenuEntry::new("c", "Custom Limit (Type Manually)"),
        MenuEntry::new("0", "Cancel").with_aliases(&["b", "q"]),
    ];

    let mut mem_sel = 1;
    let mem_choice = run_menu(&mem_header, &mem_entries, &mut mem_sel)?;
    let memory = match mem_choice {
        Some(0) => "2G".to_string(),
        Some(1) => "4G".to_string(),
        Some(2) => "8G".to_string(),
        Some(3) => "16G".to_string(),
        Some(4) => {
            match run_input_prompt(
                "CUSTOM MEMORY LIMIT",
                "Enter memory limit with suffix (e.g. 6G, 12G, 512M):",
                Some("4G"),
            )? {
                Some(m) if !m.trim().is_empty() => m.trim().to_string(),
                _ => return Ok(()),
            }
        }
        _ => return Ok(()),
    };

    // Step 6: Autostart Preference
    let start_header = format!(
        "{}\r\n{}\r\n{}\r\n How should server '{}' be initialized upon creation?\r\n{}",
        "================================================================================".cyan().bold(),
        "                         STEP 6/6: INITIALIZATION MODE                          ".cyan().bold(),
        "================================================================================".cyan().bold(),
        server_name,
        "--------------------------------------------------------------------------------".dimmed()
    );

    let start_entries = vec![
        MenuEntry::new("1", "Start Server Immediately (Background Daemon)"),
        MenuEntry::new("2", "Create Server Only (Do not start now)"),
        MenuEntry::new("0", "Cancel").with_aliases(&["b", "q"]),
    ];

    let mut start_sel = 0;
    let start_choice = run_menu(&start_header, &start_entries, &mut start_sel)?;
    let start_now = match start_choice {
        Some(0) => true,
        Some(1) => false,
        _ => return Ok(()),
    };

    // Execution
    print_in_place_status(
        "CREATING MINECRAFT SERVER",
        &[
            format!("Setting up server '{}' ({} {})...", server_name, selected_sw_name, version),
            "Downloading server jarfile and configuring runtime environment...".to_string(),
            "Please wait...".to_string(),
        ],
    )?;

    let res = handle_new(
        &server_name,
        Some(selected_sw_id),
        Some(&version),
        None,
        Some(&memory),
        true,       // agree_eula
        false,      // tmp
        !start_now, // no_start
        true,       // yes = true (non-interactive execution)
        true,       // aikar G1GC flags
        false,      // zgc
        false,      // shenandoah
        None,       // jvm_flags
        paths,
    ).await;

    match res {
        Ok(_) => {
            let status_note = if start_now {
                "[RUNNING] Server has started in the background daemon.".green().to_string()
            } else {
                "[STOPPED] Server created. Start anytime with option [3].".dimmed().to_string()
            };

            show_modal_message(
                "SERVER SETUP COMPLETE",
                &[
                    format!("[OK] Server '{}' was registered successfully!", server_name).green().bold().to_string(),
                    format!("Software: {} (Version: {})", selected_sw_name, version),
                    format!("Memory:   {}", memory),
                    format!("Status:   {}", status_note),
                ],
                false,
            )?;
        }
        Err(e) => {
            show_modal_message(
                "SERVER CREATION FAILED",
                &[format!("[ERROR] Failed to set up server '{}': {}", server_name, e)],
                true,
            )?;
        }
    }

    Ok(())
}

async fn start_server_daemon(server_name: &str, paths: &CraftPaths) -> Result<()> {
    let registry = ServersRegistry::load(paths)?;
    let server = registry.find_by_name(server_name)
        .ok_or_else(|| CraftError::ServerNotFound(format!("Server '{}' not found", server_name)))?;
    DaemonClient::ensure_daemon_started(paths).await?;
    let mut client = DaemonClient::connect(paths).await?;
    client.start_server(&server.path).await?;
    Ok(())
}

async fn stop_server_daemon(server_name: &str, force: bool, paths: &CraftPaths) -> Result<()> {
    let registry = ServersRegistry::load(paths)?;
    let server = registry.find_by_name(server_name)
        .ok_or_else(|| CraftError::ServerNotFound(format!("Server '{}' not found", server_name)))?;
    if !DaemonClient::is_daemon_running(paths) {
        return Err(CraftError::Other("Daemon is not running; no background servers active.".to_string()));
    }
    let mut client = DaemonClient::connect(paths).await?;
    client.stop_server(&server.path, force).await?;
    Ok(())
}

async fn restart_server_daemon(server_name: &str, force: bool, paths: &CraftPaths) -> Result<()> {
    let registry = ServersRegistry::load(paths)?;
    let server = registry.find_by_name(server_name)
        .ok_or_else(|| CraftError::ServerNotFound(format!("Server '{}' not found", server_name)))?;
    if !DaemonClient::is_daemon_running(paths) {
        return Err(CraftError::Other("Daemon is not running; no background servers active.".to_string()));
    }
    let mut client = DaemonClient::connect(paths).await?;
    let _ = client.stop_server(&server.path, force).await;
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    client.start_server(&server.path).await?;
    Ok(())
}

pub async fn quick_start_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let registry = ServersRegistry::load(paths)?;
    if registry.servers.is_empty() {
        show_empty_servers_modal(paths).await?;
        return Ok(());
    }

    let running_paths = if DaemonClient::is_daemon_running(paths) {
        if let Ok(mut c) = DaemonClient::connect(paths).await {
            c.get_running().await.unwrap_or_default()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let mut entries = Vec::new();
    for (i, s) in registry.servers.iter().enumerate() {
        let is_running = running_paths.contains(&s.path)
            || s.path.canonicalize().map(|p| running_paths.contains(&p)).unwrap_or(false);
        let status_badge = if is_running { "[ALREADY RUNNING]".green().to_string() } else { "[STOPPED]".dimmed().to_string() };
        let hotkey = if i < 9 { (i + 1).to_string() } else { ((b'a' + (i - 9) as u8) as char).to_string() };
        entries.push(MenuEntry::new(hotkey, format!("{:<20} {:<10} {:<10} {}", s.name, s.software, s.version, status_badge)));
    }
    entries.push(MenuEntry::new("0", "Back to Dashboard").with_aliases(&["b", "q"]));

    let mut sel = 0;
    let header = format!(
        "{}\r\n{}\r\n{}\r\n Select a server to start in the background:\r\n{}",
        "================================================================================".cyan().bold(),
        "                                QUICK START SERVER                              ".cyan().bold(),
        "================================================================================".cyan().bold(),
        "--------------------------------------------------------------------------------".dimmed()
    );

    if let Some(idx) = run_menu(&header, &entries, &mut sel)? {
        if idx < registry.servers.len() {
            let server = &registry.servers[idx];
            let is_running = running_paths.contains(&server.path)
                || server.path.canonicalize().map(|p| running_paths.contains(&p)).unwrap_or(false);
            if is_running {
                show_modal_message(
                    "SERVER ALREADY RUNNING",
                    &[
                        format!("Server '{}' is already running!", server.name),
                        "Use 'Attach Live Console' or 'Stop Server' from the dashboard.".to_string(),
                    ],
                    false,
                )?;
            } else {
                print_in_place_status("STARTING SERVER", &[format!("Starting server '{}' in background daemon...", server.name)])?;
                match start_server_daemon(&server.name, paths).await {
                    Ok(_) => {
                        show_modal_message(
                            "SERVER STARTED",
                            &[
                                format!("[OK] Server '{}' started successfully!", server.name).green().bold().to_string(),
                                format!("Platform: {} {}", server.software, server.version),
                                format!("Path:     {}", server.path.display()),
                            ],
                            false,
                        )?;
                    }
                    Err(e) => {
                        show_modal_message(
                            "START FAILED",
                            &[format!("[ERROR] Failed to start server '{}': {}", server.name, e)],
                            true,
                        )?;
                    }
                }
            }
        }
    }
    Ok(())
}

pub async fn stop_servers_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let registry = ServersRegistry::load(paths)?;
    if registry.servers.is_empty() {
        show_empty_servers_modal(paths).await?;
        return Ok(());
    }

    let running_paths = if DaemonClient::is_daemon_running(paths) {
        if let Ok(mut c) = DaemonClient::connect(paths).await {
            c.get_running().await.unwrap_or_default()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let running_servers: Vec<_> = registry.servers.iter()
        .filter(|s| running_paths.contains(&s.path) || s.path.canonicalize().map(|p| running_paths.contains(&p)).unwrap_or(false))
        .collect();

    if running_servers.is_empty() {
        show_modal_message(
            "NO RUNNING SERVERS",
            &[
                "No servers are currently running on this host.".yellow().to_string(),
                "Use 'Quick Start Server' to launch one.".to_string(),
            ],
            false,
        )?;
        return Ok(());
    }

    let mut entries = Vec::new();
    for (i, s) in running_servers.iter().enumerate() {
        let hotkey = if i < 9 { (i + 1).to_string() } else { ((b'a' + (i - 9) as u8) as char).to_string() };
        entries.push(MenuEntry::new(hotkey, format!("{:<20} {:<10} {:<10} {}", s.name, s.software, s.version, "[RUNNING]".green().bold())));
    }
    entries.push(MenuEntry::new("a", "Stop ALL Running Servers"));
    entries.push(MenuEntry::new("0", "Back to Dashboard").with_aliases(&["b", "q"]));

    let mut sel = 0;
    let header = format!(
        "{}\r\n{}\r\n{}\r\n Select a running server to stop gracefully:\r\n{}",
        "================================================================================".cyan().bold(),
        "                               STOP RUNNING SERVER                              ".cyan().bold(),
        "================================================================================".cyan().bold(),
        "--------------------------------------------------------------------------------".dimmed()
    );

    if let Some(idx) = run_menu(&header, &entries, &mut sel)? {
        if idx < running_servers.len() {
            let server = running_servers[idx];
            print_in_place_status("STOPPING SERVER", &[format!("Sending stop command to server '{}'...", server.name)])?;
            match stop_server_daemon(&server.name, false, paths).await {
                Ok(_) => {
                    show_modal_message(
                        "SERVER STOPPED",
                        &[format!("[OK] Server '{}' stopped successfully.", server.name).green().bold().to_string()],
                        false,
                    )?;
                }
                Err(e) => {
                    show_modal_message(
                        "STOP FAILED",
                        &[format!("[ERROR] Failed to stop server '{}': {}", server.name, e)],
                        true,
                    )?;
                }
            }
        } else if idx == running_servers.len() {
            // Stop ALL
            print_in_place_status("STOPPING ALL SERVERS", &["Stopping all active background servers...".to_string()])?;
            let mut stopped = 0;
            for s in &running_servers {
                let _ = stop_server_daemon(&s.name, false, paths).await;
                stopped += 1;
            }
            show_modal_message(
                "SERVERS STOPPED",
                &[format!("[OK] Stopped {} servers.", stopped).green().bold().to_string()],
                false,
            )?;
        }
    }
    Ok(())
}

pub async fn restart_servers_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let registry = ServersRegistry::load(paths)?;
    if registry.servers.is_empty() {
        show_empty_servers_modal(paths).await?;
        return Ok(());
    }

    let mut entries = Vec::new();
    for (i, s) in registry.servers.iter().enumerate() {
        let hotkey = if i < 9 { (i + 1).to_string() } else { ((b'a' + (i - 9) as u8) as char).to_string() };
        entries.push(MenuEntry::new(hotkey, format!("{:<20} {:<10} {:<10}", s.name, s.software, s.version)));
    }
    entries.push(MenuEntry::new("0", "Back to Dashboard").with_aliases(&["b", "q"]));

    let mut sel = 0;
    let header = format!(
        "{}\r\n{}\r\n{}\r\n Select a server to restart:\r\n{}",
        "================================================================================".cyan().bold(),
        "                                 RESTART SERVER                                 ".cyan().bold(),
        "================================================================================".cyan().bold(),
        "--------------------------------------------------------------------------------".dimmed()
    );

    if let Some(idx) = run_menu(&header, &entries, &mut sel)? {
        if idx < registry.servers.len() {
            let server = &registry.servers[idx];
            print_in_place_status("RESTARTING SERVER", &[format!("Restarting server '{}'...", server.name)])?;
            match restart_server_daemon(&server.name, false, paths).await {
                Ok(_) => {
                    show_modal_message(
                        "SERVER RESTARTED",
                        &[format!("[OK] Server '{}' restarted successfully.", server.name).green().bold().to_string()],
                        false,
                    )?;
                }
                Err(e) => {
                    show_modal_message(
                        "RESTART FAILED",
                        &[format!("[ERROR] Failed to restart server '{}': {}", server.name, e)],
                        true,
                    )?;
                }
            }
        }
    }
    Ok(())
}

pub async fn view_servers_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let registry = ServersRegistry::load(paths)?;
    if registry.servers.is_empty() {
        show_empty_servers_modal(paths).await?;
        return Ok(());
    }

    let mut entries = Vec::new();
    for (i, s) in registry.servers.iter().enumerate() {
        let hotkey = if i < 9 { (i + 1).to_string() } else { ((b'a' + (i - 9) as u8) as char).to_string() };
        entries.push(MenuEntry::new(hotkey, format!("{:<20} {:<10} {:<10}", s.name, s.software, s.version)));
    }
    entries.push(MenuEntry::new("0", "Back to Dashboard").with_aliases(&["b", "q"]));

    let mut sel = 0;
    let header = format!(
        "{}\r\n{}\r\n{}\r\n Select a server to attach live terminal console:\r\n{}",
        "================================================================================".cyan().bold(),
        "                           ATTACH LIVE CONSOLE (VIEW)                           ".cyan().bold(),
        "================================================================================".cyan().bold(),
        "--------------------------------------------------------------------------------".dimmed()
    );

    if let Some(idx) = run_menu(&header, &entries, &mut sel)? {
        if idx < registry.servers.len() {
            let server = &registry.servers[idx];
            let _ = exec_console_action(|| async {
                handle_view(&server.name, None, paths).await
            }).await;
        }
    }
    Ok(())
}

pub async fn rm_servers_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let registry = ServersRegistry::load(paths)?;
    if registry.servers.is_empty() {
        show_empty_servers_modal(paths).await?;
        return Ok(());
    }

    let mut entries = Vec::new();
    for (i, s) in registry.servers.iter().enumerate() {
        let hotkey = if i < 9 { (i + 1).to_string() } else { ((b'a' + (i - 9) as u8) as char).to_string() };
        entries.push(MenuEntry::new(hotkey, format!("{:<20} {:<10} {:<10}", s.name, s.software, s.version)));
    }
    entries.push(MenuEntry::new("0", "Back to Dashboard").with_aliases(&["b", "q"]));

    let mut sel = 0;
    let header = format!(
        "{}\r\n{}\r\n{}\r\n Select a server to unregister or delete:\r\n{}",
        "================================================================================".cyan().bold(),
        "                                 DELETE SERVER                                  ".cyan().bold(),
        "================================================================================".cyan().bold(),
        "--------------------------------------------------------------------------------".dimmed()
    );

    if let Some(idx) = run_menu(&header, &entries, &mut sel)? {
        if idx < registry.servers.len() {
            let server = &registry.servers[idx];

            let confirm_header = format!(
                "{}\r\n{}\r\n{}\r\n How do you want to remove server '{}'?\r\n{}",
                "================================================================================".cyan().bold(),
                format!("REMOVE SERVER: {}", server.name).cyan().bold(),
                "================================================================================".cyan().bold(),
                server.name,
                "--------------------------------------------------------------------------------".dimmed()
            );

            let confirm_entries = vec![
                MenuEntry::new("1", "Unregister from Craft (Preserve world & server files on disk)"),
                MenuEntry::new("2", "Permanently Delete Server Directory & World Files (-rf)"),
                MenuEntry::new("0", "Cancel").with_aliases(&["b", "q"]),
            ];

            let mut c_sel = 0;
            match run_menu(&confirm_header, &confirm_entries, &mut c_sel)? {
                Some(0) => {
                    let mut reg = ServersRegistry::load(paths)?;
                    reg.remove(&server.path);
                    reg.save(paths)?;
                    show_modal_message(
                        "SERVER UNREGISTERED",
                        &[format!("[OK] Server '{}' unregistered from Craft registry. Files preserved.", server.name).green().bold().to_string()],
                        false,
                    )?;
                }
                Some(1) => {
                    let mut reg = ServersRegistry::load(paths)?;
                    reg.remove(&server.path);
                    reg.save(paths)?;
                    if server.path.exists() {
                        let _ = std::fs::remove_dir_all(&server.path);
                    }
                    show_modal_message(
                        "SERVER DELETED",
                        &[format!("[OK] Server '{}' and its directory permanently removed.", server.name).green().bold().to_string()],
                        false,
                    )?;
                }
                _ => {}
            }
        }
    }
    Ok(())
}

pub async fn manage_servers_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let mut selected = 0;

    loop {
        let registry = ServersRegistry::load(paths)?;
        let daemon_running = DaemonClient::is_daemon_running(paths);
        let running_paths = if daemon_running {
            if let Ok(mut c) = DaemonClient::connect(paths).await {
                c.get_running().await.unwrap_or_default()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

        if registry.servers.is_empty() {
            let header = format!(
                "{}\r\n{}\r\n{}\r\n No servers currently registered on this host.\r\n{}",
                "================================================================================".cyan().bold(),
                "                               REGISTERED SERVERS                               ".cyan().bold(),
                "================================================================================".cyan().bold(),
                "--------------------------------------------------------------------------------".dimmed()
            );

            let entries = vec![
                MenuEntry::new("1", "Create Your First Server (Setup Wizard)").with_aliases(&["2", "c", "n"]),
                MenuEntry::new("0", "Back to Main Menu").with_aliases(&["b", "q"]),
            ];

            match run_menu(&header, &entries, &mut selected)? {
                Some(0) => {
                    gui_create_server_wizard(paths).await?;
                }
                _ => return Ok(()),
            }
            continue;
        }

        let header = format!(
            "{}\r\n{}\r\n{}\r\n Select a server to inspect details, control lifecycle, or attach console.\r\n{}",
            "================================================================================".cyan().bold(),
            "                               REGISTERED SERVERS                               ".cyan().bold(),
            "================================================================================".cyan().bold(),
            "--------------------------------------------------------------------------------".dimmed()
        );

        let mut entries = Vec::new();
        for (idx, s) in registry.servers.iter().enumerate() {
            let is_running = running_paths.contains(&s.path)
                || s.path.canonicalize().map(|p| running_paths.contains(&p)).unwrap_or(false);
            let status_str = if is_running {
                "[RUNNING]".green().bold().to_string()
            } else {
                "[STOPPED]".dimmed().to_string()
            };
            let hotkey = if idx < 9 {
                (idx + 1).to_string()
            } else {
                ((b'a' + (idx - 9) as u8) as char).to_string()
            };
            entries.push(MenuEntry::new(
                hotkey,
                format!("{:<20} {:<10} {:<10} {}", s.name, s.software, s.version, status_str),
            ));
        }

        entries.push(MenuEntry::new("n", "Create New Server (Setup Wizard)").with_aliases(&["c"]));
        entries.push(MenuEntry::new("0", "Back to Main Menu").with_aliases(&["b", "q"]));

        let sel = run_menu(&header, &entries, &mut selected)?;

        match sel {
            Some(idx) if idx < registry.servers.len() => {
                let chosen = &registry.servers[idx];
                server_control_panel(&chosen.name, paths).await?;
            }
            Some(idx) if idx == registry.servers.len() => {
                gui_create_server_wizard(paths).await?;
            }
            _ => return Ok(()),
        }
    }
}

async fn server_control_panel(server_name: &str, paths: &CraftPaths) -> Result<()> {
    let mut selected = 0;

    loop {
        let registry = ServersRegistry::load(paths)?;
        let server = match registry.servers.iter().find(|s| s.name == server_name) {
            Some(s) => s.clone(),
            None => {
                show_modal_message("SERVER NOT FOUND", &[format!("Server '{}' is no longer registered.", server_name)], true)?;
                return Ok(());
            }
        };

        let daemon_running = DaemonClient::is_daemon_running(paths);
        let is_running = if daemon_running {
            if let Ok(mut c) = DaemonClient::connect(paths).await {
                let running = c.get_running().await.unwrap_or_default();
                running.contains(&server.path)
                    || server.path.canonicalize().map(|p| running.contains(&p)).unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        };

        let status_badge = if is_running {
            "[RUNNING]".green().bold()
        } else {
            "[STOPPED]".dimmed()
        };

        let title = format!("                           SERVER: {:<20} {}", server.name, status_badge);
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Platform: {:<12} | Version: {:<10} | Memory: {}\r\n Path: {}\r\n{}",
            "================================================================================".cyan().bold(),
            title,
            "================================================================================".cyan().bold(),
            server.software.white().bold(),
            server.version.cyan(),
            server.memory.as_deref().unwrap_or("Default (2G)"),
            server.path.display(),
            "--------------------------------------------------------------------------------".dimmed()
        );

        let entries = vec![
            MenuEntry::new("1", "Start Server (Background Daemon)"),
            MenuEntry::new("2", "Start Server (Foreground Terminal)"),
            MenuEntry::new("3", "Stop Server"),
            MenuEntry::new("4", "Restart Server"),
            MenuEntry::new("5", "Attach Live Console (craft view)"),
            MenuEntry::new("6", "Create World Snapshot Backup"),
            MenuEntry::new("7", "List Existing Backups"),
            MenuEntry::new("8", "Delete / Unregister Server"),
            MenuEntry::new("0", "Back to Server List").with_aliases(&["b", "q"]),
        ];

        let sel = run_menu(&header, &entries, &mut selected)?;

        match sel {
            Some(0) => {
                // Start background
                if is_running {
                    show_modal_message("ALREADY RUNNING", &[format!("Server '{}' is already running.", server.name)], false)?;
                } else {
                    print_in_place_status("STARTING SERVER", &[format!("Starting server '{}' in background daemon...", server.name)])?;
                    match start_server_daemon(&server.name, paths).await {
                        Ok(_) => show_modal_message("SERVER STARTED", &[format!("[OK] Server '{}' started in daemon.", server.name).green().bold().to_string()], false)?,
                        Err(e) => show_modal_message("START FAILED", &[format!("[ERROR] {}", e)], true)?,
                    }
                }
            }
            Some(1) => {
                // Start foreground
                let _ = exec_console_action(|| async {
                    run_foreground_server(&server.path).await
                }).await;
            }
            Some(2) => {
                // Stop
                if !is_running {
                    show_modal_message("SERVER NOT RUNNING", &[format!("Server '{}' is not currently running.", server.name)], false)?;
                } else {
                    print_in_place_status("STOPPING SERVER", &[format!("Stopping server '{}'...", server.name)])?;
                    match stop_server_daemon(&server.name, false, paths).await {
                        Ok(_) => show_modal_message("SERVER STOPPED", &[format!("[OK] Server '{}' stopped.", server.name).green().bold().to_string()], false)?,
                        Err(e) => show_modal_message("STOP FAILED", &[format!("[ERROR] {}", e)], true)?,
                    }
                }
            }
            Some(3) => {
                // Restart
                print_in_place_status("RESTARTING SERVER", &[format!("Restarting server '{}'...", server.name)])?;
                match restart_server_daemon(&server.name, false, paths).await {
                    Ok(_) => show_modal_message("SERVER RESTARTED", &[format!("[OK] Server '{}' restarted.", server.name).green().bold().to_string()], false)?,
                    Err(e) => show_modal_message("RESTART FAILED", &[format!("[ERROR] {}", e)], true)?,
                }
            }
            Some(4) => {
                // View live console
                let _ = exec_console_action(|| async {
                    handle_view(&server.name, None, paths).await
                }).await;
            }
            Some(5) => {
                // Create backup
                print_in_place_status("CREATING BACKUP", &[format!("Creating snapshot for server '{}'...", server.name)])?;
                let engine = BackupEngine::new(paths);
                match engine.create_backup(&server.name, &server.path, None, false).await {
                    Ok(file) => show_modal_message("BACKUP CREATED", &[format!("[OK] Archive: {}", file.display()).green().bold().to_string()], false)?,
                    Err(e) => show_modal_message("BACKUP FAILED", &[format!("[ERROR] {}", e)], true)?,
                }
            }
            Some(6) => {
                // List backups
                let engine = BackupEngine::new(paths);
                let list = engine.list_backups(&server.name);
                if list.is_empty() {
                    show_modal_message("NO BACKUPS", &[format!("No existing backups found for server '{}'.", server.name)], false)?;
                } else {
                    let lines: Vec<String> = list.iter().map(|b| {
                        let mb = (b.size_bytes as f64) / (1024.0 * 1024.0);
                        format!("{:<40} {:>8.2} MB  {}", b.filename, mb, b.created_at)
                    }).collect();
                    show_modal_message("EXISTING BACKUPS", &lines, false)?;
                }
            }
            Some(7) => {
                // Delete server
                let confirm_header = format!(
                    "{}\r\n{}\r\n{}\r\n Are you sure you want to remove server '{}'?\r\n{}",
                    "================================================================================".cyan().bold(),
                    format!("DELETE SERVER: {}", server.name).cyan().bold(),
                    "================================================================================".cyan().bold(),
                    server.name,
                    "--------------------------------------------------------------------------------".dimmed()
                );
                let confirm_entries = vec![
                    MenuEntry::new("1", "Unregister from Craft (Preserve world & server files)"),
                    MenuEntry::new("2", "Permanently Delete Server Directory & Files (-rf)"),
                    MenuEntry::new("0", "Cancel").with_aliases(&["b", "q"]),
                ];
                let mut c_sel = 0;
                match run_menu(&confirm_header, &confirm_entries, &mut c_sel)? {
                    Some(0) => {
                        let mut reg = ServersRegistry::load(paths)?;
                        reg.remove(&server.path);
                        reg.save(paths)?;
                        show_modal_message("SERVER UNREGISTERED", &[format!("[OK] Server '{}' unregistered. Files kept on disk.", server.name)], false)?;
                        return Ok(());
                    }
                    Some(1) => {
                        let mut reg = ServersRegistry::load(paths)?;
                        reg.remove(&server.path);
                        reg.save(paths)?;
                        if server.path.exists() {
                            let _ = std::fs::remove_dir_all(&server.path);
                        }
                        show_modal_message("SERVER DELETED", &[format!("[OK] Server '{}' and directory removed.", server.name)], false)?;
                        return Ok(());
                    }
                    _ => {}
                }
            }
            _ => return Ok(()),
        }
    }
}

pub async fn ping_menu() -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let target = match run_input_prompt(
        "SERVER NETWORK PING",
        "Enter server address (IP:Port or Domain):",
        Some("127.0.0.1:25565"),
    )? {
        Some(t) if !t.trim().is_empty() => t.trim().to_string(),
        _ => return Ok(()),
    };

    let mut proto_sel = 0;
    let proto_header = format!(
        "{}\r\n{}\r\n{}\r\n Target: {}\r\n Select protocol query mode:\r\n{}",
        "================================================================================".cyan().bold(),
        "                             SELECT PING PROTOCOL                               ".cyan().bold(),
        "================================================================================".cyan().bold(),
        target,
        "--------------------------------------------------------------------------------".dimmed()
    );

    let proto_entries = vec![
        MenuEntry::new("1", "Java Edition (Server List Ping SLP)"),
        MenuEntry::new("2", "Bedrock Edition (RakNet Unconnected Ping)"),
        MenuEntry::new("0", "Cancel").with_aliases(&["b", "q"]),
    ];

    let choice = run_menu(&proto_header, &proto_entries, &mut proto_sel)?;
    let is_bedrock = match choice {
        Some(0) => false,
        Some(1) => true,
        _ => return Ok(()),
    };

    let (host, port) = if let Some(idx) = target.find(':') {
        let (h, p) = target.split_at(idx);
        let port_num: u16 = p[1..].parse().unwrap_or(if is_bedrock { 19132 } else { 25565 });
        (h.to_string(), port_num)
    } else {
        (target.clone(), if is_bedrock { 19132 } else { 25565 })
    };

    print_in_place_status("PINGING SERVER", &[format!("Connecting to {}:{}...", host, port)])?;

    if is_bedrock {
        match ping_bedrock_server(&host, port).await {
            Ok(res) => {
                show_modal_message(
                    "BEDROCK SERVER ONLINE",
                    &[
                        format!("Server Name: {}", res.server_name),
                        format!("Version:     {} (Protocol {})", res.version, res.protocol_version),
                        format!("Players:     {}/{}", res.online_players, res.max_players),
                        format!("World:       {}", res.world_name),
                        format!("Latency:     {} ms", res.latency_ms),
                    ],
                    false,
                )?;
            }
            Err(e) => {
                show_modal_message(
                    "BEDROCK PING FAILED",
                    &[format!("[ERROR] Could not reach {}:{}: {}", host, port, e)],
                    true,
                )?;
            }
        }
    } else {
        match ping_java_server(&host, port).await {
            Ok(res) => {
                show_modal_message(
                    "JAVA SERVER ONLINE",
                    &[
                        format!("MOTD:     {}", res.motd),
                        format!("Version:  {} (Protocol {})", res.version_name, res.protocol_version),
                        format!("Players:  {}/{}", res.online_players, res.max_players),
                        format!("Latency:  {} ms", res.latency_ms),
                    ],
                    false,
                )?;
            }
            Err(e) => {
                show_modal_message(
                    "JAVA PING FAILED",
                    &[format!("[ERROR] Could not reach {}:{}: {}", host, port, e)],
                    true,
                )?;
            }
        }
    }

    Ok(())
}

pub async fn backups_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let mut selected = 0;

    loop {
        let registry = ServersRegistry::load(paths)?;
        if registry.servers.is_empty() {
            show_empty_servers_modal(paths).await?;
            return Ok(());
        }

        let header = format!(
            "{}\r\n{}\r\n{}\r\n Create compressed backups, inspect archive history, or restore worlds.\r\n{}",
            "================================================================================".cyan().bold(),
            "                       WORLD SNAPSHOTS & BACKUP MANAGER                         ".cyan().bold(),
            "================================================================================".cyan().bold(),
            "--------------------------------------------------------------------------------".dimmed()
        );

        let entries = vec![
            MenuEntry::new("1", "Create World Snapshot"),
            MenuEntry::new("2", "List Existing Backups"),
            MenuEntry::new("3", "Restore Server from Backup"),
            MenuEntry::new("0", "Back to Main Menu").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                // Create
                let mut s_entries = Vec::new();
                for (i, s) in registry.servers.iter().enumerate() {
                    let hotkey = if i < 9 { (i + 1).to_string() } else { ((b'a' + (i - 9) as u8) as char).to_string() };
                    s_entries.push(MenuEntry::new(hotkey, s.name.clone()));
                }
                s_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b", "q"]));
                let s_header = " Select server to snapshot:";
                let mut s_sel = 0;
                if let Some(idx) = run_menu(s_header, &s_entries, &mut s_sel)? {
                    if idx < registry.servers.len() {
                        let server = &registry.servers[idx];
                        let mode_header = " Choose backup scope:";
                        let mode_entries = vec![
                            MenuEntry::new("1", "Full Server Snapshot (All configs, plugins, and worlds)"),
                            MenuEntry::new("2", "World Only Snapshot (Fastest, skips binaries/logs)"),
                            MenuEntry::new("0", "Cancel").with_aliases(&["b", "q"]),
                        ];
                        let mut m_sel = 0;
                        if let Some(m_idx) = run_menu(mode_header, &mode_entries, &mut m_sel)? {
                            let world_only = match m_idx {
                                0 => false,
                                1 => true,
                                _ => continue,
                            };
                            print_in_place_status("CREATING BACKUP", &[format!("Compressing snapshot for '{}'...", server.name)])?;
                            let engine = BackupEngine::new(paths);
                            match engine.create_backup(&server.name, &server.path, None, world_only).await {
                                Ok(file) => {
                                    let meta = std::fs::metadata(&file)?;
                                    let mb = (meta.len() as f64) / (1024.0 * 1024.0);
                                    show_modal_message(
                                        "SNAPSHOT CREATED",
                                        &[
                                            format!("[OK] Successfully saved snapshot for server '{}'!", server.name).green().bold().to_string(),
                                            format!("File: {}", file.display()),
                                            format!("Size: {:.2} MB", mb),
                                        ],
                                        false,
                                    )?;
                                }
                                Err(e) => {
                                    show_modal_message("BACKUP FAILED", &[format!("[ERROR] {}", e)], true)?;
                                }
                            }
                        }
                    }
                }
            }
            Some(1) => {
                // List
                let mut s_entries = Vec::new();
                for (i, s) in registry.servers.iter().enumerate() {
                    let hotkey = if i < 9 { (i + 1).to_string() } else { ((b'a' + (i - 9) as u8) as char).to_string() };
                    s_entries.push(MenuEntry::new(hotkey, s.name.clone()));
                }
                s_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b", "q"]));
                let s_header = " Select server to view backups:";
                let mut s_sel = 0;
                if let Some(idx) = run_menu(s_header, &s_entries, &mut s_sel)? {
                    if idx < registry.servers.len() {
                        let server = &registry.servers[idx];
                        let engine = BackupEngine::new(paths);
                        let list = engine.list_backups(&server.name);
                        if list.is_empty() {
                            show_modal_message("NO BACKUPS FOUND", &[format!("No backups exist for server '{}'.", server.name)], false)?;
                        } else {
                            let lines: Vec<String> = list.iter().map(|b| {
                                let mb = (b.size_bytes as f64) / (1024.0 * 1024.0);
                                format!("{:<40} {:>8.2} MB  {}", b.filename, mb, b.created_at)
                            }).collect();
                            show_modal_message(&format!("BACKUPS FOR {}", server.name), &lines, false)?;
                        }
                    }
                }
            }
            Some(2) => {
                // Restore
                let mut s_entries = Vec::new();
                for (i, s) in registry.servers.iter().enumerate() {
                    let hotkey = if i < 9 { (i + 1).to_string() } else { ((b'a' + (i - 9) as u8) as char).to_string() };
                    s_entries.push(MenuEntry::new(hotkey, s.name.clone()));
                }
                s_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b", "q"]));
                let s_header = " Select server to restore:";
                let mut s_sel = 0;
                if let Some(idx) = run_menu(s_header, &s_entries, &mut s_sel)? {
                    if idx < registry.servers.len() {
                        let server = &registry.servers[idx];
                        let engine = BackupEngine::new(paths);
                        let list = engine.list_backups(&server.name);

                        let mut b_entries = Vec::new();
                        for (bi, b) in list.iter().enumerate() {
                            let hotkey = if bi < 9 { (bi + 1).to_string() } else { ((b'a' + (bi - 9) as u8) as char).to_string() };
                            let mb = (b.size_bytes as f64) / (1024.0 * 1024.0);
                            b_entries.push(MenuEntry::new(hotkey, format!("{:<32} ({:.1} MB)", b.filename, mb)));
                        }
                        b_entries.push(MenuEntry::new("c", "Custom Archive Path"));
                        b_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b", "q"]));

                        let b_header = format!(" Select backup archive to restore to '{}':", server.name);
                        let mut b_sel = 0;
                        if let Some(b_idx) = run_menu(&b_header, &b_entries, &mut b_sel)? {
                            let target_archive = if b_idx < list.len() {
                                paths.backups_dir.join(&server.name).join(&list[b_idx].filename)
                            } else if b_idx == list.len() {
                                match run_input_prompt("CUSTOM ARCHIVE", "Enter path to archive (.tar.gz / .zip):", None)? {
                                    Some(p) if !p.trim().is_empty() => PathBuf::from(p.trim()),
                                    _ => continue,
                                }
                            } else {
                                continue;
                            };

                            print_in_place_status("RESTORING SERVER", &[format!("Unpacking backup '{}' into '{}'...", target_archive.display(), server.path.display())])?;
                            match engine.restore_backup(&target_archive, &server.path) {
                                Ok(_) => {
                                    show_modal_message(
                                        "RESTORE COMPLETE",
                                        &[format!("[OK] Successfully restored server '{}' from backup!", server.name).green().bold().to_string()],
                                        false,
                                    )?;
                                }
                                Err(e) => {
                                    show_modal_message("RESTORE FAILED", &[format!("[ERROR] {}", e)], true)?;
                                }
                            }
                        }
                    }
                }
            }
            _ => return Ok(()),
        }
    }
}

pub async fn plugins_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let mut selected = 0;

    loop {
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Search and install Minecraft plugins and extensions with one click.\r\n{}",
            "================================================================================".cyan().bold(),
            "                         PLUGINS & EXTENSIONS MANAGER                           ".cyan().bold(),
            "================================================================================".cyan().bold(),
            "--------------------------------------------------------------------------------".dimmed()
        );

        let entries = vec![
            MenuEntry::new("1", "Search Plugins Online (Modrinth, Hangar, Poggit)"),
            MenuEntry::new("2", "Install Plugin by ID / Slug to Server"),
            MenuEntry::new("0", "Back to Main Menu").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                let query = match run_input_prompt(
                    "SEARCH PLUGINS ONLINE",
                    "Search keyword (e.g. essentials, viaversion, luckperms, worldedit):",
                    None,
                )? {
                    Some(q) if !q.trim().is_empty() => q.trim().to_string(),
                    _ => continue,
                };

                print_in_place_status("SEARCHING PLUGINS", &[format!("Querying plugin repositories for '{}'...", query)])?;
                let pm = PluginManager::new();
                let results = pm.search(&query).await;

                if results.is_empty() {
                    show_modal_message(
                        "NO PLUGINS FOUND",
                        &[format!("No plugins found matching query '{}'.", query)],
                        false,
                    )?;
                } else {
                    let mut p_entries = Vec::new();
                    for (i, hit) in results.iter().take(9).enumerate() {
                        let hotkey = (i + 1).to_string();
                        let desc = if hit.description.len() > 40 { format!("{}...", &hit.description[..37]) } else { hit.description.clone() };
                        p_entries.push(MenuEntry::new(hotkey, format!("{:<18} [{}] - {}", hit.name, hit.source, desc)));
                    }
                    p_entries.push(MenuEntry::new("0", "Back").with_aliases(&["b", "q"]));

                    let p_header = format!(" Search results for '{}' - select to install:", query);
                    let mut p_sel = 0;
                    if let Some(p_idx) = run_menu(&p_header, &p_entries, &mut p_sel)? {
                        if p_idx < results.len() {
                            let chosen_plugin = &results[p_idx];
                            let registry = ServersRegistry::load(paths)?;
                            if registry.servers.is_empty() {
                                show_empty_servers_modal(paths).await?;
                                continue;
                            }

                            let mut s_entries = Vec::new();
                            for (si, s) in registry.servers.iter().enumerate() {
                                let hotkey = if si < 9 { (si + 1).to_string() } else { ((b'a' + (si - 9) as u8) as char).to_string() };
                                s_entries.push(MenuEntry::new(hotkey, s.name.clone()));
                            }
                            s_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b", "q"]));

                            let s_header = format!(" Install '{}' to which server?", chosen_plugin.name);
                            let mut s_sel = 0;
                            if let Some(s_idx) = run_menu(&s_header, &s_entries, &mut s_sel)? {
                                if s_idx < registry.servers.len() {
                                    let server = &registry.servers[s_idx];
                                    print_in_place_status("INSTALLING PLUGIN", &[format!("Downloading '{}' into '{}'...", chosen_plugin.name, server.path.display())])?;
                                    match pm.install_from_modrinth(&server.path, &chosen_plugin.id_or_slug).await {
                                        Ok(dest) => {
                                            show_modal_message(
                                                "PLUGIN INSTALLED",
                                                &[
                                                    format!("[OK] Installed '{}' successfully!", chosen_plugin.name).green().bold().to_string(),
                                                    format!("Server: {}", server.name),
                                                    format!("File:   {}", dest.display()),
                                                ],
                                                false,
                                            )?;
                                        }
                                        Err(e) => {
                                            show_modal_message("INSTALLATION FAILED", &[format!("[ERROR] {}", e)], true)?;
                                        }
                                    }
                                }
                            }
                        }
                    }
                }
            }
            Some(1) => {
                let registry = ServersRegistry::load(paths)?;
                if registry.servers.is_empty() {
                    show_empty_servers_modal(paths).await?;
                    continue;
                }

                let mut s_entries = Vec::new();
                for (si, s) in registry.servers.iter().enumerate() {
                    let hotkey = if si < 9 { (si + 1).to_string() } else { ((b'a' + (si - 9) as u8) as char).to_string() };
                    s_entries.push(MenuEntry::new(hotkey, s.name.clone()));
                }
                s_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b", "q"]));

                let s_header = " Select server to install plugin:";
                let mut s_sel = 0;
                if let Some(s_idx) = run_menu(s_header, &s_entries, &mut s_sel)? {
                    if s_idx < registry.servers.len() {
                        let server = &registry.servers[s_idx];
                        let project_id = match run_input_prompt("PLUGIN ID", "Enter Modrinth plugin slug or ID (e.g. luckperms, spark):", None)? {
                            Some(p) if !p.trim().is_empty() => p.trim().to_string(),
                            _ => continue,
                        };

                        print_in_place_status("INSTALLING PLUGIN", &[format!("Installing '{}' to '{}'...", project_id, server.name)])?;
                        let pm = PluginManager::new();
                        match pm.install_from_modrinth(&server.path, &project_id).await {
                            Ok(dest) => {
                                show_modal_message(
                                    "PLUGIN INSTALLED",
                                    &[
                                        format!("[OK] Installed plugin '{}' successfully!", project_id).green().bold().to_string(),
                                        format!("Server: {}", server.name),
                                        format!("File:   {}", dest.display()),
                                    ],
                                    false,
                                )?;
                            }
                            Err(e) => {
                                show_modal_message("INSTALLATION FAILED", &[format!("[ERROR] {}", e)], true)?;
                            }
                        }
                    }
                }
            }
            _ => return Ok(()),
        }
    }
}

pub async fn remotes_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let mut selected = 0;

    loop {
        let header = format!(
            "{}\r\n{}\r\n{}\r\n Manage remote game server hosts and orchestrated deployments over SSH.\r\n{}",
            "================================================================================".cyan().bold(),
            "                            REMOTE VPS HOSTS (SSH)                              ".cyan().bold(),
            "================================================================================".cyan().bold(),
            "--------------------------------------------------------------------------------".dimmed()
        );

        let entries = vec![
            MenuEntry::new("1", "List Configured Remote Hosts"),
            MenuEntry::new("2", "Test Remote Host Connection"),
            MenuEntry::new("3", "Add New Remote Host"),
            MenuEntry::new("4", "Remove Remote Host"),
            MenuEntry::new("0", "Back to Main Menu").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                // List
                let reg = RemotesRegistry::load(paths)?;
                if reg.remotes.is_empty() {
                    show_modal_message("NO REMOTE HOSTS", &["No remote hosts configured.", "Add one with option [3]."], false)?;
                } else {
                    let lines: Vec<String> = reg.remotes.iter().map(|r| {
                        format!("{:<16} {}@{}:{}", r.alias, r.user, r.host, r.port)
                    }).collect();
                    show_modal_message("CONFIGURED REMOTE HOSTS", &lines, false)?;
                }
            }
            Some(1) => {
                // Test
                let reg = RemotesRegistry::load(paths)?;
                if reg.remotes.is_empty() {
                    show_modal_message("NO REMOTE HOSTS", &["No remote hosts configured to test."], false)?;
                    continue;
                }

                let mut r_entries = Vec::new();
                for (i, r) in reg.remotes.iter().enumerate() {
                    let hotkey = (i + 1).to_string();
                    r_entries.push(MenuEntry::new(hotkey, format!("{:<16} ({}@{})", r.alias, r.user, r.host)));
                }
                r_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b", "q"]));

                let mut r_sel = 0;
                if let Some(r_idx) = run_menu(" Select remote host to test connection:", &r_entries, &mut r_sel)? {
                    if r_idx < reg.remotes.len() {
                        let remote = &reg.remotes[r_idx];
                        print_in_place_status("TESTING SSH CONNECTION", &[format!("Connecting to {}@{}:{}...", remote.user, remote.host, remote.port)])?;

                        let session_res = craft_remote::RemoteSession::connect(remote);
                        match session_res {
                            Ok(session) => {
                                let uname = session.exec("uname -a").map(|(_, out, _)| out.trim().to_string()).unwrap_or_default();
                                show_modal_message(
                                    "SSH CONNECTION SUCCESSFUL",
                                    &[
                                        format!("[OK] Successfully connected to host '{}'!", remote.alias).green().bold().to_string(),
                                        format!("Remote host: {}@{}:{}", remote.user, remote.host, remote.port),
                                        format!("System info: {}", uname),
                                    ],
                                    false,
                                )?;
                            }
                            Err(e) => {
                                show_modal_message("SSH CONNECTION FAILED", &[format!("[ERROR] {}", e)], true)?;
                            }
                        }
                    }
                }
            }
            Some(2) => {
                // Add
                let alias = match run_input_prompt("ADD REMOTE HOST (1/2)", "Enter host alias (e.g. prod-vps, ovh-node):", None)? {
                    Some(a) if !a.trim().is_empty() => a.trim().to_string(),
                    _ => continue,
                };

                let conn = match run_input_prompt("ADD REMOTE HOST (2/2)", "Enter SSH connection string (user@host or user@host:port):", None)? {
                    Some(c) if !c.trim().is_empty() => c.trim().to_string(),
                    _ => continue,
                };

                match parse_connection_string(&conn) {
                    Ok((user, host, port)) => {
                        let mut reg = RemotesRegistry::load(paths)?;
                        let config = RemoteHostConfig {
                            alias: alias.clone(),
                            host,
                            port,
                            user,
                            auth_type: RemoteAuthType::Key,
                            key_path: None,
                            password: None,
                            remote_dir: None,
                            os_type: None,
                        };
                        reg.add(config)?;
                        reg.save(paths)?;
                        show_modal_message("REMOTE HOST ADDED", &[format!("[OK] Remote host '{}' configured successfully!", alias).green().bold().to_string()], false)?;
                    }
                    Err(e) => {
                        show_modal_message("INVALID CONNECTION STRING", &[format!("[ERROR] {}", e)], true)?;
                    }
                }
            }
            Some(3) => {
                // Remove
                let mut reg = RemotesRegistry::load(paths)?;
                if reg.remotes.is_empty() {
                    show_modal_message("NO REMOTE HOSTS", &["No remote hosts configured to remove."], false)?;
                    continue;
                }

                let mut r_entries = Vec::new();
                for (i, r) in reg.remotes.iter().enumerate() {
                    let hotkey = (i + 1).to_string();
                    r_entries.push(MenuEntry::new(hotkey, r.alias.clone()));
                }
                r_entries.push(MenuEntry::new("0", "Cancel").with_aliases(&["b", "q"]));

                let mut r_sel = 0;
                if let Some(r_idx) = run_menu(" Select remote host to delete:", &r_entries, &mut r_sel)? {
                    if r_idx < reg.remotes.len() {
                        let alias = reg.remotes[r_idx].alias.clone();
                        reg.remove(&alias);
                        reg.save(paths)?;
                        show_modal_message("REMOTE HOST REMOVED", &[format!("[OK] Removed remote host '{}'.", alias)], false)?;
                    }
                }
            }
            _ => return Ok(()),
        }
    }
}

pub async fn daemon_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let mut selected = 0;

    loop {
        let is_running = DaemonClient::is_daemon_running(paths);
        let status_badge = if is_running { "[ONLINE]".green().bold() } else { "[OFFLINE]".yellow().bold() };

        let header = format!(
            "{}\r\n{}\r\n{}\r\n System Supervisor Daemon Status: {}\r\n{}",
            "================================================================================".cyan().bold(),
            "                          CRAFT SERVICE DAEMON CONTROL                          ".cyan().bold(),
            "================================================================================".cyan().bold(),
            status_badge,
            "--------------------------------------------------------------------------------".dimmed()
        );

        let entries = vec![
            MenuEntry::new("1", "Check Daemon Status"),
            MenuEntry::new("2", "Start Daemon (Background Supervisor)"),
            MenuEntry::new("3", "Stop Daemon"),
            MenuEntry::new("4", "Restart Daemon"),
            MenuEntry::new("0", "Back to Main Menu").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                if is_running {
                    let pid = read_pid_file(&paths.pid_file).unwrap_or(0);
                    let mut lines = vec![
                        format!("[ONLINE] Craft service daemon is active and managing servers.").green().bold().to_string(),
                        format!("Process PID: {}", pid),
                    ];
                    if let Ok(mut client) = DaemonClient::connect(paths).await {
                        let running = client.get_running().await.unwrap_or_default();
                        lines.push(format!("Active background servers: {}", running.len()));
                        for p in running {
                            lines.push(format!("  - {}", p.display()));
                        }
                    }
                    show_modal_message("DAEMON STATUS", &lines, false)?;
                } else {
                    show_modal_message(
                        "DAEMON STATUS",
                        &[
                            "[OFFLINE] Craft service daemon is currently stopped.".yellow().to_string(),
                            "Start the daemon with option [2] to enable 24/7 background servers.".to_string(),
                        ],
                        false,
                    )?;
                }
            }
            Some(1) => {
                if is_running {
                    show_modal_message("ALREADY RUNNING", &["Craft service daemon is already running.".to_string()], false)?;
                } else {
                    print_in_place_status("STARTING DAEMON", &["Launching Craft supervisor daemon in background...".to_string()])?;
                    match DaemonClient::ensure_daemon_started(paths).await {
                        Ok(_) => show_modal_message("DAEMON STARTED", &["[OK] Craft service daemon is now online!".green().bold().to_string()], false)?,
                        Err(e) => show_modal_message("DAEMON START FAILED", &[format!("[ERROR] {}", e)], true)?,
                    }
                }
            }
            Some(2) => {
                if !is_running {
                    show_modal_message("ALREADY STOPPED", &["Craft service daemon is not running.".to_string()], false)?;
                } else {
                    print_in_place_status("STOPPING DAEMON", &["Stopping Craft service daemon...".to_string()])?;
                    if let Some(pid) = read_pid_file(&paths.pid_file) {
                        let _ = kill_process(pid, false);
                    }
                    show_modal_message("DAEMON STOPPED", &["[OK] Craft service daemon has been stopped.".green().bold().to_string()], false)?;
                }
            }
            Some(3) => {
                print_in_place_status("RESTARTING DAEMON", &["Restarting supervisor daemon...".to_string()])?;
                if is_running {
                    if let Some(pid) = read_pid_file(&paths.pid_file) {
                        let _ = kill_process(pid, false);
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                }
                match DaemonClient::ensure_daemon_started(paths).await {
                    Ok(_) => show_modal_message("DAEMON RESTARTED", &["[OK] Craft service daemon restarted successfully.".green().bold().to_string()], false)?,
                    Err(e) => show_modal_message("RESTART FAILED", &[format!("[ERROR] {}", e)], true)?,
                }
            }
            _ => return Ok(()),
        }
    }
}

pub fn cache_menu(paths: &CraftPaths) -> Result<()> {
    let _guard = AltScreenGuard::enter();
    let mut selected = 0;

    loop {
        let cache = CacheManager::new(paths);
        let size = cache.get_cache_size();
        let mb = (size as f64) / (1024.0 * 1024.0);

        let header = format!(
            "{}\r\n{}\r\n{}\r\n Total Download Cache: {:.2} MB | Location: {}\r\n{}",
            "================================================================================".cyan().bold(),
            "                         CACHE & STORAGE MANAGEMENT                             ".cyan().bold(),
            "================================================================================".cyan().bold(),
            mb,
            paths.cache_dir.display(),
            "--------------------------------------------------------------------------------".dimmed()
        );

        let entries = vec![
            MenuEntry::new("1", "Show Current Cache Size"),
            MenuEntry::new("2", "Purge All Download Caches"),
            MenuEntry::new("0", "Back to Main Menu").with_aliases(&["b", "q"]),
        ];

        match run_menu(&header, &entries, &mut selected)? {
            Some(0) => {
                show_modal_message(
                    "CACHE STORAGE",
                    &[
                        format!("Current download cache: {:.2} MB", mb),
                        format!("Cache directory:        {}", paths.cache_dir.display()),
                    ],
                    false,
                )?;
            }
            Some(1) => {
                let conf_header = format!(
                    "{}\r\n{}\r\n{}\r\n Delete all cached jarfiles and archives ({:.2} MB)?\r\n{}",
                    "================================================================================".cyan().bold(),
                    "                               CONFIRM CACHE PURGE                              ".cyan().bold(),
                    "================================================================================".cyan().bold(),
                    mb,
                    "--------------------------------------------------------------------------------".dimmed()
                );
                let conf_entries = vec![
                    MenuEntry::new("1", "Yes, Purge All Download Caches"),
                    MenuEntry::new("2", "Cancel"),
                ];
                let mut c_sel = 1;
                if let Some(0) = run_menu(&conf_header, &conf_entries, &mut c_sel)? {
                    match cache.clean_cache() {
                        Ok(cleaned) => {
                            let cl_mb = (cleaned as f64) / (1024.0 * 1024.0);
                            show_modal_message(
                                "CACHE PURGED",
                                &[format!("[OK] Cleared {:.2} MB of downloaded caches.", cl_mb).green().bold().to_string()],
                                false,
                            )?;
                        }
                        Err(e) => {
                            show_modal_message("PURGE FAILED", &[format!("[ERROR] {}", e)], true)?;
                        }
                    }
                }
            }
            _ => return Ok(()),
        }
    }
}
