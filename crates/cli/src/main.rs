use std::io::IsTerminal;
use clap::Parser;
use colored::Colorize;
use craft_core::{CraftPaths, Result};
use tracing_subscriber::EnvFilter;

mod cli;
mod commands;

use cli::{Cli, Commands};
use commands::{
    auto::handle_auto,
    backup::handle_backup,
    cache::handle_cache,
    dashboard::{
        backups_menu, cache_menu, daemon_menu,
        gui_create_server_wizard_with_name, handle_dashboard, ping_menu,
        plugins_menu, quick_start_menu, remotes_menu, restart_servers_menu,
        rm_servers_menu, stop_servers_menu, view_servers_menu,
    },
    deploy::handle_deploy,
    dockerize::handle_dockerize,
    fix::handle_fix,
    load::handle_load,
    ls::handle_ls,
    net::{handle_firewall, handle_loopback, handle_ping, handle_rcon},
    new::handle_new,
    plugin::handle_plugin,
    remote::{execute_remote, handle_remote},
    restart::handle_restart,
    rm::handle_rm,
    run::handle_run,
    service::handle_service,
    stop::handle_stop,
    template::handle_template,
    update::handle_update,
    ver::handle_ver,
    view::handle_view,
};

#[tokio::main]
async fn main() {
    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new("warn"));

    tracing_subscriber::fmt()
        .with_env_filter(filter)
        .init();

    let cli = Cli::parse();
    let paths = match CraftPaths::new() {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{}: {}", "Initialization Error".red().bold(), e);
            std::process::exit(1);
        }
    };

    let result: Result<()> = match cli.command {
        None => {
            if std::io::stdin().is_terminal() {
                handle_dashboard(&paths).await
            } else {
                print_banner();
                Ok(())
            }
        }
        Some(Commands::Manage) => {
            handle_dashboard(&paths).await
        }
        Some(Commands::New {
            name,
            software,
            version,
            software_opt,
            version_opt,
            path,
            memory,
            agree_eula,
            tmp,
            no_start,
            yes,
            aikar,
            zgc,
            shenandoah,
            jvm_flags,
            remote,
        }) => {
            let sw = software.or(software_opt);
            let ver = version.or(version_opt);
            if let Some(alias) = remote {
                let mut remote_cmd = format!(
                    "craft new {} {} {}",
                    name,
                    sw.as_deref().unwrap_or("paper"),
                    ver.as_deref().unwrap_or("latest"),
                );
                if let Some(ref m) = memory {
                    remote_cmd.push_str(&format!(" --memory {}", m));
                }
                if agree_eula {
                    remote_cmd.push_str(" --agree-eula");
                }
                if no_start {
                    remote_cmd.push_str(" --no-start");
                }
                if yes {
                    remote_cmd.push_str(" --yes");
                }
                if aikar {
                    remote_cmd.push_str(" --aikar");
                }
                if zgc {
                    remote_cmd.push_str(" --zgc");
                }
                if shenandoah {
                    remote_cmd.push_str(" --shenandoah");
                }
                if let Some(ref flags) = jvm_flags {
                    remote_cmd.push_str(&format!(" --jvm-flags \"{}\"", flags.join(" ")));
                }
                execute_remote(&alias, &remote_cmd, false, &paths)
            } else if (name.is_empty() || sw.is_none()) && std::io::stdin().is_terminal() && !yes {
                gui_create_server_wizard_with_name(&name, &paths).await
            } else {
                handle_new(
                    &name,
                    sw.as_deref(),
                    ver.as_deref(),
                    path,
                    memory.as_deref(),
                    agree_eula,
                    tmp,
                    no_start,
                    yes,
                    aikar,
                    zgc,
                    shenandoah,
                    jvm_flags,
                    &paths,
                ).await
            }
        }
        Some(Commands::Run { name, path, here, remote }) => {
            if let Some(alias) = remote {
                let remote_cmd = format!("craft run {}{}", name, if here { " --here" } else { "" });
                execute_remote(&alias, &remote_cmd, here, &paths)
            } else if name.is_empty() && path.is_none() && !here && std::io::stdin().is_terminal() {
                quick_start_menu(&paths).await
            } else {
                handle_run(&name, path, here, &paths).await
            }
        }
        Some(Commands::Stop { name, path, force, all, remote }) => {
            if let Some(alias) = remote {
                let remote_cmd = format!(
                    "craft stop {}{}{}",
                    name,
                    if force { " --force" } else { "" },
                    if all { " --all" } else { "" }
                );
                execute_remote(&alias, &remote_cmd, false, &paths)
            } else if name.is_empty() && path.is_none() && !force && !all && std::io::stdin().is_terminal() {
                stop_servers_menu(&paths).await
            } else {
                handle_stop(&name, path, force, all, &paths).await
            }
        }
        Some(Commands::Restart { name, path, force, remote }) => {
            if let Some(alias) = remote {
                let remote_cmd = format!("craft restart {}{}", name, if force { " --force" } else { "" });
                execute_remote(&alias, &remote_cmd, false, &paths)
            } else if name.is_empty() && path.is_none() && !force && std::io::stdin().is_terminal() {
                restart_servers_menu(&paths).await
            } else {
                handle_restart(&name, path, force, &paths).await
            }
        }
        Some(Commands::View { name, path, remote }) => {
            if let Some(alias) = remote {
                let remote_cmd = format!("craft view {}", name);
                execute_remote(&alias, &remote_cmd, true, &paths)
            } else if name.is_empty() && path.is_none() && std::io::stdin().is_terminal() {
                view_servers_menu(&paths).await
            } else {
                handle_view(&name, path, &paths).await
            }
        }
        Some(Commands::Ls { remote }) => {
            if let Some(alias) = remote {
                execute_remote(&alias, "craft ls", false, &paths)
            } else {
                handle_ls(&paths).await
            }
        }
        Some(Commands::Rm { name, path, rf, remote }) => {
            if let Some(alias) = remote {
                let remote_cmd = format!("craft rm {}{}", name, if rf { " -rf" } else { "" });
                execute_remote(&alias, &remote_cmd, false, &paths)
            } else if name.is_empty() && path.is_none() && !rf && std::io::stdin().is_terminal() {
                rm_servers_menu(&paths).await
            } else {
                handle_rm(&name, path, rf, &paths).await
            }
        }
        Some(Commands::Load { path, software, version, name }) => {
            handle_load(path, &software, &version, &name, &paths).await
        }
        Some(Commands::Ver { software }) => {
            handle_ver(software).await
        }
        Some(Commands::Update { softwares }) => {
            handle_update(softwares).await
        }
        Some(Commands::Cache { action }) => {
            if action.is_none() && std::io::stdin().is_terminal() {
                cache_menu(&paths)
            } else {
                handle_cache(action, &paths)
            }
        }
        Some(Commands::Service { action }) => {
            if let Some(act) = action {
                handle_service(act, &paths).await
            } else if std::io::stdin().is_terminal() {
                daemon_menu(&paths).await
            } else {
                handle_service(crate::cli::ServiceCommands::Status, &paths).await
            }
        }
        Some(Commands::Auto { action }) => {
            handle_auto(action, &paths).await
        }
        Some(Commands::Fix { name, path }) => {
            handle_fix(&name, path, &paths).await
        }
        Some(Commands::Plugin { action }) => {
            if let Some(act) = action {
                handle_plugin(act, &paths).await
            } else if std::io::stdin().is_terminal() {
                plugins_menu(&paths).await
            } else {
                eprintln!("{}: Specify a plugin action or run interactively in a TTY.", "Error".red().bold());
                Ok(())
            }
        }
        Some(Commands::Ping { target, bedrock }) => {
            if target.is_empty() && std::io::stdin().is_terminal() {
                ping_menu().await
            } else {
                handle_ping(&target, bedrock).await
            }
        }
        Some(Commands::Rcon { server, password, command }) => {
            handle_rcon(&server, password, &command, &paths).await
        }
        Some(Commands::Backup { action }) => {
            if let Some(act) = action {
                handle_backup(act, &paths).await
            } else if std::io::stdin().is_terminal() {
                backups_menu(&paths).await
            } else {
                eprintln!("{}: Specify a backup action or run interactively in a TTY.", "Error".red().bold());
                Ok(())
            }
        }
        Some(Commands::Firewall { action }) => {
            match action {
                cli::FirewallCommands::Allow { server, ip } => {
                    handle_firewall(&server, &ip, &paths)
                }
            }
        }
        Some(Commands::Loopback { action }) => {
            handle_loopback(action)
        }
        Some(Commands::Template { action }) => {
            handle_template(action)
        }
        Some(Commands::Dockerize { server }) => {
            handle_dockerize(&server, &paths)
        }
        Some(Commands::Remote { action }) => {
            if let Some(act) = action {
                handle_remote(act, &paths).await
            } else if std::io::stdin().is_terminal() {
                remotes_menu(&paths).await
            } else {
                handle_remote(crate::cli::RemoteCommands::Ls, &paths).await
            }
        }
        Some(Commands::Deploy { action }) => {
            handle_deploy(action, &paths)
        }
    };

    if let Err(e) = result {
        eprintln!("{}: {}", "Error".red().bold(), e);
        std::process::exit(1);
    }
}

fn print_banner() {
    println!("{}", "Craft - Minecraft Server Toolchain".cyan().bold());
    println!("{}", "High-performance Minecraft server management CLI, interactive dashboard, and daemon.\n".dimmed());
    println!("Usage: craft [COMMAND] [OPTIONS]\n");
    println!("Commands:");
    println!("  manage                            Open interactive Server Manager Dashboard");
    println!("  new [name] [software] [version]   Set up a new server (interactive wizard if omitted)");
    println!("  run [name] [--here]               Run an existing server (interactive selector if omitted)");
    println!("  stop [name] [--all]               Stop running servers");
    println!("  restart [name]                    Restart a running server");
    println!("  view [name]                       Attach to server live console (logs/interactive)");
    println!("  ls                                List all registered servers and status");
    println!("  rm [name] [-rf]                   Unregister or delete a server");
    println!("  load <path> <software> <version>  Load an existing server directory");
    println!("  ver [software]                    List available software/versions");
    println!("  update                            Update version lists");
    println!("  cache                             Manage download cache");
    println!("  service <start|stop|status>       Manage background daemon");
    println!("  auto <how|add|rm|start>           Configure auto-run on boot");
    println!("  fix [name]                        Diagnose and repair server");
    println!("  plugin search <query>             Search plugins (Modrinth/Hangar/Poggit)");
    println!("  ping <target>                     Ping Java or Bedrock server");
    println!("  backup create <server>            Create compressed world snapshot");
    println!("  template plugin <name>            Scaffold plugin/datapack project");
    println!("  dockerize <server>                Generate Dockerfile & compose file");
    println!("  remote <add|ls|rm|test|setup>     Manage remote hosts over SSH");
    println!("  deploy <up|down|status|logs|exec> Deploy containerized stack with Docker");
    println!("\nGlobal Flags:");
    println!("  --remote <alias>                  Execute any command on a remote host");
    println!("\nRun 'craft --help' for full flags and subcommand reference.");
}

