use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::process::Stdio;
use colored::Colorize;
use tokio::process::Command;
use craft_core::{CraftError, CraftPaths, Result, ServersRegistry};
use craft_daemon::DaemonClient;

pub async fn handle_run(
    name_arg: &str,
    custom_path: Option<PathBuf>,
    here: bool,
    paths: &CraftPaths,
) -> Result<()> {
    let server_path = paths.resolve_server_path(
        custom_path.as_deref(),
        if name_arg.is_empty() { None } else { Some(name_arg) },
        true,
    )?;

    if !server_path.exists() {
        return Err(CraftError::InvalidPath(server_path.to_string_lossy().to_string()));
    }

    let registry = ServersRegistry::load(paths)?;
    if registry.find_by_path(&server_path).is_none() {
        return Err(CraftError::ServerNotFound(format!(
            "Server at '{}' is not registered. Use 'craft load' or 'craft new' first.",
            server_path.display()
        )));
    }

    if here {
        run_foreground_server(&server_path).await
    } else {
        DaemonClient::ensure_daemon_started(paths).await?;
        let mut client = DaemonClient::connect(paths).await?;
        client.start_server(&server_path).await?;
        println!("{}", format!("Server at '{}' started successfully in the background!", server_path.display()).green());
        println!("{}", format!("Use 'craft view {}' to attach to its live console.", server_path.file_name().unwrap_or_default().to_string_lossy()).dimmed());
        Ok(())
    }
}

pub async fn run_foreground_server(server_path: &Path) -> Result<()> {
    let script = if cfg!(windows) {
        server_path.join("start.cmd")
    } else {
        server_path.join("start.sh")
    };

    if !script.exists() {
        return Err(CraftError::Other(format!(
            "Start script not found at '{}'",
            script.display()
        )));
    }

    let mut cmd = if cfg!(windows) {
        let mut c = Command::new("cmd.exe");
        c.arg("/c").arg(&script);
        c
    } else {
        let mut c = Command::new("sh");
        c.arg(&script);
        c
    };

    cmd.current_dir(server_path)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit());

    let mut child = cmd.spawn()
        .map_err(|e| CraftError::Process(format!("Failed to start server: {}", e)))?;

    let status = child.wait().await
        .map_err(|e| CraftError::Process(format!("Error waiting for server process: {}", e)))?;

    // Check EULA after exit if it failed because EULA was not accepted
    let eula_file = server_path.join("eula.txt");
    if eula_file.exists() {
        if let Ok(content) = fs::read_to_string(&eula_file) {
            if content.contains("eula=false") {
                println!("\n{}", "You need to accept the Mojang EULA to run this server.".yellow().bold());
                print!("{}", "Type 'agree' to accept the EULA and restart the server: ".yellow());
                let _ = io::stdout().flush();

                let mut input = String::new();
                let _ = io::stdin().read_line(&mut input);

                if input.trim().eq_ignore_ascii_case("agree") {
                    let updated = content.replace("eula=false", "eula=true");
                    let _ = fs::write(&eula_file, updated);
                    println!("{}", "EULA accepted. Restarting server...".green());
                    return Box::pin(run_foreground_server(server_path)).await;
                } else {
                    println!("{}", "EULA was not accepted. Server will not run.".red());
                    return Ok(());
                }
            }
        }
    }

    println!("{}", format!("\nServer process exited with code {:?}", status.code()).dimmed());
    Ok(())
}
