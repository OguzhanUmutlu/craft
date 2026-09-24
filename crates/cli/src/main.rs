use clap::Parser;
use colored::Colorize;
use craft_core::{CraftPaths, Result};
use std::io::IsTerminal;
use tracing_subscriber::EnvFilter;

use craft_cli::cli::{self, Cli, Commands};
use craft_cli::commands::{
    self,
    auto::handle_auto,
    backup::handle_backup,
    cache::handle_cache,
    catalog::handle_catalog,
    cluster::handle_cluster,
    dashboard::{
        backups_menu, cache_menu, daemon_menu, gui_create_server_wizard_with_name,
        handle_dashboard, ping_menu, plugins_menu, quick_start_menu, remotes_menu,
        restart_servers_menu, rm_servers_menu, stop_servers_menu, view_servers_menu,
    },
    datapack::handle_datapack,
    deploy::handle_deploy,
    dev::handle_dev,
    dockerize::handle_dockerize,
    fix::handle_fix,
    load::handle_load,
    ls::handle_ls,
    migrate::handle_migrate,
    mod_cmd::handle_mod,
    net::{handle_firewall, handle_loopback, handle_ping, handle_rcon},
    new::handle_new,
    plugin::handle_plugin,
    prop::handle_prop,
    remote::{execute_remote, handle_remote},
    restart::handle_restart,
    rm::handle_rm,
    run::handle_run,
    service::handle_service,
    stop::handle_stop,
    template::handle_template,
    trash::handle_trash,
    update::handle_update,
    ver::handle_ver,
    view::handle_view,
    world::handle_world,
};

#[tokio::main]
async fn main() {
    let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("warn"));

    tracing_subscriber::fmt().with_env_filter(filter).init();

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
        Some(Commands::Manage {
            remote_node,
            remote,
        }) => {
            if let Some(alias) = remote {
                let registry = match craft_core::RemotesRegistry::load(&paths) {
                    Ok(r) => r,
                    Err(e) => return eprintln!("{}: {}", "Error".red().bold(), e),
                };
                if let Some(host_config) = registry.find(&alias) {
                    commands::dashboard::remote_tui::manage_host_servers(&paths, host_config).await
                } else {
                    Err(craft_core::CraftError::Other(format!(
                        "Remote host '{}' not found in registry. Use 'craft remote add' or 'craft ui' to add it.",
                        alias
                    )))
                }
            } else {
                if let Some(ref node) = remote_node {
                    commands::dashboard::screen::set_remote_node(Some(node.clone()));
                    commands::dashboard::screen::set_root_breadcrumbs(&[
                        "Dashboard",
                        "Remote Hosts",
                        node,
                    ]);
                }
                handle_dashboard(&paths).await
            }
        }
        Some(Commands::New {
            name,
            name_opt,
            software,
            version,
            software_opt,
            version_opt,
            port,
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
            runtime,
            exec,
        }) => {
            let server_name = if !name.trim().is_empty() {
                name
            } else {
                name_opt.unwrap_or_default()
            };
            let sw = software.or(software_opt);
            let ver = version.or(version_opt);
            if let Some(alias) = remote {
                let mut remote_cmd = format!(
                    "craft new {} {} {}",
                    server_name,
                    sw.as_deref().unwrap_or("paper"),
                    ver.as_deref().unwrap_or("latest"),
                );
                if let Some(p) = port {
                    remote_cmd.push_str(&format!(" --port {}", p));
                }
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
                if let Some(ref r) = runtime {
                    remote_cmd.push_str(&format!(" --runtime {}", r));
                }
                if let Some(ref e) = exec {
                    remote_cmd.push_str(&format!(" --exec \"{}\"", e));
                }
                execute_remote(&alias, &remote_cmd, false, &paths)
            } else if (server_name.is_empty() || sw.is_none())
                && std::io::stdin().is_terminal()
                && !yes
            {
                gui_create_server_wizard_with_name(&server_name, &paths).await
            } else {
                handle_new(
                    &server_name,
                    sw.as_deref(),
                    ver.as_deref(),
                    port,
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
                    runtime.as_deref(),
                    exec.as_deref(),
                    &paths,
                )
                .await
            }
        }
        Some(Commands::Run {
            name,
            path,
            here,
            daemon: _,
            remote,
        }) => {
            if let Some(alias) = remote {
                let remote_cmd = format!("craft run {}{}", name, if here { " --here" } else { "" });
                execute_remote(&alias, &remote_cmd, here, &paths)
            } else if name.is_empty() && path.is_none() && !here && std::io::stdin().is_terminal() {
                quick_start_menu(&paths).await
            } else {
                handle_run(&name, path, here, &paths).await
            }
        }
        Some(Commands::Stop {
            name,
            path,
            force,
            all,
            remote,
        }) => {
            if let Some(alias) = remote {
                let remote_cmd = format!(
                    "craft stop {}{}{}",
                    name,
                    if force { " --force" } else { "" },
                    if all { " --all" } else { "" }
                );
                execute_remote(&alias, &remote_cmd, false, &paths)
            } else if name.is_empty()
                && path.is_none()
                && !force
                && !all
                && std::io::stdin().is_terminal()
            {
                stop_servers_menu(&paths).await
            } else {
                handle_stop(&name, path, force, all, &paths).await
            }
        }
        Some(Commands::Restart {
            name,
            path,
            force,
            remote,
        }) => {
            if let Some(alias) = remote {
                let remote_cmd = format!(
                    "craft restart {}{}",
                    name,
                    if force { " --force" } else { "" }
                );
                execute_remote(&alias, &remote_cmd, false, &paths)
            } else if name.is_empty() && path.is_none() && !force && std::io::stdin().is_terminal()
            {
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
        Some(Commands::Log { action, name, path }) => {
            commands::log::handle_log(&name, path, action, &paths).await
        }
        Some(Commands::Forecast { action, server, remote, json }) => {
            commands::forecast::handle_forecast(&server, remote, json, action, &paths).await
        }
        Some(Commands::Ls { remote }) => {
            if let Some(alias) = remote {
                execute_remote(&alias, "craft ls", false, &paths)
            } else {
                handle_ls(&paths).await
            }
        }
        Some(Commands::Rm {
            name,
            path,
            rf,
            remote,
        }) => {
            if let Some(alias) = remote {
                let remote_cmd = format!("craft rm {}{}", name, if rf { " -rf" } else { "" });
                execute_remote(&alias, &remote_cmd, false, &paths)
            } else if name.is_empty() && path.is_none() && !rf && std::io::stdin().is_terminal() {
                rm_servers_menu(&paths).await
            } else {
                handle_rm(&name, path, rf, &paths).await
            }
        }
        Some(Commands::Load {
            path,
            software,
            version,
            name,
        }) => handle_load(path, &software, &version, &name, &paths).await,
        Some(Commands::Ver { software }) => handle_ver(software).await,
        Some(Commands::Update { softwares }) => handle_update(softwares).await,
        Some(Commands::Cache { action }) => {
            if action.is_none() && std::io::stdin().is_terminal() {
                cache_menu(&paths)
            } else {
                handle_cache(action, &paths)
            }
        }
        Some(Commands::Catalog { action }) => handle_catalog(action, &paths).await,
        Some(Commands::Service { action }) => {
            if let Some(act) = action {
                handle_service(act, &paths).await
            } else if std::io::stdin().is_terminal() {
                daemon_menu(&paths).await
            } else {
                handle_service(cli::ServiceCommands::Status, &paths).await
            }
        }
        Some(Commands::Auto { action }) => handle_auto(action, &paths).await,
        Some(Commands::Fix { name, path }) => handle_fix(&name, path, &paths).await,
        Some(Commands::Plugin { action }) => {
            if let Some(act) = action {
                handle_plugin(act, &paths).await
            } else if std::io::stdin().is_terminal() {
                plugins_menu(&paths).await
            } else {
                eprintln!(
                    "{}: Specify a plugin action or run interactively in a TTY.",
                    "Error".red().bold()
                );
                Ok(())
            }
        }
        Some(Commands::Mod { action }) => {
            if let Some(act) = action {
                handle_mod(act, &paths).await
            } else if std::io::stdin().is_terminal() {
                plugins_menu(&paths).await
            } else {
                eprintln!(
                    "{}: Specify a mod action or run interactively in a TTY.",
                    "Error".red().bold()
                );
                Ok(())
            }
        }
        Some(Commands::Datapack { action }) => {
            if let Some(act) = action {
                handle_datapack(act, &paths).await
            } else if std::io::stdin().is_terminal() {
                plugins_menu(&paths).await
            } else {
                eprintln!(
                    "{}: Specify a datapack action or run interactively in a TTY.",
                    "Error".red().bold()
                );
                Ok(())
            }
        }
        Some(Commands::Ping {
            target,
            bedrock,
            a2s,
        }) => {
            if target.is_empty() && std::io::stdin().is_terminal() {
                ping_menu().await
            } else {
                handle_ping(&target, bedrock, a2s).await
            }
        }
        Some(Commands::Rcon {
            server,
            password,
            command,
        }) => handle_rcon(&server, password, &command, &paths).await,
        Some(Commands::Backup { action }) => {
            if let Some(act) = action {
                handle_backup(act, &paths).await
            } else if std::io::stdin().is_terminal() {
                backups_menu(&paths).await
            } else {
                eprintln!(
                    "{}: Specify a backup action or run interactively in a TTY.",
                    "Error".red().bold()
                );
                Ok(())
            }
        }
        Some(Commands::Firewall { action }) => match action {
            cli::FirewallCommands::Allow { server, ip } => handle_firewall(&server, &ip, &paths),
        },
        Some(Commands::Loopback { action }) => handle_loopback(action),
        Some(Commands::Template { action }) => handle_template(action),
        Some(Commands::Dockerize { server }) => handle_dockerize(&server, &paths),
        Some(Commands::Remote { action }) => {
            if let Some(act) = action {
                handle_remote(act, &paths).await
            } else if std::io::stdin().is_terminal() {
                remotes_menu(&paths).await
            } else {
                handle_remote(cli::RemoteCommands::Ls, &paths).await
            }
        }
        Some(Commands::Migrate {
            action,
            server,
            to,
            remote_name,
            remote_port,
            trash_source,
            start,
        }) => {
            if let Some(act) = action {
                match act {
                    cli::MigrateCommands::Live {
                        server,
                        target_node,
                        target_host,
                        target_port,
                        freeze_max_ms,
                        json,
                    } => {
                        commands::live_migrate::handle_migrate_live(
                            &server,
                            &target_node,
                            target_host,
                            target_port,
                            freeze_max_ms,
                            json,
                            &paths,
                        )
                        .await
                    }
                    cli::MigrateCommands::Status { id, json } => {
                        commands::live_migrate::handle_migrate_status(id, json, &paths).await
                    }
                    cli::MigrateCommands::Abort { id, reason, json } => {
                        commands::live_migrate::handle_migrate_abort(id, reason, json, &paths).await
                    }
                    cli::MigrateCommands::List { json } => {
                        commands::live_migrate::handle_migrate_list(json, &paths).await
                    }
                }
            } else {
                handle_migrate(
                    &server,
                    &to,
                    remote_name,
                    remote_port,
                    trash_source,
                    start,
                    &paths,
                )
                .await
            }
        }
        Some(Commands::Cluster { action }) => {
            if let Some(act) = action {
                handle_cluster(act, &paths).await
            } else {
                handle_cluster(cli::ClusterCommands::Ls, &paths).await
            }
        }
        Some(Commands::Deploy { action }) => handle_deploy(action, &paths).await,
        Some(Commands::Prop { server, action }) => handle_prop(&server, action, &paths).await,
        Some(Commands::World { server, action }) => handle_world(&server, action, &paths).await,
        Some(Commands::Dev { server, action }) => handle_dev(&server, action, &paths).await,
        Some(Commands::Trash { action }) => handle_trash(action, &paths).await,
        Some(Commands::Lua { script, args }) => {
            craft_scripting::LuaEngine::new().and_then(|engine| engine.run_file(&script, &args))
        }
        Some(Commands::Software { action }) => {
            commands::software::handle_software(action, &paths).await
        }
        Some(Commands::Webhook { action }) => {
            commands::webhook::handle_webhook(action, &paths).await
        }
        Some(Commands::Gateway { action }) => {
            commands::gateway::handle_gateway(action, &paths).await
        }
        Some(Commands::Optimize { name, profile, apply }) => {
            commands::optimize::handle_optimize(&name, &profile, apply, &paths)
        }
        Some(Commands::Modpack { action }) => {
            commands::modpack::handle_modpack(action, &paths).await
        }
        Some(Commands::Autoscale { name, enable, disable, idle_timeout, motd, status }) => {
            commands::autoscale::handle_autoscale(&name, enable, disable, idle_timeout, motd, status, &paths).await
        }
        Some(Commands::Hibernate { name, wake }) => {
            commands::hibernate::handle_hibernate(&name, wake, &paths).await
        }
        Some(Commands::User { action }) => {
            commands::user::handle_user(action, &paths)
        }
        Some(Commands::Audit { action }) => {
            commands::audit::handle_audit(action, &paths)
        }
        Some(Commands::Dr { action }) => {
            commands::dr::handle_dr(action, &paths).await
        }
        Some(Commands::Mesh { action }) => {
            commands::mesh::handle_mesh(action, &paths)
        }
        Some(Commands::Ai { action }) => {
            commands::ai::handle_ai(action, &paths).await
        }
        Some(Commands::Edge { action }) => {
            commands::edge::handle_edge(action, &paths).await
        }
        Some(Commands::Script { action }) => {
            commands::script::handle_script(&paths, &action).await
        }
        Some(Commands::Profile { action }) => {
            commands::profile::handle_profile(action, &paths).await
        }
        Some(Commands::Sdn { action }) => {
            commands::sdn::handle_sdn(action, &paths).await
        }
        Some(Commands::Raft { action }) => {
            commands::raft::handle_raft(action, &paths).await
        }
        Some(Commands::Quota { action }) => {
            commands::quota::handle_quota(action, &paths).await
        }
        Some(Commands::Trace { action }) => {
            commands::trace::handle_trace(action, &paths).await
        }
        Some(Commands::Anvil { action }) => {
            commands::anvil::handle_anvil(action, &paths).await
        }
        Some(Commands::Numa { action }) => {
            commands::numa::handle_numa(action, &paths).await
        }
        Some(Commands::Anycast { action }) => match action {
            cli::AnycastCommands::Route {
                action,
                prefix,
                asn,
                json,
            } => {
                commands::live_migrate::handle_anycast_route(
                    &action, prefix, asn, json, &paths,
                )
                .await
            }
        },
        Some(Commands::Bpf { action }) => {
            commands::ebpf::handle_bpf(action, &paths).await
        }
        Some(Commands::Attest { action }) => {
            commands::attest::handle_attest(action, &paths).await
        }
        Some(Commands::Pqc { action }) => {
            commands::pqc::handle_pqc(action, &paths).await
        }
        Some(Commands::Hsm { action }) => {
            commands::hsm::handle_hsm(action, &paths).await
        }
        Some(Commands::Memory { action }) => {
            commands::memory::handle_memory(&paths, action).await
        }
        Some(Commands::Xdp { action }) => {
            commands::xdp::handle_xdp(&paths, action).await
        }
        Some(Commands::Pmu { action }) => {
            commands::pmu::handle_pmu(&paths, action).await
        }
        Some(Commands::Shm { action }) => {
            commands::shm::handle_shm(&paths, action).await
        }
        Some(Commands::Patch { action }) => {
            commands::patch::handle_patch(&paths, action).await
        }
        Some(Commands::Vm { action }) => {
            commands::vm::handle_vm(&paths, action).await
        }
        Some(Commands::Crash { action }) => {
            commands::crash::handle_crash(&paths, action).await
        }
    };

    if let Err(e) = result {
        modalx::terminal::restore_terminal();
        eprintln!("{}: {}", "Error".red().bold(), e);
        std::process::exit(1);
    }

    modalx::terminal::restore_terminal();
}

fn print_banner() {
    println!("{}", "Craft - Minecraft Server Toolchain".cyan().bold());
    println!(
        "{}",
        "High-performance Minecraft server management CLI, interactive dashboard, and daemon.\n"
            .dimmed()
    );
    println!("Usage: craft [COMMAND] [OPTIONS]\n");
    println!("Commands:");
    println!("  manage                            Open interactive Server Manager Dashboard");
    println!(
        "  new [name] [software] [version]   Set up a new server (interactive wizard if omitted)"
    );
    println!("  run [name] [--here]               Run an existing server (interactive selector if omitted)");
    println!("  stop [name] [--all]               Stop running servers");
    println!("  restart [name]                    Restart a running server");
    println!(
        "  view [name]                       Attach to server live console (logs/interactive)"
    );
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
    println!("  prop <server> [ls|get|set]        View/edit server.properties keys");
    println!("  world <server> [ls|info|players]  Manage worlds, inspect players & NBT");
    println!("  dev <server> [link|debug|reload]  Developer tools (JDWP debug, hot link)");
    println!("  trash [ls|restore|empty]          Manage recoverable trash bin items");
    println!("  remote <add|ls|rm|test|setup>     Manage remote hosts over SSH");
    println!("  deploy <up|down|status|logs|exec> Deploy containerized stack with Docker");
    println!("  lua <script> [args...]            Execute Lua script with Craft API");
    println!("  webhook <add|rm|ls|test>          Manage event notification webhooks");
    println!("  gateway <status|enable|metrics>   Manage WebSocket gateway & metrics");
    println!("  optimize <server> [--apply]       Dynamically tune JVM memory & GC profiles");
    println!("  modpack <inspect|install>         Universal modpack distribution engine");
    println!("  autoscale [server] [--enable]     Manage idle hibernation & wake triggers");
    println!("  hibernate <server> [--wake]       Manually sleep or wake server via SleepProxy");
    println!("  user <add|ls|rm|passwd>           Manage multi-tenant users, roles & scopes");
    println!("  audit <ls|verify>                 Cryptographically verify HMAC audit chain");
    println!("  dr <plan|test|failover|verify>    Automated disaster recovery & cold-start reconstitution");
    println!("  mesh <ls|add|rm|sync|health>      Distributed multi-cloud storage mesh & replication");
    println!("  ai <status|analyze|profile|policy> Autonomous intelligence & predictive performance");
    println!("  edge <status|add|probe|sync-routing> Global edge mesh, traffic routing & session handoffs");
    println!("  profile <tick|packets|histogram>  Real-time tick profiling, Netty inspection & histograms");
    println!("  sdn <status|up|down|policy|peers> Zero-trust mesh & eBPF microsegmentation");
    println!("  raft <status|propose|lock|logs>   Distributed Raft consensus & dynamic arbitration");
    println!("  quota <list|get|set|tenant|balance> Cgroups v2 resource quotas & fair-share scheduling");
    println!("  trace <status|list|get|export>    Distributed tracing & OpenTelemetry (OTel)");
    println!("  anvil <status|inspect|bench>      Hardware-accelerated Anvil storage & io_uring");
    println!("  migrate <server|live|status>      Cold or zero-downtime live server migration");
    println!("  anycast route <announce|withdraw> Global Anycast BGP route announcements");
    println!("  bpf <trace|status|flamegraph|gc>  Autonomous eBPF kernel observability & JVM GC telemetry");
    println!("  attest <verify|inspect|policy|sign|hermetic> Cryptographic supply chain verification & SLSA signing");
    println!("  xdp <status|attach|detach|rule|bench> Autonomous eBPF XDP firewall & Anti-DDoS mitigation");
    println!("\nGlobal Flags:");
    println!("  --remote <alias>                  Execute any command on a remote host");
    println!("\nRun 'craft --help' for full flags and subcommand reference.");
}
