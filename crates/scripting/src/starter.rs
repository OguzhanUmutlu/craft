use crate::config::{CustomRuntimeType, CustomServerConfig};
use craft_core::Result;
use std::fs;
use std::path::Path;

pub fn generate_server_lua(name: &str, port: u16) -> String {
    format!(
        r#"-- Custom Game Server in Lua (Craft Engine)
-- Server: {name} | Port: {port}

craft.log("Initializing server '{name}' on port {port}...")
craft.log("Server root: " .. (craft.server and craft.server.path or "."))

local running = true
local ticks = 0

craft.log("Server ready! Type 'help', 'status', or 'stop' in the console.")

while running do
    ticks = ticks + 1
    craft.sleep(1000)

    -- Periodic heartbeat every 30 seconds
    if ticks % 30 == 0 then
        craft.log(string.format("[%s] Heartbeat - uptime: %d seconds", "{name}", ticks))
    end

    -- Process standard input commands
    local cmd = io.read("*l")
    if cmd then
        cmd = cmd:gsub("^%s*(.-)%s*$", "%1") -- trim whitespace
        if cmd == "stop" or cmd == "exit" or cmd == "quit" then
            craft.log("Received shutdown command. Exiting gracefully...")
            running = false
        elseif cmd == "help" then
            craft.log("Available commands: help, status, echo <msg>, stop")
        elseif cmd == "status" then
            craft.log(string.format("Status: ONLINE | Port: %d | Uptime: %ds", {port}, ticks))
        elseif cmd:sub(1, 5) == "echo " then
            craft.log("[echo] " .. cmd:sub(6))
        elseif #cmd > 0 then
            craft.log("Unknown command: '" .. cmd .. "'. Type 'help' for command list.")
        end
    end
end

craft.log("Server '{name}' stopped cleanly.")
"#
    )
}

pub fn generate_hooks_lua(name: &str) -> String {
    format!(
        r#"-- Craft Lifecycle Event Hooks
-- Server: {name}
-- Documentation: https://craft.oguzhanumutlu.com/custom-servers/hooks

-- Called immediately before the server process starts
function on_pre_start(server)
    craft.log(string.format("[hooks] Preparing to start '%s'...", server.name))
    -- Perform pre-flight checks, update configuration files, or fetch assets
end

-- Called immediately after the server process is spawned
function on_post_start(server, pid)
    craft.log(string.format("[hooks] Server '%s' started with PID %d", server.name, pid))
    -- Send webhooks, register server with master directory, etc.
end

-- Called when a stop command is issued before terminating the process
function on_pre_stop(server, pid)
    craft.log(string.format("[hooks] Stopping server '%s' (PID %d)...", server.name, pid))
    -- Broadcast server shutdown messages, flush state, etc.
end

-- Called after the server process has fully exited
function on_post_stop(server, exit_code)
    craft.log(string.format("[hooks] Server '%s' exited with code %d", server.name, exit_code))
    -- Archive logs, cleanup temp files, or trigger post-run backups
end

-- Optional custom health probe (return true if server is healthy, false otherwise)
function health_check(server)
    -- Default to true (or inspect custom metrics / port sockets)
    return true
end
"#
    )
}

pub fn generate_server_sh(name: &str, port: u16) -> String {
    format!(
        r#"#!/bin/sh
# Custom Game Server Script Wrapper
# Server: {name} | Port: {port}

echo "[Craft] Starting server '{name}' on port {port}..."
running=1
ticks=0

cleanup() {{
    echo "[Craft] Caught signal, stopping '{name}'..."
    running=0
}}

trap cleanup INT TERM

echo "[Craft] Server running. Type 'stop' or 'help' to interact."

while [ $running -eq 1 ]; do
    ticks=$((ticks + 1))
    sleep 1

    if [ $((ticks % 30)) -eq 0 ]; then
        echo "[Craft] [{name}] Heartbeat - uptime: ${{ticks}}s"
    fi

    # Read non-blocking input line if available
    if read -t 0 2>/dev/null; then
        read -r cmd
        case "$cmd" in
            stop|exit|quit)
                echo "[Craft] Exiting..."
                running=0
                ;;
            help)
                echo "[Craft] Commands: help, status, stop"
                ;;
            status)
                echo "[Craft] Status: ONLINE | Port: {port} | Uptime: ${{ticks}}s"
                ;;
            *)
                if [ -n "$cmd" ]; then
                    echo "[Craft] Unknown command: $cmd"
                fi
                ;;
        esac
    fi
done

echo "[Craft] Server '{name}' stopped."
"#
    )
}

pub fn generate_server_cmd(name: &str, port: u16) -> String {
    format!(
        r#"@echo off
rem Custom Game Server Script Wrapper
rem Server: {name} | Port: {port}

echo [Craft] Starting server '{name}' on port {port}...
set ticks=0

:loop
set /a ticks+=1
timeout /t 1 >nul

set /a rem=ticks %% 30
if %rem% equ 0 (
    echo [Craft] [{name}] Heartbeat - uptime: %ticks%s
)

goto loop
"#
    )
}

pub fn generate_start_script(config: &CustomServerConfig, is_windows: bool) -> String {
    let name = &config.server.name;
    let exec = &config.runtime.executable;
    let args_str = config.runtime.arguments.join(" ");

    if is_windows {
        match config.server.runtime_type {
            CustomRuntimeType::Lua => {
                format!(
                    "@echo off\r\n\
                    rem Craft Lua Server Launcher\r\n\
                    craft lua \"{}\" {}\r\n",
                    exec, args_str
                )
            }
            CustomRuntimeType::Script => {
                format!(
                    "@echo off\r\n\
                    call \"{}\" {}\r\n",
                    exec, args_str
                )
            }
            CustomRuntimeType::Binary => {
                format!(
                    "@echo off\r\n\
                    if not exist \"{exec}\" (\r\n\
                        echo [Craft] ==========================================================\r\n\
                        echo [Craft] Custom Game Server: '{name}'\r\n\
                        echo [Craft] Target executable '{exec}' was not found.\r\n\
                        echo [Craft] Place your game server executable in this directory\r\n\
                        echo [Craft] or update 'craft.custom.toml'.\r\n\
                        echo [Craft] ==========================================================\r\n\
                        pause\r\n\
                        exit /b 1\r\n\
                    )\r\n\
                    \"{exec}\" {args_str}\r\n"
                )
            }
        }
    } else {
        match config.server.runtime_type {
            CustomRuntimeType::Lua => {
                format!(
                    "#!/bin/sh\n\
                    # Craft Lua Server Launcher\n\
                    exec craft lua \"{}\" {}\n",
                    exec, args_str
                )
            }
            CustomRuntimeType::Script => {
                format!(
                    "#!/bin/sh\n\
                    exec \"{}\" {}\n",
                    exec, args_str
                )
            }
            CustomRuntimeType::Binary => {
                format!(
                    "#!/bin/sh\n\
                    if [ ! -f \"{exec}\" ]; then\n\
                        echo \"[Craft] ==========================================================\"\n\
                        echo \"[Craft] Custom Game Server: '{name}'\"\n\
                        echo \"[Craft] Target executable '{exec}' was not found in:\"\n\
                        echo \"[Craft]   $(pwd)\"\n\
                        echo \"[Craft]\"\n\
                        echo \"[Craft] Quick Setup:\"\n\
                        echo \"[Craft] 1. Place your compiled server binary in this directory.\"\n\
                        echo \"[Craft] 2. Ensure executable permissions: chmod +x '{exec}'\"\n\
                        echo \"[Craft] 3. Configure arguments/port in 'craft.custom.toml'.\"\n\
                        echo \"[Craft] ==========================================================\"\n\
                        exit 1\n\
                    fi\n\
                    exec \"{exec}\" {args_str}\n"
                )
            }
        }
    }
}

pub fn generate_readme(config: &CustomServerConfig) -> String {
    format!(
        r#"# Custom Game Server: {}

This server is managed by **Craft Server Manager** using the Custom Game Server subsystem.

## Files in this Directory

- `craft.custom.toml`: Declarative server runtime configuration (ports, args, environment, stop commands).
- `hooks.lua`: Lifecycle event hooks executed before and after server transitions.
- `start.sh` (Linux/macOS) / `start.cmd` (Windows): The launcher script used by Craft to start the server.
{}
## Configuration Reference (`craft.custom.toml`)

```toml
[server]
name = "{}"
type = "{}" # binary | lua | script

[runtime]
executable = "{}"
arguments = [{}]

[network]
port = {}
protocol = "{}"

[lifecycle]
stop_method = "{}"
stop_command = "{}"
hooks = "hooks.lua"
```

## Running the Server

- **Interactive Console**: `craft run {} --here` or open `craft ui` -> Local Servers -> Select Server -> Live Console.
- **Background Daemon**: `craft run {}`
- **Graceful Stop**: `craft stop {}`
"#,
        config.server.name,
        match config.server.runtime_type {
            CustomRuntimeType::Lua => "- `server.lua`: The Lua server entry script.\n",
            CustomRuntimeType::Script =>
                "- `server.sh` / `server.cmd`: The shell script implementation.\n",
            CustomRuntimeType::Binary =>
                "- Copy your compiled game server binary here (e.g. `./server`).\n",
        },
        config.server.name,
        config.server.runtime_type.as_str(),
        config.runtime.executable,
        config
            .runtime
            .arguments
            .iter()
            .map(|a| format!("\"{}\"", a))
            .collect::<Vec<_>>()
            .join(", "),
        config.network.port,
        config.network.protocol,
        config.lifecycle.stop_method,
        config
            .lifecycle
            .stop_command
            .as_deref()
            .unwrap_or("stop\\n")
            .trim_end(),
        config.server.name,
        config.server.name,
        config.server.name,
    )
}

pub fn generate_all_starter_files(server_dir: &Path, config: &CustomServerConfig) -> Result<()> {
    fs::create_dir_all(server_dir)?;

    // 1. craft.custom.toml
    config.save_to_dir(server_dir)?;

    // 2. hooks.lua
    let hooks_path = server_dir.join("hooks.lua");
    if !hooks_path.exists() {
        fs::write(&hooks_path, generate_hooks_lua(&config.server.name))?;
    }

    // 3. Runtime specific files
    match config.server.runtime_type {
        CustomRuntimeType::Lua => {
            let script_path = server_dir.join(&config.runtime.executable);
            if !script_path.exists() {
                fs::write(
                    &script_path,
                    generate_server_lua(&config.server.name, config.network.port),
                )?;
            }
        }
        CustomRuntimeType::Script => {
            let sh_path = server_dir.join("server.sh");
            if !sh_path.exists() {
                fs::write(
                    &sh_path,
                    generate_server_sh(&config.server.name, config.network.port),
                )?;
                #[cfg(not(target_os = "windows"))]
                {
                    use std::os::unix::fs::PermissionsExt;
                    let _ = fs::set_permissions(&sh_path, fs::Permissions::from_mode(0o755));
                }
            }
            let cmd_path = server_dir.join("server.cmd");
            if !cmd_path.exists() {
                fs::write(
                    &cmd_path,
                    generate_server_cmd(&config.server.name, config.network.port),
                )?;
            }
        }
        CustomRuntimeType::Binary => {
            // Binary must be provided by user or placed later
        }
    }

    // 4. start.sh and start.cmd
    let start_sh = server_dir.join("start.sh");
    fs::write(&start_sh, generate_start_script(config, false))?;
    #[cfg(not(target_os = "windows"))]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&start_sh, fs::Permissions::from_mode(0o755));
    }

    let start_cmd = server_dir.join("start.cmd");
    fs::write(&start_cmd, generate_start_script(config, true))?;

    // 5. README.md
    let readme_path = server_dir.join("README.md");
    fs::write(&readme_path, generate_readme(config))?;

    Ok(())
}
