use crate::protocol::{IpcRequest, IpcResponse};
use crate::supervisor::Supervisor;
use craft_core::{
    is_process_running, read_pid_file, remove_pid_file, write_pid_file, CraftError, CraftPaths,
    Result,
};
use std::path::{Path, PathBuf};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tracing::{error, info, warn};

pub const DAEMON_PORT: u16 = 8123;

pub struct DaemonServer {
    paths: CraftPaths,
    supervisor: Supervisor,
    tick_service: crate::tick_service::TickService,
}

impl DaemonServer {
    pub fn new(paths: CraftPaths) -> Self {
        let supervisor = Supervisor::new(paths.clone());
        let tick_service = crate::tick_service::TickService::new(paths.clone());
        Self { paths, supervisor, tick_service }
    }

    pub async fn run(self) -> Result<()> {
        let pid = std::process::id();
        write_pid_file(&self.paths.pid_file, pid)?;

        // Initialize telemetry uptime tracking
        crate::telemetry::init_telemetry_start_time();

        // Auto-start configured servers
        self.supervisor.auto_start_servers().await;

        // Launch in-process automated backup scheduler
        let _scheduler_handle =
            crate::scheduler::DaemonScheduler::start(self.paths.clone(), self.supervisor.clone());

        // Launch in-process hibernation manager
        let hibernation =
            crate::hibernation::HibernationManager::start(self.paths.clone(), self.supervisor.clone());

        // Launch in-process Autopilot operational intelligence engine
        let autopilot = std::sync::Arc::new(crate::autopilot::AutopilotEngine::new(self.paths.clone()));
        let ap_sup = self.supervisor.clone();
        let ap_engine = autopilot.clone();
        tokio::spawn(async move {
            ap_engine.run_autonomous_loop(ap_sup, 5).await;
        });

        // Launch in-process Edge State Broker
        let edge_broker = std::sync::Arc::new(crate::edge_broker::EdgeStateBroker::new());

        // Launch in-process Tick Profiling & Netty Packet Telemetry Service
        self.tick_service.clone().start_sampling_loop(self.supervisor.clone(), 3);

        // Launch in-process Fleet Healer and Canary Rollout Engine
        let fleet_healer = std::sync::Arc::new(crate::fleet_healer::FleetHealer::new(
            self.paths.clone(),
            self.supervisor.clone(),
            self.tick_service.clone(),
        ));
        fleet_healer.clone().start_autonomous_loop(5);

        // Launch in-process Log Ingestion and Incident Forensics Engine
        let log_indexer = std::sync::Arc::new(crate::log_indexer::LogIngestionService::new(&self.paths));

        // Launch in-process Workload Forecasting and Predictive Auto-Scaling Service
        let forecasting = std::sync::Arc::new(crate::forecasting_service::WorkloadForecastingService::new(
            &self.paths,
            hibernation.clone(),
        ));
        forecasting.clone().start_worker();

        // Launch in-process Modpack CI/CD, Binary Delta & Range Chunk Distribution Service
        let modpack_service = std::sync::Arc::new(crate::modpack_service::ModpackDistributionService::new(
            &self.paths,
        ));

        // Load settings to check gateway and storage monitor config
        let settings = craft_core::GlobalSettings::load(&self.paths).unwrap_or_default();

        // Launch WebSocket & HTTP Gateway Server if enabled
        if settings.gateway_enabled {
            let bind = settings.gateway_bind.clone();
            let port = settings.gateway_port;
            let token = settings.gateway_token.clone();
            let p_gw = self.paths.clone();
            let s_gw = self.supervisor.clone();
            let gw = crate::gateway::GatewayServer::new(bind, port, token, s_gw, p_gw);
            if let Err(e) = gw.start() {
                error!("Gateway server error: {}", e);
            }
        }

        // Launch disk storage exhaustion background monitor
        let p_storage = self.paths.clone();
        let warn_threshold_bytes = settings.storage_warning_threshold_bytes;
        let warn_threshold_percent = settings.storage_warning_threshold_percent;
        tokio::spawn(async move {
            run_storage_monitor(p_storage, warn_threshold_bytes, warn_threshold_percent).await;
        });

        #[cfg(not(target_os = "windows"))]
        {
            if self.paths.socket_file.exists() {
                let _ = std::fs::remove_file(&self.paths.socket_file);
            }
            let listener = tokio::net::UnixListener::bind(&self.paths.socket_file)
                .map_err(|e| CraftError::Ipc(format!("Failed to bind UNIX socket: {}", e)))?;
            info!(
                "Craft daemon listening on UNIX socket: {}",
                self.paths.socket_file.display()
            );

            let supervisor = self.supervisor.clone();
            let hib_mgr = hibernation.clone();
            let ap_mgr = autopilot.clone();
            let eb_mgr = edge_broker.clone();
            let ts_mgr = self.tick_service.clone();
            let fh_mgr = fleet_healer.clone();
            let li_mgr = log_indexer.clone();
            let fc_mgr = forecasting.clone();
            let mp_mgr = modpack_service.clone();
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let sup = supervisor.clone();
                        let hib = hib_mgr.clone();
                        let ap = ap_mgr.clone();
                        let eb = eb_mgr.clone();
                        let ts = ts_mgr.clone();
                        let fh = fh_mgr.clone();
                        let li = li_mgr.clone();
                        let fc = fc_mgr.clone();
                        let mp = mp_mgr.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, sup, hib, ap, eb, ts, fh, li, fc, mp).await {
                                warn!("IPC client disconnected with error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Error accepting IPC connection: {}", e);
                    }
                }
            }
        }

        #[cfg(target_os = "windows")]
        {
            use tokio::net::windows::named_pipe::ServerOptions;
            let pipe_name = r"\\.\pipe\craft-daemon";
            info!(
                "Craft daemon listening on Windows Named Pipe: {}",
                pipe_name
            );

            let mut server = ServerOptions::new()
                .first_pipe_instance(true)
                .create(pipe_name)
                .map_err(|e| CraftError::Ipc(format!("Failed to create named pipe: {}", e)))?;

            let supervisor = self.supervisor.clone();
            let hib_mgr = hibernation.clone();
            let ap_mgr = autopilot.clone();
            let eb_mgr = edge_broker.clone();
            let ts_mgr = self.tick_service.clone();
            let fh_mgr = fleet_healer.clone();
            let li_mgr = log_indexer.clone();
            let fc_mgr = forecasting.clone();
            let mp_mgr = modpack_service.clone();
            loop {
                if let Err(e) = server.connect().await {
                    error!("Error connecting named pipe client: {}", e);
                    continue;
                }
                let client = server;
                server = ServerOptions::new().create(pipe_name).map_err(|e| {
                    CraftError::Ipc(format!("Failed to create next pipe instance: {}", e))
                })?;

                let sup = supervisor.clone();
                let hib = hib_mgr.clone();
                let ap = ap_mgr.clone();
                let eb = eb_mgr.clone();
                let ts = ts_mgr.clone();
                let fh = fh_mgr.clone();
                let li = li_mgr.clone();
                let fc = fc_mgr.clone();
                let mp = mp_mgr.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(client, sup, hib, ap, eb, ts, fh, li, fc, mp).await {
                        warn!("IPC client error: {}", e);
                    }
                });
            }
        }
    }
}

impl Drop for DaemonServer {
    fn drop(&mut self) {
        remove_pid_file(&self.paths.pid_file);
        #[cfg(not(target_os = "windows"))]
        {
            let _ = std::fs::remove_file(&self.paths.socket_file);
        }
    }
}

async fn handle_connection<S>(
    mut stream: S,
    supervisor: Supervisor,
    hibernation: std::sync::Arc<crate::hibernation::HibernationManager>,
    autopilot: std::sync::Arc<crate::autopilot::AutopilotEngine>,
    edge_broker: std::sync::Arc<crate::edge_broker::EdgeStateBroker>,
    tick_service: crate::tick_service::TickService,
    fleet_healer: std::sync::Arc<crate::fleet_healer::FleetHealer>,
    log_indexer: std::sync::Arc<crate::log_indexer::LogIngestionService>,
    forecasting: std::sync::Arc<crate::forecasting_service::WorkloadForecastingService>,
    modpack_service: std::sync::Arc<crate::modpack_service::ModpackDistributionService>,
) -> Result<()>
where
    S: AsyncRead + AsyncWrite + Unpin + Send + 'static,
{
    loop {
        let req = match read_frame::<S, IpcRequest>(&mut stream).await {
            Ok(Some(r)) => r,
            Ok(None) => break, // Connection closed
            Err(e) => return Err(e),
        };

        match req {
            IpcRequest::Ping => {
                write_frame(&mut stream, &IpcResponse::Pong).await?;
            }
            IpcRequest::GetRunning => {
                let paths = supervisor.get_running_paths().await;
                write_frame(&mut stream, &IpcResponse::RunningList { paths }).await?;
            }
            IpcRequest::StartServer { path } => {
                let resp = match supervisor.start_server(&path).await {
                    Ok(_) => IpcResponse::Success {
                        message: format!("Server '{}' started successfully", path.display()),
                    },
                    Err(CraftError::Other(msg)) if msg.contains("already running") => {
                        IpcResponse::AlreadyRunning { path }
                    }
                    Err(e) => IpcResponse::Error {
                        error: e.to_string(),
                    },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::StopServer { path, force } => {
                let resp = match supervisor.stop_server(&path, force).await {
                    Ok(_) => IpcResponse::Success {
                        message: format!("Server '{}' stopped successfully", path.display()),
                    },
                    Err(e) => IpcResponse::Error {
                        error: e.to_string(),
                    },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::SendInput { path, input } => {
                let resp = match supervisor.send_input(&path, &input).await {
                    Ok(_) => IpcResponse::Success {
                        message: "Input sent".to_string(),
                    },
                    Err(e) => IpcResponse::Error {
                        error: e.to_string(),
                    },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::AttachConsole { path } => {
                match supervisor.get_console_stream(&path).await {
                    Ok((backlog, mut rx)) => {
                        write_frame(
                            &mut stream,
                            &IpcResponse::LogBacklog {
                                path: path.clone(),
                                data: backlog,
                            },
                        )
                        .await?;

                        // Streaming loop
                        let (mut reader, mut writer) = tokio::io::split(stream);

                        let write_task = tokio::spawn(async move {
                            while let Ok(line) = rx.recv().await {
                                let resp = IpcResponse::LogChunk {
                                    path: path.clone(),
                                    data: line,
                                };
                                if write_frame(&mut writer, &resp).await.is_err() {
                                    break;
                                }
                            }
                        });

                        let sup = supervisor.clone();
                        let read_task = tokio::spawn(async move {
                            while let Ok(Some(req)) = read_frame::<_, IpcRequest>(&mut reader).await
                            {
                                match req {
                                    IpcRequest::SendInput { path, input } => {
                                        let _ = sup.send_input(&path, &input).await;
                                    }
                                    IpcRequest::DetachConsole { .. } => break,
                                    _ => {}
                                }
                            }
                        });

                        tokio::select! {
                            _ = write_task => {},
                            _ = read_task => {},
                        }
                        return Ok(());
                    }
                    Err(e) => {
                        write_frame(
                            &mut stream,
                            &IpcResponse::Error {
                                error: e.to_string(),
                            },
                        )
                        .await?;
                    }
                }
            }
            IpcRequest::DetachConsole { .. } => {}
            IpcRequest::GetCircuitBreakers => {
                let items = supervisor.get_circuit_breaker_infos().await;
                write_frame(&mut stream, &IpcResponse::CircuitBreakersList { items }).await?;
            }
            IpcRequest::ResetCircuitBreaker { path } => {
                supervisor.reset_circuit_breaker(&path).await;
                write_frame(
                    &mut stream,
                    &IpcResponse::Success {
                        message: format!("Reset circuit breaker for '{}'", path.display()),
                    },
                )
                .await?;
            }
            IpcRequest::GetBackupSchedules => {
                let mut items = Vec::new();
                if let Ok(reg) = craft_core::GlobalBackupRegistry::load(supervisor.paths()) {
                    for (name, policy) in reg.server_policies {
                        let last_str = policy.last_backup_timestamp.and_then(|ts| {
                            chrono::DateTime::from_timestamp(ts, 0)
                                .map(|dt| dt.format("%Y-%m-%d %H:%M:%S UTC").to_string())
                        });
                        items.push(crate::scheduler::BackupScheduleInfo {
                            server_name: name,
                            enabled: policy.enabled,
                            schedule: policy.schedule_display(),
                            retention_count: policy.retention_count,
                            format: policy.compression_format().to_string(),
                            last_backup: last_str,
                        });
                    }
                }
                write_frame(&mut stream, &IpcResponse::BackupSchedulesList { items }).await?;
            }
            IpcRequest::HibernateServer { server_name } => {
                let resp = match hibernation.hibernate_server(&server_name).await {
                    Ok(()) => IpcResponse::Success {
                        message: format!("Server '{}' hibernated successfully", server_name),
                    },
                    Err(e) => IpcResponse::Error {
                        error: e.to_string(),
                    },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::WakeServer { server_name } => {
                let resp = match hibernation.wake_server(&server_name).await {
                    Ok(()) => IpcResponse::Success {
                        message: format!("Server '{}' woken up successfully", server_name),
                    },
                    Err(e) => IpcResponse::Error {
                        error: e.to_string(),
                    },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::GetAutoscaleStatus => {
                let items = hibernation.get_autoscale_status().await.unwrap_or_default();
                write_frame(&mut stream, &IpcResponse::AutoscaleStatusList { items }).await?;
            }
            IpcRequest::GetIntelligenceStatus { server } => {
                let policy = craft_core::IntelligencePolicy::default();
                let reports = match server {
                    Some(s) => {
                        if let Some(rep) = autopilot.get_diagnostic_report(&s, &policy).await {
                            vec![rep]
                        } else {
                            vec![]
                        }
                    }
                    None => {
                        let mut list = Vec::new();
                        if let Ok(reg) = craft_core::ServersRegistry::load(autopilot.paths()) {
                            for s in reg.servers {
                                if let Some(rep) = autopilot.get_diagnostic_report(&s.name, &policy).await {
                                    list.push(rep);
                                }
                            }
                        }
                        list
                    }
                };
                write_frame(&mut stream, &IpcResponse::IntelligenceReports { items: reports }).await?;
            }
            IpcRequest::TriggerDiagnosticRun { server, duration_secs } => {
                let policy = craft_core::IntelligencePolicy::default();
                let canonical = match craft_core::ServersRegistry::load(autopilot.paths()) {
                    Ok(r) => r.find_by_name(&server).map(|s| s.path.clone()),
                    Err(_) => None,
                };
                let pid = match canonical.as_ref() {
                    Some(p) => supervisor.get_server_pid(p).await,
                    None => None,
                };
                let _ = autopilot.trigger_jfr_profiling(&server, pid, duration_secs).await;
                let report = autopilot.get_diagnostic_report(&server, &policy).await.unwrap_or_else(|| {
                    craft_core::DiagnosticReport {
                        timestamp: std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs(),
                        server_name: server.clone(),
                        tps_current: 20.0,
                        mspt_current: 20.0,
                        mspt_p50: 20.0,
                        mspt_p95: 20.0,
                        mspt_p99: 20.0,
                        memory_rss_bytes: 0,
                        memory_growth_rate_mb_min: 0.0,
                        predicted_tte_seconds: None,
                        cpu_usage_percent: 0.0,
                        active_players: 0,
                        anomalies: vec![],
                        recommendations: vec![],
                        jfr_profile_file: None,
                    }
                });
                let markdown = craft_core::format_report_markdown(&report);
                write_frame(&mut stream, &IpcResponse::DiagnosticRunCompleted { report, markdown }).await?;
            }
            IpcRequest::ExecuteRemediation { server, action, dry_run } => {
                let canonical = match craft_core::ServersRegistry::load(autopilot.paths()) {
                    Ok(r) => r.find_by_name(&server).map(|s| s.path.clone()),
                    Err(_) => None,
                };
                if let Some(ref path) = canonical {
                    let pid = supervisor.get_server_pid(path).await;
                    let policy = craft_core::IntelligencePolicy::default();
                    let res = autopilot.execute_remediation(&server, path, action, dry_run, pid, &supervisor, &policy).await;
                    match res {
                        Ok(msg) => write_frame(&mut stream, &IpcResponse::RemediationResult { message: msg }).await?,
                        Err(e) => write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?,
                    }
                } else {
                    write_frame(&mut stream, &IpcResponse::Error { error: format!("Server '{}' not found", server) }).await?;
                }
            }
            IpcRequest::UpdateIntelligencePolicy { server, policy } => {
                let mut reg = craft_core::IntelligenceRegistry::load(autopilot.paths()).unwrap_or_default();
                reg.set_policy(server, policy);
                if let Err(e) = reg.save(autopilot.paths()) {
                    write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?;
                } else {
                    write_frame(&mut stream, &IpcResponse::Success { message: "Policy updated successfully".to_string() }).await?;
                }
            }
            IpcRequest::GetEdgeMeshStatus => {
                let reg = craft_core::EdgeRegistry::load(supervisor.paths()).unwrap_or_default();
                let nodes = reg.list_nodes().to_vec();
                let backbone = edge_broker.get_backbone_conditions().await;
                write_frame(&mut stream, &IpcResponse::EdgeMeshStatus { nodes, backbone }).await?;
            }
            IpcRequest::RegisterEdgeNode { node } => {
                let res = craft_core::EdgeRegistry::modify(supervisor.paths(), |reg| {
                    reg.add_node(node)
                });
                match res {
                    Ok(()) => write_frame(&mut stream, &IpcResponse::Success { message: "Edge node registered successfully".to_string() }).await?,
                    Err(e) => write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?,
                }
            }
            IpcRequest::RemoveEdgeNode { name } => {
                let res = craft_core::EdgeRegistry::modify(supervisor.paths(), |reg| {
                    reg.remove_node(&name)
                });
                match res {
                    Ok(true) => write_frame(&mut stream, &IpcResponse::Success { message: format!("Edge node '{}' removed", name) }).await?,
                    Ok(false) => write_frame(&mut stream, &IpcResponse::Error { error: format!("Edge node '{}' not found", name) }).await?,
                    Err(e) => write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?,
                }
            }
            IpcRequest::TriggerEdgeHandoff { handoff } => {
                match edge_broker.register_handoff(handoff).await {
                    Ok(token) => write_frame(&mut stream, &IpcResponse::EdgeHandoffResult {
                        success: true,
                        message: format!("Session handoff issued with token '{}'", token),
                        handoff: None,
                    }).await?,
                    Err(e) => write_frame(&mut stream, &IpcResponse::EdgeHandoffResult {
                        success: false,
                        message: e.to_string(),
                        handoff: None,
                    }).await?,
                }
            }
            IpcRequest::ConsumeEdgeHandoff { token } => {
                match edge_broker.consume_handoff(&token).await {
                    Ok(handoff) => write_frame(&mut stream, &IpcResponse::EdgeHandoffResult {
                        success: true,
                        message: "Session handoff successfully consumed".to_string(),
                        handoff: Some(handoff),
                    }).await?,
                    Err(e) => write_frame(&mut stream, &IpcResponse::EdgeHandoffResult {
                        success: false,
                        message: e.to_string(),
                        handoff: None,
                    }).await?,
                }
            }
            IpcRequest::BroadcastEdgeChat { envelope } => {
                let secret = craft_core::DEFAULT_CHAT_SECRET;
                match edge_broker.broadcast_chat(envelope, secret).await {
                    Ok(delivered) => write_frame(&mut stream, &IpcResponse::EdgeChatBroadcastResult { delivered_nodes: delivered }).await?,
                    Err(e) => write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?,
                }
            }
            IpcRequest::ApplyLatencyPlaybook { server_name, preset } => {
                match crate::edge_broker::EdgeStateBroker::apply_playbook_to_server(supervisor.paths(), &server_name, &preset) {
                    Ok(pb) => write_frame(&mut stream, &IpcResponse::LatencyPlaybookApplied {
                        server_name: server_name.clone(),
                        view_distance: pb.view_distance,
                        simulation_distance: pb.simulation_distance,
                        message: format!("Applied latency playbook '{}' to server '{}'", pb.preset, server_name),
                    }).await?,
                    Err(e) => write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?,
                }
            }
            IpcRequest::GetTickProfile { server_name } => {
                match tick_service.get_tick_profile(&server_name).await {
                    Some((summary, sparkline)) => {
                        write_frame(&mut stream, &IpcResponse::TickProfile { summary, sparkline }).await?;
                    }
                    None => {
                        write_frame(&mut stream, &IpcResponse::Error { error: format!("No tick profile metrics available for server '{}'", server_name) }).await?;
                    }
                }
            }
            IpcRequest::GetPacketStats { server_name } => {
                match tick_service.get_packet_stats(&server_name).await {
                    Some(summary) => {
                        write_frame(&mut stream, &IpcResponse::PacketStats { summary }).await?;
                    }
                    None => {
                        write_frame(&mut stream, &IpcResponse::Error { error: format!("No packet statistics available for server '{}'", server_name) }).await?;
                    }
                }
            }
            IpcRequest::GetLatencyHistogram { server_name } => {
                match tick_service.get_latency_histogram(&server_name).await {
                    Some((histogram, chart_lines)) => {
                        write_frame(&mut stream, &IpcResponse::LatencyHistogram { histogram, chart_lines }).await?;
                    }
                    None => {
                        write_frame(&mut stream, &IpcResponse::Error { error: format!("No latency histogram available for server '{}'", server_name) }).await?;
                    }
                }
            }
            IpcRequest::StartClusterRollout { plan } => {
                match fleet_healer.start_cluster_rollout(plan) {
                    Ok(rollout_id) => write_frame(&mut stream, &IpcResponse::ClusterRolloutStarted { rollout_id }).await?,
                    Err(e) => write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?,
                }
            }
            IpcRequest::GetClusterRolloutStatus { cluster } => {
                let reg = craft_core::RolloutRegistry::load(supervisor.paths()).unwrap_or_default();
                let record = reg.get_active_rollout(&cluster).cloned();
                write_frame(&mut stream, &IpcResponse::ClusterRolloutStatus { record }).await?;
            }
            IpcRequest::AbortClusterRollout { rollout_id, reason } => {
                let res = craft_core::RolloutRegistry::modify(supervisor.paths(), |reg| {
                    reg.abort_rollout(&rollout_id, &reason)
                });
                match res {
                    Ok(()) => write_frame(&mut stream, &IpcResponse::ClusterRolloutAborted {
                        message: format!("Rollout '{}' successfully aborted", rollout_id),
                    }).await?,
                    Err(e) => write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?,
                }
            }
            IpcRequest::GetFleetHealth { cluster } => {
                match fleet_healer.evaluate_fleet_health(&cluster).await {
                    Ok(status) => write_frame(&mut stream, &IpcResponse::FleetHealth { status }).await?,
                    Err(e) => write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?,
                }
            }
            IpcRequest::ExecuteFleetHeal { cluster, action } => {
                match fleet_healer.execute_fleet_heal(&cluster, action).await {
                    Ok(msg) => write_frame(&mut stream, &IpcResponse::FleetHealResult { message: msg }).await?,
                    Err(e) => write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?,
                }
            }
            IpcRequest::SearchLogs { query } => {
                match log_indexer.search(&query).await {
                    Ok(result) => write_frame(&mut stream, &IpcResponse::LogSearchResults { result }).await?,
                    Err(e) => write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?,
                }
            }
            IpcRequest::GetIncidentForensics { server_name, incident_id } => {
                match log_indexer.get_incident_forensics(&server_name, incident_id.as_deref()) {
                    Ok(timeline) => write_frame(&mut stream, &IpcResponse::IncidentForensics { timeline }).await?,
                    Err(e) => write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?,
                }
            }
            IpcRequest::ListIncidents { server_name } => {
                match log_indexer.list_incidents(server_name.as_deref()) {
                    Ok(incidents) => write_frame(&mut stream, &IpcResponse::IncidentList { incidents }).await?,
                    Err(e) => write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?,
                }
            }
            IpcRequest::IngestLogsNow { server_name } => {
                let start = std::time::Instant::now();
                match log_indexer.ingest_all_servers(server_name.as_deref()) {
                    Ok((indexed_lines, blocks_created)) => {
                        let duration_ms = start.elapsed().as_millis() as u64;
                        write_frame(&mut stream, &IpcResponse::IngestResult {
                            indexed_lines,
                            blocks_created,
                            duration_ms,
                        }).await?;
                    }
                    Err(e) => write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?,
                }
            }
            IpcRequest::GetWorkloadForecast { server_name, horizon_hours } => {
                match forecasting.get_forecast(&server_name, horizon_hours).await {
                    Ok(forecast) => write_frame(&mut stream, &IpcResponse::WorkloadForecastResult { forecast }).await?,
                    Err(e) => write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?,
                }
            }
            IpcRequest::GetCostOptimizationReport { server_name } => {
                match forecasting.get_cost_report(server_name.as_deref()).await {
                    Ok(report) => write_frame(&mut stream, &IpcResponse::CostOptimizationReportResult { report }).await?,
                    Err(e) => write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?,
                }
            }
            IpcRequest::SetWorkloadPolicy { policy } => {
                match forecasting.set_policy(policy) {
                    Ok(()) => {
                        let policies = forecasting.list_policies().unwrap_or_default();
                        write_frame(&mut stream, &IpcResponse::WorkloadPolicyResult { policies }).await?
                    }
                    Err(e) => write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?,
                }
            }
            IpcRequest::TriggerProactiveScalingNow { server_name } => {
                match forecasting.trigger_proactive_scaling(&server_name).await {
                    Ok((message, applied_action)) => {
                        write_frame(&mut stream, &IpcResponse::ProactiveScalingResult { message, applied_action }).await?
                    }
                    Err(e) => write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?,
                }
            }
            IpcRequest::BuildModpack {
                name,
                version,
                loader,
                mc_version,
                base_path,
            } => {
                match modpack_service.build_modpack(
                    &name,
                    &version,
                    &loader,
                    &mc_version,
                    std::path::Path::new(&base_path),
                ) {
                    Ok(manifest) => {
                        write_frame(&mut stream, &IpcResponse::ModpackBuildResult { manifest }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::GenerateDelta {
                pack_name,
                source_version,
                target_version,
            } => {
                match modpack_service.generate_delta(&pack_name, &source_version, &target_version) {
                    Ok(delta_manifest) => {
                        write_frame(&mut stream, &IpcResponse::DeltaResult { delta_manifest }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::GetModpackStatus { pack_name } => {
                match modpack_service.get_status(&pack_name) {
                    Ok((versions, deltas)) => {
                        write_frame(&mut stream, &IpcResponse::ModpackStatus { versions, deltas }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::GetModpackChunk {
                file_path,
                range_header,
            } => {
                match modpack_service.read_chunk(&file_path, range_header.as_deref()) {
                    Ok(chunk) => {
                        write_frame(&mut stream, &IpcResponse::ModpackChunk { chunk }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::GetSdnTopology => {
                match crate::sdn_service::SdnService::get_topology(supervisor.paths()) {
                    Ok(topology) => {
                        write_frame(&mut stream, &IpcResponse::SdnTopologyResult { topology }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::ApplySdnPolicy { policy } => {
                match crate::sdn_service::SdnService::apply_policy(supervisor.paths(), policy) {
                    Ok(rules_count) => {
                        write_frame(
                            &mut stream,
                            &IpcResponse::SdnPolicyResult {
                                message: format!("Microsegmentation policy applied with {} active rules", rules_count),
                                rules_count,
                            },
                        )
                        .await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::RotateSdnKeys => {
                match crate::sdn_service::SdnService::rotate_keys(supervisor.paths()) {
                    Ok(summary) => {
                        write_frame(&mut stream, &IpcResponse::SdnKeyRotationResult { summary }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::GetPeerStatus { node_id } => {
                match crate::sdn_service::SdnService::get_peer_status(supervisor.paths(), &node_id) {
                    Ok(peer) => {
                        write_frame(&mut stream, &IpcResponse::SdnPeerStatusResult { peer }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::GetRaftStatus => {
                match crate::raft_service::RaftConsensusService::get_status(supervisor.paths()) {
                    Ok(status) => {
                        write_frame(&mut stream, &IpcResponse::RaftStatusResult { status }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::ProposeRaftCommand { payload } => {
                match crate::raft_service::RaftConsensusService::propose(supervisor.paths(), payload) {
                    Ok((term, index)) => {
                        write_frame(&mut stream, &IpcResponse::RaftCommandProposedResult { term, index }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::AcquireDistributedLock { lock_name, holder_id, lease_secs } => {
                match crate::raft_service::RaftConsensusService::acquire_lock(supervisor.paths(), &lock_name, &holder_id, lease_secs) {
                    Ok(lock) => {
                        write_frame(&mut stream, &IpcResponse::DistributedLockAcquiredResult { lock }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::ReleaseDistributedLock { lock_name, holder_id } => {
                match crate::raft_service::RaftConsensusService::release_lock(supervisor.paths(), &lock_name, &holder_id) {
                    Ok(()) => {
                        write_frame(&mut stream, &IpcResponse::DistributedLockReleasedResult { message: format!("Lock '{}' released", lock_name) }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::StepDownRaftLeader => {
                match crate::raft_service::RaftConsensusService::step_down(supervisor.paths()) {
                    Ok(()) => {
                        write_frame(&mut stream, &IpcResponse::RaftStepDownResult { message: "Leader stepped down".to_string() }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::TransferRaftLeadership { target_node_id } => {
                match crate::raft_service::RaftConsensusService::transfer_leadership(supervisor.paths(), &target_node_id) {
                    Ok(()) => {
                        write_frame(&mut stream, &IpcResponse::RaftLeadershipTransferredResult { message: format!("Leadership transferred to '{}'", target_node_id) }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::GetRaftLogs { limit } => {
                match crate::raft_service::RaftConsensusService::get_logs(supervisor.paths(), limit) {
                    Ok(entries) => {
                        write_frame(&mut stream, &IpcResponse::RaftLogsResult { entries }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::RaftGetMultiRaftStatus { group_id } => {
                match crate::multi_raft_service::MultiRaftService::global(supervisor.paths()).get_status(group_id) {
                    Ok((registry, statuses, learner_progress)) => {
                        write_frame(&mut stream, &IpcResponse::RaftMultiRaftStatusResult { registry, statuses, learner_progress }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::RaftReconfigureMembership { group_id, change_type, node } => {
                match crate::multi_raft_service::MultiRaftService::global(supervisor.paths()).reconfigure_membership(group_id, change_type, node) {
                    Ok((success, phase, message)) => {
                        write_frame(&mut stream, &IpcResponse::RaftReconfigureMembershipResult { success, phase, message }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::RaftTriggerCompaction { group_id, force } => {
                match crate::multi_raft_service::MultiRaftService::global(supervisor.paths()).trigger_compaction(group_id, force) {
                    Ok((last_included_index, entries_compacted, snapshot_bytes, duration_ms)) => {
                        write_frame(&mut stream, &IpcResponse::RaftCompactionResult { group_id, last_included_index, entries_compacted, snapshot_bytes, duration_ms }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::RaftRoutePartitionKey { key } => {
                match crate::multi_raft_service::MultiRaftService::global(supervisor.paths()).route_partition_key(&key) {
                    Ok((key, group_id, partition_name, leader_node_id)) => {
                        write_frame(&mut stream, &IpcResponse::RaftPartitionRouteResult { key, group_id, partition_name, leader_node_id }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::RaftManagePartition { action, partition, group_id } => {
                match crate::multi_raft_service::MultiRaftService::global(supervisor.paths()).manage_partition(&action, partition, group_id) {
                    Ok((success, message, partitions)) => {
                        write_frame(&mut stream, &IpcResponse::RaftManagePartitionResult { success, message, partitions }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::GetServerQuota { server } => {
                match crate::quota_service::QuotaService::get_server_quota(supervisor.paths(), &server) {
                    Ok(summary) => {
                        write_frame(&mut stream, &IpcResponse::ServerQuotaResult { summary }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::SetServerQuota { limits } => {
                match crate::quota_service::QuotaService::set_server_quota(supervisor.paths(), limits) {
                    Ok(summary) => {
                        write_frame(&mut stream, &IpcResponse::ServerQuotaResult { summary }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::GetTenantQuota { tenant } => {
                match crate::quota_service::QuotaService::get_tenant_quota(supervisor.paths(), &tenant) {
                    Ok((quota, allocated_memory_mb, allocated_cpu_percent, server_count)) => {
                        write_frame(&mut stream, &IpcResponse::TenantQuotaResult {
                            quota,
                            allocated_memory_mb,
                            allocated_cpu_percent,
                            server_count,
                        }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::SetTenantQuota { quota } => {
                match crate::quota_service::QuotaService::set_tenant_quota(supervisor.paths(), quota) {
                    Ok(()) => {
                        write_frame(&mut stream, &IpcResponse::Success { message: "Tenant quota updated successfully".to_string() }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::ListQuotaUsage { tenant } => {
                match crate::quota_service::QuotaService::list_quota_usage(supervisor.paths(), tenant.as_deref()) {
                    Ok(items) => {
                        write_frame(&mut stream, &IpcResponse::QuotaUsageListResult { items }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::EnforceFairShareNow => {
                match crate::quota_service::QuotaService::enforce_fair_share(supervisor.paths()) {
                    Ok((rebalanced_count, message)) => {
                        write_frame(&mut stream, &IpcResponse::FairShareEnforcedResult { rebalanced_count, message }).await?
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?
                    }
                }
            }
            IpcRequest::GetTracingStatus => {
                let status = crate::tracing_service::TracingService::global(supervisor.paths()).get_status();
                write_frame(&mut stream, &IpcResponse::TracingStatusResult { status }).await?;
            }
            IpcRequest::QueryTraces { service, name, min_duration_micros, error_only, limit } => {
                let spans = crate::tracing_service::TracingService::global(supervisor.paths())
                    .query_traces(service, name, min_duration_micros, error_only, limit);
                write_frame(&mut stream, &IpcResponse::TracesQueryResult { spans }).await?;
            }
            IpcRequest::GetTraceDetails { trace_id } => {
                let trace_tree = crate::tracing_service::TracingService::global(supervisor.paths())
                    .get_trace_details(&trace_id);
                write_frame(&mut stream, &IpcResponse::TraceDetailsResult { trace_tree }).await?;
            }
            IpcRequest::ExportTracesNow { limit } => {
                let (exported_count, destination) = crate::tracing_service::TracingService::global(supervisor.paths())
                    .export_traces_now(limit).await;
                write_frame(&mut stream, &IpcResponse::TracesExportedResult { exported_count, destination }).await?;
            }
            IpcRequest::SetTracingConfig { config } => {
                match crate::tracing_service::TracingService::global(supervisor.paths()).set_config(config) {
                    Ok(cfg) => {
                        write_frame(&mut stream, &IpcResponse::TracingConfigResult { config: cfg }).await?;
                    }
                    Err(e) => {
                        write_frame(&mut stream, &IpcResponse::Error { error: e.to_string() }).await?;
                    }
                }
            }
            IpcRequest::GetAnvilStatus => {
                let status = crate::anvil_service::AnvilService::global(supervisor.paths()).get_status();
                write_frame(&mut stream, &IpcResponse::AnvilStatusResult { status }).await?;
            }
            IpcRequest::InspectRegion { server_path, region_file } => {
                let resp = match crate::anvil_service::AnvilService::global(supervisor.paths())
                    .inspect_region(&server_path, &region_file)
                {
                    Ok(details) => IpcResponse::AnvilRegionInspectionResult { details },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::PrefetchChunks { server_path, world, center_x, center_z, radius } => {
                let resp = match crate::anvil_service::AnvilService::global(supervisor.paths())
                    .prefetch(&server_path, &world, center_x, center_z, radius)
                {
                    Ok(summary) => IpcResponse::AnvilPrefetchResult { summary },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::BenchmarkAnvil { chunks } => {
                let resp = match crate::anvil_service::AnvilService::global(supervisor.paths())
                    .benchmark(chunks)
                {
                    Ok(report) => IpcResponse::AnvilBenchmarkResult { report },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::SetAnvilConfig { config } => {
                let resp = match crate::anvil_service::AnvilService::global(supervisor.paths())
                    .set_config(config)
                {
                    Ok(cfg) => IpcResponse::AnvilConfigResult { config: cfg },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::GetNumaStatus => {
                let summary = crate::dpdk_service::DpdkNumaService::global(supervisor.paths()).get_numa_status().await;
                write_frame(&mut stream, &IpcResponse::NumaStatusResult { summary }).await?;
            }
            IpcRequest::PinServerCores { server_name, cpus, numa_node, policy } => {
                let resp = match crate::dpdk_service::DpdkNumaService::global(supervisor.paths())
                    .pin_server_cores(&server_name, cpus, numa_node, policy)
                    .await
                {
                    Ok(config) => IpcResponse::PinServerCoresResult {
                        config,
                        message: format!("Pinned server {server_name} successfully"),
                    },
                    Err(e) => IpcResponse::Error { error: e },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::SetNumaPolicy { server_name, policy } => {
                let resp = match crate::dpdk_service::DpdkNumaService::global(supervisor.paths())
                    .set_numa_policy(&server_name, policy)
                    .await
                {
                    Ok(config) => IpcResponse::NumaPolicyResult {
                        config,
                        message: format!("Updated NUMA policy for server {server_name}"),
                    },
                    Err(e) => IpcResponse::Error { error: e },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::BenchmarkNumaMemory { node_id, size_mb } => {
                let resp = match crate::dpdk_service::DpdkNumaService::global(supervisor.paths())
                    .benchmark_numa_memory(node_id, size_mb)
                    .await
                {
                    Ok(report) => IpcResponse::NumaBenchmarkResult { report },
                    Err(e) => IpcResponse::Error { error: e },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::GetDpdkStatus { bench_count } => {
                let stats = crate::dpdk_service::DpdkNumaService::global(supervisor.paths())
                    .get_dpdk_status(bench_count);
                write_frame(&mut stream, &IpcResponse::DpdkStatusResult { stats }).await?;
            }
            IpcRequest::MigrationStartLive { plan } => {
                let resp = match crate::migration_service::MigrationService::global(supervisor.paths())
                    .start_live_migration(plan)
                {
                    Ok(p) => IpcResponse::MigrationStarted { plan: p },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::MigrationGetStatus { migration_id } => {
                let resp = match crate::migration_service::MigrationService::global(supervisor.paths())
                    .get_migration_status(migration_id.as_deref())
                {
                    Ok(plans) => IpcResponse::MigrationStatus { plans },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::MigrationAbort { migration_id, reason } => {
                let resp = match crate::migration_service::MigrationService::global(supervisor.paths())
                    .abort_migration(&migration_id, reason.as_deref())
                {
                    Ok(plan) => IpcResponse::MigrationAborted {
                        plan,
                        message: "Live migration successfully aborted".to_string(),
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::MigrationList => {
                let resp = match crate::migration_service::MigrationService::global(supervisor.paths())
                    .list_migrations()
                {
                    Ok(plans) => IpcResponse::MigrationListResult { plans },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::AnycastRouteManage { action, route } => {
                let resp = match crate::migration_service::MigrationService::global(supervisor.paths())
                    .manage_anycast_route(&action, route)
                {
                    Ok((success, message, routes)) => IpcResponse::AnycastRouteManageResult {
                        success,
                        message,
                        routes,
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::EbpfStartProfiling {
                server_name,
                probe_type,
                duration_secs,
                sample_rate_hz,
            } => {
                let resp = match crate::ebpf_service::EbpfObservabilityService::global(supervisor.paths())
                    .start_profiling(&server_name, probe_type, duration_secs, sample_rate_hz)
                {
                    Ok(descriptor) => IpcResponse::EbpfProfilingStarted { descriptor },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::EbpfGetStatus { server_name } => {
                let resp = match crate::ebpf_service::EbpfObservabilityService::global(supervisor.paths())
                    .get_status(&server_name)
                {
                    Ok((descriptor, socket_telemetry, syscall_aggregations)) => {
                        IpcResponse::EbpfStatusResult {
                            descriptor,
                            socket_telemetry,
                            syscall_aggregations,
                        }
                    }
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::EbpfGetFlameGraph { server_name, format } => {
                let resp = match crate::ebpf_service::EbpfObservabilityService::global(supervisor.paths())
                    .get_flamegraph(&server_name, &format)
                {
                    Ok((content, root_node)) => IpcResponse::EbpfFlameGraphResult {
                        server_name,
                        format,
                        content,
                        root_node,
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::EbpfGetGcTelemetry { server_name, limit } => {
                let resp = match crate::ebpf_service::EbpfObservabilityService::global(supervisor.paths())
                    .get_gc_telemetry(&server_name, limit)
                {
                    Ok(events) => IpcResponse::EbpfGcTelemetryResult { server_name, events },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::EbpfStopProfiling { server_name, probe_id } => {
                let resp = match crate::ebpf_service::EbpfObservabilityService::global(supervisor.paths())
                    .stop_profiling(&server_name, probe_id.as_deref())
                {
                    Ok(descriptor) => IpcResponse::EbpfProfilingStopped {
                        descriptor,
                        message: "Kernel probe detached successfully".to_string(),
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::SupplyChainVerify {
                artifact_path,
                attestation_path,
                strict,
            } => {
                let service = crate::supply_chain_service::SupplyChainService::global(supervisor.paths());
                let resp = match service.verify_artifact(
                    &artifact_path,
                    attestation_path.as_deref(),
                    strict,
                ) {
                    Ok(verdict) => IpcResponse::SupplyChainVerdict { verdict },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::SupplyChainGetPolicy => {
                let service = crate::supply_chain_service::SupplyChainService::global(supervisor.paths());
                let resp = match service.get_policy() {
                    Ok((policy, trust_anchors_count)) => {
                        IpcResponse::SupplyChainPolicyResult {
                            policy,
                            trust_anchors_count,
                        }
                    }
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::SupplyChainSetPolicy { policy } => {
                let service = crate::supply_chain_service::SupplyChainService::global(supervisor.paths());
                let resp = match service.set_policy(policy) {
                    Ok(()) => IpcResponse::Success {
                        message: "Supply chain policy updated successfully".to_string(),
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::SupplyChainInspectAttestation { identifier } => {
                let service = crate::supply_chain_service::SupplyChainService::global(supervisor.paths());
                let resp = match service.inspect_attestation(&identifier) {
                    Ok(attestation) => IpcResponse::SupplyChainAttestationResult { attestation },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::HermeticBuildRun {
                build_dir,
                command,
                args,
                allow_network,
            } => {
                let service = crate::supply_chain_service::SupplyChainService::global(supervisor.paths());
                let resp = match service.run_hermetic_build(&build_dir, &command, &args, allow_network).await {
                    Ok(manifest) => IpcResponse::HermeticBuildResult { manifest },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::PqcGetStatus => {
                let service = crate::pqc_service::PqcService::global(supervisor.paths());
                let resp = match service.get_status() {
                    Ok(summary) => IpcResponse::PqcStatus { summary },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::PqcSetPolicy { policy } => {
                let service = crate::pqc_service::PqcService::global(supervisor.paths());
                let resp = match service.set_policy(policy.clone()) {
                    Ok(()) => IpcResponse::PqcPolicyResult {
                        policy,
                        message: "PQC policy updated successfully".to_string(),
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::PqcGenerateKeyPair { suite, algorithm } => {
                let service = crate::pqc_service::PqcService::global(supervisor.paths());
                let resp = match service.generate_keypair(suite, algorithm) {
                    Ok(keypair) => IpcResponse::PqcKeyPairResult { keypair },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::PqcBenchmark { iterations } => {
                let service = crate::pqc_service::PqcService::global(supervisor.paths());
                let resp = match service.run_benchmark(iterations) {
                    Ok(report) => IpcResponse::PqcBenchmarkResult { report },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::PqcMigrateNode { target_phase } => {
                let service = crate::pqc_service::PqcService::global(supervisor.paths());
                let resp = match service.migrate_phase(target_phase) {
                    Ok(message) => IpcResponse::PqcMigrationResult {
                        new_phase: target_phase,
                        message,
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::HsmGetStatus => {
                let service = crate::hsm_service::HsmService::global(supervisor.paths());
                let resp = match service.get_status() {
                    Ok(summary) => IpcResponse::HsmStatus { summary },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::HsmGenerateKey { label, key_type } => {
                let service = crate::hsm_service::HsmService::global(supervisor.paths());
                let resp = match service.generate_key(&label, key_type) {
                    Ok(key) => IpcResponse::HsmKeyResult { key },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::HsmSign { label, data } => {
                let service = crate::hsm_service::HsmService::global(supervisor.paths());
                let resp = match service.sign_data(&label, &data) {
                    Ok(signature) => IpcResponse::HsmSignResult { signature },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::HsmAttest { nonce, pcr_mask } => {
                let service = crate::hsm_service::HsmService::global(supervisor.paths());
                let resp = match service.attest_enclave(pcr_mask, nonce) {
                    Ok(quote) => IpcResponse::HsmAttestResult { quote },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::HsmZkProve { cluster_id } => {
                let service = crate::hsm_service::HsmService::global(supervisor.paths());
                let resp = match service.prove_zk_membership(&cluster_id) {
                    Ok(proof) => IpcResponse::HsmZkProveResult { proof },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::HsmZkVerify { cluster_id: _, proof } => {
                let service = crate::hsm_service::HsmService::global(supervisor.paths());
                let resp = match service.verify_zk_membership(&proof) {
                    Ok(valid) => IpcResponse::HsmZkVerifyResult {
                        valid,
                        message: if valid {
                            "Zero-knowledge cluster membership proof verified successfully".to_string()
                        } else {
                            "Zero-knowledge cluster membership proof verification failed".to_string()
                        },
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::CompactionGetStatus => {
                let service = crate::compaction_service::CompactionService::global(supervisor.paths());
                let resp = match service.get_status() {
                    Ok(summary) => IpcResponse::CompactionStatusResult { summary },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::CompactionTriggerNow => {
                let service = crate::compaction_service::CompactionService::global(supervisor.paths());
                let resp = match service.trigger_compaction() {
                    Ok(result) => IpcResponse::CompactionCycleResult { result },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::CompactionConfigureThp { mode, defrag } => {
                let service = crate::compaction_service::CompactionService::global(supervisor.paths());
                let resp = match service.configure_thp(mode, defrag) {
                    Ok(status) => IpcResponse::CompactionThpConfigured {
                        status,
                        message: format!("Transparent hugepages configured to mode='{}', defrag='{}'", mode, defrag),
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::CompactionGetPoolStats => {
                let service = crate::compaction_service::CompactionService::global(supervisor.paths());
                let resp = match service.get_pool_stats() {
                    Ok(stats) => IpcResponse::CompactionPoolStatsResult { stats },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::CompactionResetMetrics => {
                let service = crate::compaction_service::CompactionService::global(supervisor.paths());
                let resp = match service.reset_metrics() {
                    Ok(()) => IpcResponse::CompactionMetricsResetResult {
                        message: "Compaction and page pool metrics reset successfully".to_string(),
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::XdpGetStatus => {
                let service = crate::xdp_service::XdpService::global(supervisor.paths());
                let resp = match service.get_status() {
                    Ok(status) => IpcResponse::XdpStatusResult { status },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::XdpAttachInterface { interface_name, mode } => {
                let service = crate::xdp_service::XdpService::global(supervisor.paths());
                let resp = match service.attach_interface(&interface_name, mode) {
                    Ok(status) => IpcResponse::XdpStatusResult { status },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::XdpDetachInterface => {
                let service = crate::xdp_service::XdpService::global(supervisor.paths());
                let resp = match service.detach_interface() {
                    Ok(status) => IpcResponse::XdpStatusResult { status },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::XdpAddRule { rule } => {
                let service = crate::xdp_service::XdpService::global(supervisor.paths());
                let resp = match service.add_rule(rule) {
                    Ok(status) => IpcResponse::XdpRuleModified {
                        status,
                        message: "XDP filter rule added successfully".to_string(),
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::XdpRemoveRule { rule_id } => {
                let service = crate::xdp_service::XdpService::global(supervisor.paths());
                let resp = match service.remove_rule(&rule_id) {
                    Ok(status) => IpcResponse::XdpRuleModified {
                        status,
                        message: format!("XDP filter rule '{}' removed successfully", rule_id),
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::XdpResetMetrics => {
                let service = crate::xdp_service::XdpService::global(supervisor.paths());
                let resp = match service.reset_metrics() {
                    Ok(_) => IpcResponse::XdpMetricsResetResult {
                        message: "XDP metrics reset successfully".to_string(),
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::PmuGetStatus => {
                let service = crate::pmu_service::PmuService::global(supervisor.paths());
                let resp = match service.get_status() {
                    Ok(summary) => IpcResponse::PmuStatusResult { summary },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::PmuStartSampling { target_pid, sample_rate_hz } => {
                let service = crate::pmu_service::PmuService::global(supervisor.paths());
                let resp = match service.start_sampling(target_pid, sample_rate_hz) {
                    Ok(summary) => IpcResponse::PmuStatusResult { summary },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::PmuStopSampling => {
                let service = crate::pmu_service::PmuService::global(supervisor.paths());
                let resp = match service.stop_sampling() {
                    Ok(summary) => IpcResponse::PmuStatusResult { summary },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::PmuSampleNow { target_pid } => {
                let service = crate::pmu_service::PmuService::global(supervisor.paths());
                let resp = match service.sample_now(target_pid) {
                    Ok(sample) => IpcResponse::PmuSampleResult { sample },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::PmuGetHotspots { limit } => {
                let service = crate::pmu_service::PmuService::global(supervisor.paths());
                let resp = match service.get_hotspots(limit) {
                    Ok(hotspots) => IpcResponse::PmuHotspotsResult { hotspots },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::PmuRunBench { iterations } => {
                let service = crate::pmu_service::PmuService::global(supervisor.paths());
                let resp = match service.run_bench(iterations) {
                    Ok(report) => IpcResponse::PmuBenchResult { report },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::PmuResetMetrics => {
                let service = crate::pmu_service::PmuService::global(supervisor.paths());
                let resp = match service.reset_metrics() {
                    Ok(_) => IpcResponse::PmuMetricsResetResult {
                        message: "PMU metrics reset successfully".to_string(),
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::ShmGetStatus { server } => {
                let service = crate::shm_service::ShmService::global(supervisor.paths());
                let resp = match service.get_status(server.as_deref()) {
                    Ok(summary) => IpcResponse::ShmStatusResult { summary },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::ShmCreateChannel { server, channel, slot_size, slot_count } => {
                let service = crate::shm_service::ShmService::global(supervisor.paths());
                let resp = match service.create_channel(&server, &channel, slot_size, slot_count) {
                    Ok(meta) => IpcResponse::ShmChannelCreatedResult { meta },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::ShmCloseChannel { server, channel } => {
                let service = crate::shm_service::ShmService::global(supervisor.paths());
                let resp = match service.close_channel(&server, &channel) {
                    Ok(closed) => IpcResponse::ShmChannelClosedResult { closed },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::ShmWriteEvent { server, channel, payload } => {
                let service = crate::shm_service::ShmService::global(supervisor.paths());
                let resp = match service.write_event(&server, &channel, &payload) {
                    Ok(sequence) => IpcResponse::ShmEventWrittenResult { sequence },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::ShmReadEvents { server, channel, limit } => {
                let service = crate::shm_service::ShmService::global(supervisor.paths());
                let resp = match service.read_events(&server, &channel, limit) {
                    Ok(events) => IpcResponse::ShmEventsReadResult { events },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::ShmRunBench { message_count, payload_size } => {
                let service = crate::shm_service::ShmService::global(supervisor.paths());
                let resp = match service.run_bench(message_count, payload_size) {
                    Ok(metrics) => IpcResponse::ShmBenchResult { metrics },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::ShmResetMetrics { server } => {
                let service = crate::shm_service::ShmService::global(supervisor.paths());
                let resp = match service.reset_metrics(server.as_deref()) {
                    Ok(_) => IpcResponse::ShmMetricsResetResult {
                        message: "SHM metrics reset successfully".to_string(),
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::PatchGetStatus { server } => {
                let service = crate::patch_service::DynamicPatchService::global(supervisor.paths());
                let resp = match service.get_status(server.as_deref()) {
                    Ok(summary) => IpcResponse::PatchStatusResult { summary },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::PatchApply {
                server,
                patch_name,
                target_symbol,
                shadow_bytes_hex,
            } => {
                let service = crate::patch_service::DynamicPatchService::global(supervisor.paths());
                let resp = match service.apply_patch(
                    &server,
                    &patch_name,
                    &target_symbol,
                    &shadow_bytes_hex,
                ) {
                    Ok(manifest) => IpcResponse::PatchAppliedResult { manifest },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::PatchRollback { server, patch_name } => {
                let service = crate::patch_service::DynamicPatchService::global(supervisor.paths());
                let resp = match service.rollback_patch(&server, &patch_name) {
                    Ok(rolled_back) => IpcResponse::PatchRolledBackResult { rolled_back },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::PatchGetDiff { server, patch_name } => {
                let service = crate::patch_service::DynamicPatchService::global(supervisor.paths());
                let resp = match service.get_diff(&server, &patch_name) {
                    Ok(diff) => IpcResponse::PatchDiffResult { diff },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::PatchRunBench { iterations } => {
                let service = crate::patch_service::DynamicPatchService::global(supervisor.paths());
                let resp = match service.run_bench(iterations) {
                    Ok(metrics) => IpcResponse::PatchBenchResult { metrics },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::PatchResetMetrics { server } => {
                let service = crate::patch_service::DynamicPatchService::global(supervisor.paths());
                let resp = match service.reset_metrics(server.as_deref()) {
                    Ok(_) => IpcResponse::PatchMetricsResetResult {
                        message: "Dynamic patch metrics reset successfully".to_string(),
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::VmGetStatus { vm_id } => {
                let service = crate::vm_service::MicroVmService::global(supervisor.paths());
                let resp = match service.get_status(vm_id.as_deref()) {
                    Ok(summary) => IpcResponse::VmStatusResult { summary },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::VmSpawn {
                name,
                vcpus,
                memory_mb,
                vsock_cid,
                virtio_devices,
            } => {
                let service = crate::vm_service::MicroVmService::global(supervisor.paths());
                let resp = match service.spawn_vm(&name, vcpus, memory_mb, vsock_cid, virtio_devices) {
                    Ok(descriptor) => IpcResponse::VmSpawnResult { descriptor },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::VmStop { vm_id, force } => {
                let service = crate::vm_service::MicroVmService::global(supervisor.paths());
                let resp = match service.stop_vm(&vm_id, force) {
                    Ok(stopped) => IpcResponse::VmStopResult { stopped },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::VmInspect { vm_id } => {
                let service = crate::vm_service::MicroVmService::global(supervisor.paths());
                let resp = match service.inspect_vm(&vm_id) {
                    Ok(descriptor) => IpcResponse::VmInspectResult { descriptor },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::VmRunBench {
                concurrency,
                iterations,
            } => {
                let service = crate::vm_service::MicroVmService::global(supervisor.paths());
                let resp = match service.run_bench(concurrency, iterations) {
                    Ok(metrics) => IpcResponse::VmBenchResult { metrics },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::VmResetMetrics { vm_id } => {
                let service = crate::vm_service::MicroVmService::global(supervisor.paths());
                let resp = match service.reset_metrics(vm_id.as_deref()) {
                    Ok(_) => IpcResponse::VmResetMetricsResult {
                        message: "MicroVM metrics reset successfully".to_string(),
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::CrashGetStatus { server } => {
                let service = crate::crash_service::CrashTriageService::global(supervisor.paths());
                let resp = match service.get_status(server.as_deref()) {
                    Ok(summary) => IpcResponse::CrashStatusResult { summary },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::CrashTriageFile {
                server,
                file_path,
            } => {
                let service = crate::crash_service::CrashTriageService::global(supervisor.paths());
                let resp = match service.triage_file(
                    server.as_deref(),
                    &file_path,
                ) {
                    Ok(report) => IpcResponse::CrashTriageReportResult { report },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::CrashListReports { server, limit } => {
                let service = crate::crash_service::CrashTriageService::global(supervisor.paths());
                let resp = match service.list_reports(server.as_deref(), limit) {
                    Ok(reports) => IpcResponse::CrashReportsListResult { reports },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::CrashGetReport { report_id } => {
                let service = crate::crash_service::CrashTriageService::global(supervisor.paths());
                let resp = match service.get_report(&report_id) {
                    Ok(Some(report)) => IpcResponse::CrashTriageReportResult { report },
                    Ok(None) => IpcResponse::Error {
                        error: format!("Crash report not found: {}", report_id),
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::CrashRunBench { iterations } => {
                let service = crate::crash_service::CrashTriageService::global(supervisor.paths());
                let resp = match service.run_bench(iterations) {
                    Ok(metrics) => IpcResponse::CrashBenchResult { metrics },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::CrashResetMetrics { server } => {
                let service = crate::crash_service::CrashTriageService::global(supervisor.paths());
                let resp = match service.reset_metrics(server.as_deref()) {
                    Ok(_) => IpcResponse::CrashMetricsResetResult {
                        message: "Crash triage metrics reset successfully".to_string(),
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::RdmaGetStatus { server } => {
                let service = crate::rdma_service::RdmaService::global(supervisor.paths());
                let resp = match service.get_status(server.as_deref()) {
                    Ok(summary) => IpcResponse::RdmaStatusResult { summary },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::RdmaRegisterMr { server, size, read_only } => {
                let service = crate::rdma_service::RdmaService::global(supervisor.paths());
                let resp = match service.register_mr(server.as_deref(), size, read_only) {
                    Ok(mr) => IpcResponse::RdmaMrResult { mr },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::RdmaConnectPeer { server, peer_address, qp_num } => {
                let service = crate::rdma_service::RdmaService::global(supervisor.paths());
                let resp = match service.connect_peer(server.as_deref(), &peer_address, qp_num) {
                    Ok(peer) => IpcResponse::RdmaPeerResult { peer },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::RdmaListPeers { server } => {
                let service = crate::rdma_service::RdmaService::global(supervisor.paths());
                let resp = match service.list_peers(server.as_deref()) {
                    Ok(peers) => IpcResponse::RdmaPeersListResult { peers },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::RdmaRunBench { iterations, buffer_size } => {
                let service = crate::rdma_service::RdmaService::global(supervisor.paths());
                let resp = match service.run_bench(iterations, buffer_size) {
                    Ok(metrics) => IpcResponse::RdmaBenchResult { metrics },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::RdmaResetMetrics { server } => {
                let service = crate::rdma_service::RdmaService::global(supervisor.paths());
                let resp = match service.reset_metrics(server.as_deref()) {
                    Ok(_) => IpcResponse::RdmaMetricsResetResult {
                        message: "RDMA fabric metrics reset successfully".to_string(),
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::SmartNicGetStatus { server } => {
                let service = crate::smartnic_service::SmartNicService::global(supervisor.paths());
                let resp = match service.get_status(server.as_deref()) {
                    Ok((summary, devices)) => IpcResponse::SmartNicStatusResult { summary, devices },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::SmartNicInstallRule { server: _, rule } => {
                let service = crate::smartnic_service::SmartNicService::global(supervisor.paths());
                let resp = match service.install_rule(rule) {
                    Ok(rule) => IpcResponse::SmartNicRuleResult { rule },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::SmartNicRemoveRule { server: _, rule_id } => {
                let service = crate::smartnic_service::SmartNicService::global(supervisor.paths());
                let resp = match service.remove_rule(&rule_id) {
                    Ok(success) => IpcResponse::SmartNicRemoveResult { success },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::SmartNicListRules { server } => {
                let service = crate::smartnic_service::SmartNicService::global(supervisor.paths());
                let resp = match service.list_rules(server.as_deref()) {
                    Ok(rules) => IpcResponse::SmartNicRulesListResult { rules },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::SmartNicRunBench { iterations, packet_size } => {
                let service = crate::smartnic_service::SmartNicService::global(supervisor.paths());
                let resp = match service.run_bench(iterations, packet_size) {
                    Ok(metrics) => IpcResponse::SmartNicBenchResult { metrics },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::SmartNicResetMetrics { server } => {
                let service = crate::smartnic_service::SmartNicService::global(supervisor.paths());
                let resp = match service.reset_metrics(server.as_deref()) {
                    Ok(message) => IpcResponse::SmartNicResetResult { message },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::MemFabricGetStatus { server } => {
                let service = crate::memfabric_service::MemFabricService::global(supervisor.paths());
                let resp = match service.get_status(server.as_deref()) {
                    Ok((summary, nodes)) => IpcResponse::MemFabricStatusResult { summary, nodes },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::MemFabricAllocatePage {
                server: _,
                page_id,
                size,
                tier,
                dimension,
            } => {
                let service = crate::memfabric_service::MemFabricService::global(supervisor.paths());
                let resp = match service.allocate_page(page_id, size, tier, dimension) {
                    Ok(page) => IpcResponse::MemFabricPageResult { page },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::MemFabricEvictDimension {
                server: _,
                dimension,
                target_node,
            } => {
                let service = crate::memfabric_service::MemFabricService::global(supervisor.paths());
                let resp = match service.evict_dimension(&dimension, target_node.as_deref()) {
                    Ok((pages_evicted, bytes_freed)) => IpcResponse::MemFabricEvictResult {
                        pages_evicted,
                        bytes_freed,
                        dimension,
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::MemFabricListPages { server } => {
                let service = crate::memfabric_service::MemFabricService::global(supervisor.paths());
                let resp = match service.list_pages(server.as_deref()) {
                    Ok(pages) => IpcResponse::MemFabricPagesListResult { pages },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::MemFabricRunBench {
                iterations,
                page_size,
            } => {
                let service = crate::memfabric_service::MemFabricService::global(supervisor.paths());
                let resp = match service.run_bench(iterations, page_size) {
                    Ok(metrics) => IpcResponse::MemFabricBenchResult { metrics },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::MemFabricResetMetrics { server } => {
                let service = crate::memfabric_service::MemFabricService::global(supervisor.paths());
                let resp = match service.reset_metrics(server.as_deref()) {
                    Ok(message) => IpcResponse::MemFabricResetResult { message },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::NvmeGetStatus { server } => {
                let service = crate::nvme_service::NvmeTargetService::global(supervisor.paths());
                let resp = match service.get_status(server.as_deref()) {
                    Ok((summary, subsystems)) => IpcResponse::NvmeStatusResult { summary, subsystems },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::NvmeCreateNamespace {
                nsid,
                size_mb,
                block_size,
                server_id,
                dimension,
            } => {
                let service = crate::nvme_service::NvmeTargetService::global(supervisor.paths());
                let resp = match service.create_namespace(nsid, size_mb, block_size, server_id, dimension) {
                    Ok(namespace) => IpcResponse::NvmeNamespaceResult { namespace },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::NvmeDeleteNamespace { nsid } => {
                let service = crate::nvme_service::NvmeTargetService::global(supervisor.paths());
                let resp = match service.delete_namespace(nsid) {
                    Ok(success) => IpcResponse::NvmeDeleteResult {
                        nsid,
                        success,
                        message: format!("Namespace ID {} deleted successfully", nsid),
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::NvmeListNamespaces { server } => {
                let service = crate::nvme_service::NvmeTargetService::global(supervisor.paths());
                let resp = match service.list_namespaces(server.as_deref()) {
                    Ok(namespaces) => IpcResponse::NvmeNamespacesListResult { namespaces },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::NvmeListSubsystems => {
                let service = crate::nvme_service::NvmeTargetService::global(supervisor.paths());
                let resp = match service.list_subsystems() {
                    Ok(subsystems) => IpcResponse::NvmeSubsystemsListResult { subsystems },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::NvmeRunBench {
                block_size,
                iterations,
            } => {
                let service = crate::nvme_service::NvmeTargetService::global(supervisor.paths());
                let resp = match service.run_bench(block_size, iterations) {
                    Ok(metrics) => IpcResponse::NvmeBenchResult { metrics },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::NvmeResetMetrics => {
                let service = crate::nvme_service::NvmeTargetService::global(supervisor.paths());
                let resp = match service.reset_metrics(None) {
                    Ok(message) => IpcResponse::NvmeResetResult { message },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::VpnGetStatus { server } => {
                let service = crate::vpn_service::VpnMeshService::global(supervisor.paths());
                let resp = match service.get_status(server.as_deref()) {
                    Ok((summary, tunnels)) => IpcResponse::VpnStatusResult { summary, tunnels },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::VpnCreateTunnel { tunnel_id, address, port, crypto_mode } => {
                let service = crate::vpn_service::VpnMeshService::global(supervisor.paths());
                let resp = match service.create_tunnel(&tunnel_id, &address, port, crypto_mode.as_deref()) {
                    Ok(tunnel) => IpcResponse::VpnTunnelCreatedResult { tunnel },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::VpnDeleteTunnel { tunnel_id } => {
                let service = crate::vpn_service::VpnMeshService::global(supervisor.paths());
                let resp = match service.delete_tunnel(&tunnel_id) {
                    Ok(success) => IpcResponse::VpnTunnelDeletedResult { tunnel_id, success },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::VpnAddPeer { tunnel_id, peer_id, endpoint, allowed_ips } => {
                let service = crate::vpn_service::VpnMeshService::global(supervisor.paths());
                let resp = match service.add_peer(&tunnel_id, &peer_id, &endpoint, allowed_ips) {
                    Ok(peer) => IpcResponse::VpnPeerAddedResult { tunnel_id, peer },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::VpnRemovePeer { tunnel_id, peer_id } => {
                let service = crate::vpn_service::VpnMeshService::global(supervisor.paths());
                let resp = match service.remove_peer(&tunnel_id, &peer_id) {
                    Ok(success) => IpcResponse::VpnPeerRemovedResult { tunnel_id, peer_id, success },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::VpnRotateKey { tunnel_id, peer_id } => {
                let service = crate::vpn_service::VpnMeshService::global(supervisor.paths());
                let resp = match service.rotate_key(&tunnel_id, peer_id.as_deref()) {
                    Ok(renegotiation_latency_micros) => IpcResponse::VpnKeyRotatedResult {
                        tunnel_id,
                        renegotiation_latency_micros,
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::VpnRunBench { iterations, packet_size } => {
                let service = crate::vpn_service::VpnMeshService::global(supervisor.paths());
                let resp = match service.run_bench(iterations, packet_size) {
                    Ok(metrics) => IpcResponse::VpnBenchResult { metrics },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::VpnResetMetrics => {
                let service = crate::vpn_service::VpnMeshService::global(supervisor.paths());
                let resp = match service.reset_metrics() {
                    Ok(_) => IpcResponse::VpnResetResult {
                        message: "VPN mesh telemetry metrics reset successfully".to_string(),
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::BftGetStatus { server } => {
                let service = crate::bft_service::BftConsensusService::global(supervisor.paths());
                let resp = match service.get_status(server.as_deref()) {
                    Ok(summary) => IpcResponse::BftStatusResult { summary },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::BftSubmitTransaction { tx_type, payload, sender } => {
                let service = crate::bft_service::BftConsensusService::global(supervisor.paths());
                let resp = match service.submit_transaction(&tx_type, &payload, sender.as_deref()) {
                    Ok((tx_id, view)) => IpcResponse::BftTransactionSubmittedResult {
                        tx_id,
                        view,
                        status: "COMMITTED".to_string(),
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::BftListValidators { server } => {
                let service = crate::bft_service::BftConsensusService::global(supervisor.paths());
                let resp = match service.list_validators(server.as_deref()) {
                    Ok(validators) => IpcResponse::BftValidatorsListResult { validators },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::BftAddValidator { node_id, address, public_key, stake } => {
                let service = crate::bft_service::BftConsensusService::global(supervisor.paths());
                let resp = match service.add_validator(&node_id, &address, &public_key, stake) {
                    Ok(()) => IpcResponse::BftValidatorAddedResult {
                        node_id: node_id.clone(),
                        message: format!("Validator '{}' successfully added to BFT quorum", node_id),
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::BftRemoveValidator { node_id } => {
                let service = crate::bft_service::BftConsensusService::global(supervisor.paths());
                let resp = match service.remove_validator(&node_id) {
                    Ok(()) => IpcResponse::BftValidatorRemovedResult {
                        node_id: node_id.clone(),
                        message: format!("Validator '{}' successfully removed from BFT quorum", node_id),
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::BftTriggerViewChange { reason } => {
                let service = crate::bft_service::BftConsensusService::global(supervisor.paths());
                let resp = match service.trigger_view_change(&reason) {
                    Ok((from_view, to_view, new_proposer)) => IpcResponse::BftViewChangedResult {
                        from_view,
                        to_view,
                        new_proposer,
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::BftVerifyZkProof { proof_json } => {
                let service = crate::bft_service::BftConsensusService::global(supervisor.paths());
                let resp = match service.verify_zk_proof(&proof_json) {
                    Ok(valid) => IpcResponse::BftZkProofVerifiedResult {
                        valid,
                        message: if valid {
                            "Zero-knowledge state transition proof verified successfully".to_string()
                        } else {
                            "Zero-knowledge state transition proof verification failed".to_string()
                        },
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::BftRunBench { iterations, validators } => {
                let service = crate::bft_service::BftConsensusService::global(supervisor.paths());
                let resp = match service.run_bench(iterations, validators) {
                    Ok(metrics) => IpcResponse::BftBenchResult { metrics },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::BftResetMetrics => {
                let service = crate::bft_service::BftConsensusService::global(supervisor.paths());
                let resp = match service.reset_metrics() {
                    Ok(_) => IpcResponse::BftResetResult {
                        message: "BFT cluster consensus metrics reset successfully".to_string(),
                    },
                    Err(e) => IpcResponse::Error { error: e.to_string() },
                };
                write_frame(&mut stream, &resp).await?;
            }
            IpcRequest::ShutdownDaemon => {
                write_frame(
                    &mut stream,
                    &IpcResponse::Success {
                        message: "Shutting down daemon".to_string(),
                    },
                )
                .await?;
                std::process::exit(0);
            }
        }
    }

    Ok(())
}

/// Generic length-delimited JSON frame reader
pub async fn read_frame<R, T>(reader: &mut R) -> Result<Option<T>>
where
    R: AsyncRead + Unpin,
    T: serde::de::DeserializeOwned,
{
    let mut len_bytes = [0u8; 4];
    match reader.read_exact(&mut len_bytes).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(CraftError::Io(e)),
    }

    let length = u32::from_be_bytes(len_bytes) as usize;
    if length > 16 * 1024 * 1024 {
        return Err(CraftError::Ipc(
            "Frame length exceeds 16MB threshold".to_string(),
        ));
    }

    let mut buf = vec![0u8; length];
    reader.read_exact(&mut buf).await?;

    let parsed: T = serde_json::from_slice(&buf)?;
    Ok(Some(parsed))
}

/// Generic length-delimited JSON frame writer
pub async fn write_frame<W, T>(writer: &mut W, value: &T) -> Result<()>
where
    W: AsyncWrite + Unpin,
    T: serde::Serialize,
{
    let bytes = serde_json::to_vec(value)?;
    let length = bytes.len() as u32;
    writer.write_all(&length.to_be_bytes()).await?;
    writer.write_all(&bytes).await?;
    writer.flush().await?;
    Ok(())
}

pub struct DaemonClient {
    #[cfg(not(target_os = "windows"))]
    stream: tokio::net::UnixStream,
    #[cfg(target_os = "windows")]
    stream: tokio::net::windows::named_pipe::NamedPipeClient,
}

impl DaemonClient {
    pub async fn connect(paths: &CraftPaths) -> Result<Self> {
        #[cfg(not(target_os = "windows"))]
        {
            let stream = tokio::net::UnixStream::connect(&paths.socket_file)
                .await
                .map_err(|e| {
                    CraftError::Ipc(format!("Could not connect to daemon socket: {}", e))
                })?;
            Ok(Self { stream })
        }

        #[cfg(target_os = "windows")]
        {
            use tokio::net::windows::named_pipe::ClientOptions;
            let pipe_name = r"\\.\pipe\craft-daemon";
            let client = ClientOptions::new().open(pipe_name).map_err(|e| {
                CraftError::Ipc(format!(
                    "Could not connect to daemon named pipe {}: {}",
                    pipe_name, e
                ))
            })?;
            Ok(Self { stream: client })
        }
    }

    pub async fn request(&mut self, req: IpcRequest) -> Result<IpcResponse> {
        write_frame(&mut self.stream, &req).await?;
        match read_frame(&mut self.stream).await? {
            Some(resp) => Ok(resp),
            None => Err(CraftError::Ipc(
                "Daemon closed connection unexpectedly".to_string(),
            )),
        }
    }

    pub async fn get_running(&mut self) -> Result<Vec<PathBuf>> {
        match self.request(IpcRequest::GetRunning).await? {
            IpcResponse::RunningList { paths } => Ok(paths),
            IpcResponse::Error { error } => Err(CraftError::Ipc(error)),
            _ => Err(CraftError::Ipc(
                "Unexpected response from daemon".to_string(),
            )),
        }
    }

    pub async fn start_server(&mut self, path: &Path) -> Result<()> {
        match self
            .request(IpcRequest::StartServer {
                path: path.to_path_buf(),
            })
            .await?
        {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::AlreadyRunning { .. } => Err(CraftError::Other(format!(
                "Server '{}' is already running",
                path.display()
            ))),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc(
                "Unexpected response from daemon".to_string(),
            )),
        }
    }

    pub async fn stop_server(&mut self, path: &Path, force: bool) -> Result<()> {
        match self
            .request(IpcRequest::StopServer {
                path: path.to_path_buf(),
                force,
            })
            .await?
        {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc(
                "Unexpected response from daemon".to_string(),
            )),
        }
    }

    pub async fn attach_console_stream(
        mut self,
        path: &Path,
    ) -> Result<(
        String,
        tokio::sync::mpsc::Sender<String>,
        tokio::sync::mpsc::Receiver<String>,
    )> {
        write_frame(
            &mut self.stream,
            &IpcRequest::AttachConsole {
                path: path.to_path_buf(),
            },
        )
        .await?;

        // Read initial response (should be LogBacklog)
        let initial_data = match read_frame::<_, IpcResponse>(&mut self.stream).await? {
            Some(IpcResponse::LogBacklog { data, .. }) => data,
            Some(IpcResponse::Error { error }) => return Err(CraftError::Other(error)),
            _ => {
                return Err(CraftError::Ipc(
                    "Expected log backlog from daemon".to_string(),
                ))
            }
        };

        let (mut reader, mut writer) = tokio::io::split(self.stream);
        let path_clone = path.to_path_buf();

        let (tx_to_daemon, mut rx_from_client) = tokio::sync::mpsc::channel::<String>(64);
        let (tx_to_client, rx_from_daemon) = tokio::sync::mpsc::channel::<String>(256);

        tokio::spawn(async move {
            while let Ok(Some(resp)) = read_frame::<_, IpcResponse>(&mut reader).await {
                if let IpcResponse::LogChunk { data, .. } = resp {
                    if tx_to_client.send(data).await.is_err() {
                        break;
                    }
                }
            }
        });

        tokio::spawn(async move {
            while let Some(line) = rx_from_client.recv().await {
                let input = if line.ends_with('\n') {
                    line
                } else {
                    format!("{}\n", line)
                };
                let req = IpcRequest::SendInput {
                    path: path_clone.clone(),
                    input,
                };
                if write_frame(&mut writer, &req).await.is_err() {
                    break;
                }
            }
        });

        Ok((initial_data, tx_to_daemon, rx_from_daemon))
    }

    pub async fn attach_console(self, path: &Path) -> Result<()> {
        let (backlog, tx_to_daemon, mut rx_from_daemon) = self.attach_console_stream(path).await?;
        print!("{}", backlog);
        let _ = std::io::Write::flush(&mut std::io::stdout());

        println!("\x1b[36m--- Attached to server console (Type 'stop' or commands, press Ctrl+C to exit) ---\x1b[0m");

        let rx_task = tokio::spawn(async move {
            while let Some(data) = rx_from_daemon.recv().await {
                print!("{}", data);
                let _ = std::io::Write::flush(&mut std::io::stdout());
            }
        });

        let tx_task = tokio::spawn(async move {
            let mut stdin = tokio::io::BufReader::new(tokio::io::stdin()).lines();
            while let Ok(Some(line)) = stdin.next_line().await {
                if tx_to_daemon.send(line).await.is_err() {
                    break;
                }
            }
        });

        tokio::select! {
            _ = rx_task => {},
            _ = tx_task => {},
            _ = tokio::signal::ctrl_c() => {
                println!("\r\n\x1b[33m[Craft] Detached from server console.\x1b[0m");
            }
        }

        Ok(())
    }

    pub fn is_daemon_running(paths: &CraftPaths) -> bool {
        if let Some(pid) = read_pid_file(&paths.pid_file) {
            is_process_running(pid)
        } else {
            false
        }
    }

    pub async fn ensure_daemon_started(paths: &CraftPaths) -> Result<()> {
        if Self::is_daemon_running(paths) {
            return Ok(());
        }

        info!("Starting Craft background service daemon...");
        let current_exe = std::env::current_exe()?;

        let mut cmd = std::process::Command::new(current_exe);
        cmd.arg("service").arg("start").arg("--foreground");

        #[cfg(target_os = "windows")]
        {
            use std::os::windows::process::CommandExt;
            const CREATE_NO_WINDOW: u32 = 0x08000000;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }

        #[cfg(not(target_os = "windows"))]
        {
            use std::os::unix::process::CommandExt;
            cmd.process_group(0);
        }

        cmd.stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null());

        let _ = cmd.spawn()?;

        // Wait up to 3 seconds for daemon socket to become ready
        for _ in 0..30 {
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
            if Self::is_daemon_running(paths) {
                return Ok(());
            }
        }

        Ok(())
    }

    pub async fn get_circuit_breakers(
        &mut self,
    ) -> Result<Vec<crate::circuit_breaker::CircuitBreakerInfo>> {
        match self.request(IpcRequest::GetCircuitBreakers).await? {
            IpcResponse::CircuitBreakersList { items } => Ok(items),
            IpcResponse::Error { error } => Err(CraftError::Ipc(error)),
            _ => Err(CraftError::Ipc(
                "Unexpected response from daemon".to_string(),
            )),
        }
    }

    pub async fn reset_circuit_breaker(&mut self, path: &Path) -> Result<()> {
        match self
            .request(IpcRequest::ResetCircuitBreaker {
                path: path.to_path_buf(),
            })
            .await?
        {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc(
                "Unexpected response from daemon".to_string(),
            )),
        }
    }

    pub async fn get_backup_schedules(
        &mut self,
    ) -> Result<Vec<crate::scheduler::BackupScheduleInfo>> {
        match self.request(IpcRequest::GetBackupSchedules).await? {
            IpcResponse::BackupSchedulesList { items } => Ok(items),
            IpcResponse::Error { error } => Err(CraftError::Ipc(error)),
            _ => Err(CraftError::Ipc(
                "Unexpected response from daemon".to_string(),
            )),
        }
    }

    pub async fn hibernate_server(&mut self, server_name: &str) -> Result<String> {
        match self
            .request(IpcRequest::HibernateServer {
                server_name: server_name.to_string(),
            })
            .await?
        {
            IpcResponse::Success { message } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn wake_server(&mut self, server_name: &str) -> Result<String> {
        match self
            .request(IpcRequest::WakeServer {
                server_name: server_name.to_string(),
            })
            .await?
        {
            IpcResponse::Success { message } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_autoscale_status(
        &mut self,
    ) -> Result<Vec<crate::protocol::AutoscaleServerStatus>> {
        match self.request(IpcRequest::GetAutoscaleStatus).await? {
            IpcResponse::AutoscaleStatusList { items } => Ok(items),
            IpcResponse::Error { error } => Err(CraftError::Ipc(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_intelligence_status(
        &mut self,
        server: Option<String>,
    ) -> Result<Vec<craft_core::DiagnosticReport>> {
        match self.request(IpcRequest::GetIntelligenceStatus { server }).await? {
            IpcResponse::IntelligenceReports { items } => Ok(items),
            IpcResponse::Error { error } => Err(CraftError::Ipc(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn trigger_diagnostic_run(
        &mut self,
        server: String,
        duration_secs: u64,
    ) -> Result<(craft_core::DiagnosticReport, String)> {
        match self.request(IpcRequest::TriggerDiagnosticRun { server, duration_secs }).await? {
            IpcResponse::DiagnosticRunCompleted { report, markdown } => Ok((report, markdown)),
            IpcResponse::Error { error } => Err(CraftError::Ipc(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn execute_remediation(
        &mut self,
        server: String,
        action: craft_core::RemediationAction,
        dry_run: bool,
    ) -> Result<String> {
        match self.request(IpcRequest::ExecuteRemediation { server, action, dry_run }).await? {
            IpcResponse::RemediationResult { message } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn update_intelligence_policy(
        &mut self,
        server: String,
        policy: craft_core::IntelligencePolicy,
    ) -> Result<()> {
        match self.request(IpcRequest::UpdateIntelligencePolicy { server, policy }).await? {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_edge_mesh_status(
        &mut self,
    ) -> Result<(Vec<craft_core::EdgeNode>, Vec<craft_core::BackboneCondition>)> {
        match self.request(IpcRequest::GetEdgeMeshStatus).await? {
            IpcResponse::EdgeMeshStatus { nodes, backbone } => Ok((nodes, backbone)),
            IpcResponse::Error { error } => Err(CraftError::Ipc(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn register_edge_node(&mut self, node: craft_core::EdgeNode) -> Result<()> {
        match self.request(IpcRequest::RegisterEdgeNode { node }).await? {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn remove_edge_node(&mut self, name: String) -> Result<()> {
        match self.request(IpcRequest::RemoveEdgeNode { name }).await? {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn trigger_edge_handoff(
        &mut self,
        handoff: craft_core::PlayerSessionHandoff,
    ) -> Result<String> {
        match self.request(IpcRequest::TriggerEdgeHandoff { handoff }).await? {
            IpcResponse::EdgeHandoffResult { success: true, message, .. } => Ok(message),
            IpcResponse::EdgeHandoffResult { success: false, message, .. } => Err(CraftError::Other(message)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn consume_edge_handoff(
        &mut self,
        token: String,
    ) -> Result<craft_core::PlayerSessionHandoff> {
        match self.request(IpcRequest::ConsumeEdgeHandoff { token }).await? {
            IpcResponse::EdgeHandoffResult { success: true, handoff: Some(h), .. } => Ok(h),
            IpcResponse::EdgeHandoffResult { message, .. } => Err(CraftError::Other(message)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn broadcast_edge_chat(
        &mut self,
        envelope: craft_core::CrossRegionChatEnvelope,
    ) -> Result<usize> {
        match self.request(IpcRequest::BroadcastEdgeChat { envelope }).await? {
            IpcResponse::EdgeChatBroadcastResult { delivered_nodes } => Ok(delivered_nodes),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn apply_latency_playbook(
        &mut self,
        server_name: String,
        preset: String,
    ) -> Result<(u32, u32, String)> {
        match self.request(IpcRequest::ApplyLatencyPlaybook { server_name, preset }).await? {
            IpcResponse::LatencyPlaybookApplied { view_distance, simulation_distance, message, .. } => {
                Ok((view_distance, simulation_distance, message))
            }
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_tick_profile(
        &mut self,
        server_name: String,
    ) -> Result<(craft_net::TickProfileSummary, String)> {
        match self.request(IpcRequest::GetTickProfile { server_name }).await? {
            IpcResponse::TickProfile { summary, sparkline } => Ok((summary, sparkline)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_packet_stats(
        &mut self,
        server_name: String,
    ) -> Result<craft_net::PacketRateSummary> {
        match self.request(IpcRequest::GetPacketStats { server_name }).await? {
            IpcResponse::PacketStats { summary } => Ok(summary),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_latency_histogram(
        &mut self,
        server_name: String,
    ) -> Result<(craft_net::LatencyHistogram, Vec<String>)> {
        match self.request(IpcRequest::GetLatencyHistogram { server_name }).await? {
            IpcResponse::LatencyHistogram { histogram, chart_lines } => Ok((histogram, chart_lines)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn start_cluster_rollout(&mut self, plan: craft_core::RolloutPlan) -> Result<String> {
        match self.request(IpcRequest::StartClusterRollout { plan }).await? {
            IpcResponse::ClusterRolloutStarted { rollout_id } => Ok(rollout_id),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_cluster_rollout_status(&mut self, cluster: String) -> Result<Option<craft_core::RolloutRecord>> {
        match self.request(IpcRequest::GetClusterRolloutStatus { cluster }).await? {
            IpcResponse::ClusterRolloutStatus { record } => Ok(record),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn abort_cluster_rollout(&mut self, rollout_id: String, reason: String) -> Result<String> {
        match self.request(IpcRequest::AbortClusterRollout { rollout_id, reason }).await? {
            IpcResponse::ClusterRolloutAborted { message } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_fleet_health(&mut self, cluster: String) -> Result<craft_core::FleetHealthStatus> {
        match self.request(IpcRequest::GetFleetHealth { cluster }).await? {
            IpcResponse::FleetHealth { status } => Ok(status),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn execute_fleet_heal(
        &mut self,
        cluster: String,
        action: craft_core::FleetHealingAction,
    ) -> Result<String> {
        match self.request(IpcRequest::ExecuteFleetHeal { cluster, action }).await? {
            IpcResponse::FleetHealResult { message } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn search_logs(&mut self, query: craft_core::LogQuery) -> Result<craft_core::LogSearchResult> {
        match self.request(IpcRequest::SearchLogs { query }).await? {
            IpcResponse::LogSearchResults { result } => Ok(result),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_incident_forensics(
        &mut self,
        server_name: String,
        incident_id: Option<String>,
    ) -> Result<craft_core::IncidentTimeline> {
        match self.request(IpcRequest::GetIncidentForensics { server_name, incident_id }).await? {
            IpcResponse::IncidentForensics { timeline } => Ok(timeline),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn list_incidents(
        &mut self,
        server_name: Option<String>,
    ) -> Result<Vec<crate::protocol::IncidentSummary>> {
        match self.request(IpcRequest::ListIncidents { server_name }).await? {
            IpcResponse::IncidentList { incidents } => Ok(incidents),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn ingest_logs_now(
        &mut self,
        server_name: Option<String>,
    ) -> Result<(usize, usize, u64)> {
        match self.request(IpcRequest::IngestLogsNow { server_name }).await? {
            IpcResponse::IngestResult { indexed_lines, blocks_created, duration_ms } => {
                Ok((indexed_lines, blocks_created, duration_ms))
            }
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_workload_forecast(
        &mut self,
        server_name: String,
        horizon_hours: u32,
    ) -> Result<craft_core::WorkloadForecast> {
        match self.request(IpcRequest::GetWorkloadForecast { server_name, horizon_hours }).await? {
            IpcResponse::WorkloadForecastResult { forecast } => Ok(forecast),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_cost_optimization_report(&mut self, server_name: Option<String>) -> Result<craft_core::CostOptimizationReport> {
        match self.request(IpcRequest::GetCostOptimizationReport { server_name }).await? {
            IpcResponse::CostOptimizationReportResult { report } => Ok(report),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn set_workload_policy(&mut self, policy: craft_core::WorkloadPolicy) -> Result<Vec<craft_core::WorkloadPolicy>> {
        match self.request(IpcRequest::SetWorkloadPolicy { policy }).await? {
            IpcResponse::WorkloadPolicyResult { policies } => Ok(policies),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn trigger_proactive_scaling(&mut self, server_name: String) -> Result<(String, String)> {
        match self.request(IpcRequest::TriggerProactiveScalingNow { server_name }).await? {
            IpcResponse::ProactiveScalingResult { message, applied_action } => Ok((message, applied_action)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn build_modpack(
        &mut self,
        name: String,
        version: String,
        loader: String,
        mc_version: String,
        base_path: String,
    ) -> Result<craft_core::ModpackBuildManifest> {
        match self
            .request(IpcRequest::BuildModpack {
                name,
                version,
                loader,
                mc_version,
                base_path,
            })
            .await?
        {
            IpcResponse::ModpackBuildResult { manifest } => Ok(manifest),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn generate_modpack_delta(
        &mut self,
        pack_name: String,
        source_version: String,
        target_version: String,
    ) -> Result<craft_core::DeltaPatchManifest> {
        match self
            .request(IpcRequest::GenerateDelta {
                pack_name,
                source_version,
                target_version,
            })
            .await?
        {
            IpcResponse::DeltaResult { delta_manifest } => Ok(delta_manifest),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_modpack_status(
        &mut self,
        pack_name: String,
    ) -> Result<(Vec<craft_core::ModpackBuildManifest>, Vec<craft_core::DeltaPatchManifest>)> {
        match self.request(IpcRequest::GetModpackStatus { pack_name }).await? {
            IpcResponse::ModpackStatus { versions, deltas } => Ok((versions, deltas)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_modpack_chunk(
        &mut self,
        file_path: String,
        range_header: Option<String>,
    ) -> Result<crate::modpack_service::ModpackChunkResponse> {
        match self
            .request(IpcRequest::GetModpackChunk {
                file_path,
                range_header,
            })
            .await?
        {
            IpcResponse::ModpackChunk { chunk } => Ok(chunk),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_sdn_topology(&mut self) -> Result<crate::sdn_service::SdnTopologySummary> {
        match self.request(IpcRequest::GetSdnTopology).await? {
            IpcResponse::SdnTopologyResult { topology } => Ok(topology),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn apply_sdn_policy(
        &mut self,
        policy: craft_core::MicrosegmentationPolicy,
    ) -> Result<usize> {
        match self.request(IpcRequest::ApplySdnPolicy { policy }).await? {
            IpcResponse::SdnPolicyResult { rules_count, .. } => Ok(rules_count),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn rotate_sdn_keys(&mut self) -> Result<crate::sdn_service::KeyRotationSummary> {
        match self.request(IpcRequest::RotateSdnKeys).await? {
            IpcResponse::SdnKeyRotationResult { summary } => Ok(summary),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_peer_status(
        &mut self,
        node_id: String,
    ) -> Result<Option<craft_net::WireguardPeerMetrics>> {
        match self.request(IpcRequest::GetPeerStatus { node_id }).await? {
            IpcResponse::SdnPeerStatusResult { peer } => Ok(peer),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_raft_status(&mut self) -> Result<crate::raft_engine::RaftStatusSummary> {
        match self.request(IpcRequest::GetRaftStatus).await? {
            IpcResponse::RaftStatusResult { status } => Ok(status),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn propose_raft_command(
        &mut self,
        payload: craft_core::RaftPayload,
    ) -> Result<(u64, u64)> {
        match self.request(IpcRequest::ProposeRaftCommand { payload }).await? {
            IpcResponse::RaftCommandProposedResult { term, index } => Ok((term, index)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn acquire_distributed_lock(
        &mut self,
        lock_name: String,
        holder_id: String,
        lease_secs: u64,
    ) -> Result<craft_core::DistributedLock> {
        match self
            .request(IpcRequest::AcquireDistributedLock {
                lock_name,
                holder_id,
                lease_secs,
            })
            .await?
        {
            IpcResponse::DistributedLockAcquiredResult { lock } => Ok(lock),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn release_distributed_lock(
        &mut self,
        lock_name: String,
        holder_id: String,
    ) -> Result<String> {
        match self
            .request(IpcRequest::ReleaseDistributedLock {
                lock_name,
                holder_id,
            })
            .await?
        {
            IpcResponse::DistributedLockReleasedResult { message } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn step_down_raft_leader(&mut self) -> Result<String> {
        match self.request(IpcRequest::StepDownRaftLeader).await? {
            IpcResponse::RaftStepDownResult { message } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn transfer_raft_leadership(&mut self, target_node_id: String) -> Result<String> {
        match self
            .request(IpcRequest::TransferRaftLeadership { target_node_id })
            .await?
        {
            IpcResponse::RaftLeadershipTransferredResult { message } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_raft_logs(
        &mut self,
        limit: Option<usize>,
    ) -> Result<Vec<craft_core::RaftLogEntry>> {
        match self.request(IpcRequest::GetRaftLogs { limit }).await? {
            IpcResponse::RaftLogsResult { entries } => Ok(entries),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_multiraft_status(
        &mut self,
        group_id: Option<u64>,
    ) -> Result<(
        craft_core::MultiRaftRegistry,
        std::collections::HashMap<u64, crate::raft_engine::RaftStatusSummary>,
        std::collections::HashMap<String, craft_core::LearnerSyncProgress>,
    )> {
        match self.request(IpcRequest::RaftGetMultiRaftStatus { group_id }).await? {
            IpcResponse::RaftMultiRaftStatusResult {
                registry,
                statuses,
                learner_progress,
            } => Ok((registry, statuses, learner_progress)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn reconfigure_raft_membership(
        &mut self,
        group_id: u64,
        change_type: craft_core::MembershipChangeType,
        node: craft_core::RaftNode,
    ) -> Result<(bool, craft_core::JointConsensusPhase, String)> {
        match self
            .request(IpcRequest::RaftReconfigureMembership {
                group_id,
                change_type,
                node,
            })
            .await?
        {
            IpcResponse::RaftReconfigureMembershipResult {
                success,
                phase,
                message,
            } => Ok((success, phase, message)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn trigger_raft_compaction(
        &mut self,
        group_id: u64,
        force: bool,
    ) -> Result<(u64, u64, u64, u64)> {
        match self
            .request(IpcRequest::RaftTriggerCompaction { group_id, force })
            .await?
        {
            IpcResponse::RaftCompactionResult {
                last_included_index,
                entries_compacted,
                snapshot_bytes,
                duration_ms,
                ..
            } => Ok((last_included_index, entries_compacted, snapshot_bytes, duration_ms)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn route_raft_partition_key(
        &mut self,
        key: String,
    ) -> Result<(String, u64, String, Option<String>)> {
        match self.request(IpcRequest::RaftRoutePartitionKey { key }).await? {
            IpcResponse::RaftPartitionRouteResult {
                key,
                group_id,
                partition_name,
                leader_node_id,
            } => Ok((key, group_id, partition_name, leader_node_id)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn manage_raft_partition(
        &mut self,
        action: String,
        partition: Option<craft_core::MultiRaftPartition>,
        group_id: Option<u64>,
    ) -> Result<(bool, String, Vec<craft_core::MultiRaftPartition>)> {
        match self
            .request(IpcRequest::RaftManagePartition {
                action,
                partition,
                group_id,
            })
            .await?
        {
            IpcResponse::RaftManagePartitionResult {
                success,
                message,
                partitions,
            } => Ok((success, message, partitions)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_server_quota(
        &mut self,
        server: String,
    ) -> Result<craft_core::QuotaUsageSummary> {
        match self.request(IpcRequest::GetServerQuota { server }).await? {
            IpcResponse::ServerQuotaResult { summary } => Ok(summary),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn set_server_quota(
        &mut self,
        limits: craft_core::ServerResourceLimit,
    ) -> Result<craft_core::QuotaUsageSummary> {
        match self.request(IpcRequest::SetServerQuota { limits }).await? {
            IpcResponse::ServerQuotaResult { summary } => Ok(summary),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_tenant_quota(
        &mut self,
        tenant: String,
    ) -> Result<(craft_core::TenantQuota, u64, u32, usize)> {
        match self.request(IpcRequest::GetTenantQuota { tenant }).await? {
            IpcResponse::TenantQuotaResult {
                quota,
                allocated_memory_mb,
                allocated_cpu_percent,
                server_count,
            } => Ok((quota, allocated_memory_mb, allocated_cpu_percent, server_count)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn set_tenant_quota(&mut self, quota: craft_core::TenantQuota) -> Result<()> {
        match self.request(IpcRequest::SetTenantQuota { quota }).await? {
            IpcResponse::Success { .. } => Ok(()),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn list_quota_usage(
        &mut self,
        tenant: Option<String>,
    ) -> Result<Vec<craft_core::QuotaUsageSummary>> {
        match self.request(IpcRequest::ListQuotaUsage { tenant }).await? {
            IpcResponse::QuotaUsageListResult { items } => Ok(items),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn enforce_fair_share_now(&mut self) -> Result<(usize, String)> {
        match self.request(IpcRequest::EnforceFairShareNow).await? {
            IpcResponse::FairShareEnforcedResult {
                rebalanced_count,
                message,
            } => Ok((rebalanced_count, message)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_tracing_status(&mut self) -> Result<craft_core::TracingStatusSummary> {
        match self.request(IpcRequest::GetTracingStatus).await? {
            IpcResponse::TracingStatusResult { status } => Ok(status),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn query_traces(
        &mut self,
        service: Option<String>,
        name: Option<String>,
        min_duration_micros: Option<u64>,
        error_only: bool,
        limit: Option<usize>,
    ) -> Result<Vec<craft_core::RecordedSpan>> {
        match self
            .request(IpcRequest::QueryTraces {
                service,
                name,
                min_duration_micros,
                error_only,
                limit,
            })
            .await?
        {
            IpcResponse::TracesQueryResult { spans } => Ok(spans),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_trace_details(&mut self, trace_id: String) -> Result<Option<craft_core::TraceTree>> {
        match self.request(IpcRequest::GetTraceDetails { trace_id }).await? {
            IpcResponse::TraceDetailsResult { trace_tree } => Ok(trace_tree),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn export_traces_now(&mut self, limit: Option<usize>) -> Result<(usize, String)> {
        match self.request(IpcRequest::ExportTracesNow { limit }).await? {
            IpcResponse::TracesExportedResult {
                exported_count,
                destination,
            } => Ok((exported_count, destination)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn set_tracing_config(&mut self, config: craft_core::TracingConfig) -> Result<craft_core::TracingConfig> {
        match self.request(IpcRequest::SetTracingConfig { config }).await? {
            IpcResponse::TracingConfigResult { config } => Ok(config),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_anvil_status(&mut self) -> Result<craft_core::AnvilStatusSummary> {
        match self.request(IpcRequest::GetAnvilStatus).await? {
            IpcResponse::AnvilStatusResult { status } => Ok(status),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn inspect_region(&mut self, server_path: PathBuf, region_file: String) -> Result<craft_core::RegionDetails> {
        match self.request(IpcRequest::InspectRegion { server_path, region_file }).await? {
            IpcResponse::AnvilRegionInspectionResult { details } => Ok(details),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn prefetch_chunks(
        &mut self,
        server_path: PathBuf,
        world: String,
        center_x: i32,
        center_z: i32,
        radius: u32,
    ) -> Result<craft_core::PrefetchSummary> {
        match self.request(IpcRequest::PrefetchChunks {
            server_path,
            world,
            center_x,
            center_z,
            radius,
        }).await? {
            IpcResponse::AnvilPrefetchResult { summary } => Ok(summary),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn benchmark_anvil(&mut self, chunks: usize) -> Result<craft_core::AnvilBenchmarkReport> {
        match self.request(IpcRequest::BenchmarkAnvil { chunks }).await? {
            IpcResponse::AnvilBenchmarkResult { report } => Ok(report),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn set_anvil_config(&mut self, config: craft_core::AnvilConfig) -> Result<craft_core::AnvilConfig> {
        match self.request(IpcRequest::SetAnvilConfig { config }).await? {
            IpcResponse::AnvilConfigResult { config } => Ok(config),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_numa_status(&mut self) -> Result<craft_core::NumaStatusSummary> {
        match self.request(IpcRequest::GetNumaStatus).await? {
            IpcResponse::NumaStatusResult { summary } => Ok(summary),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn pin_server_cores(
        &mut self,
        server_name: String,
        cpus: Vec<usize>,
        numa_node: Option<u32>,
        policy: craft_core::NumaPolicy,
    ) -> Result<craft_core::ServerPinningConfig> {
        match self.request(IpcRequest::PinServerCores { server_name, cpus, numa_node, policy }).await? {
            IpcResponse::PinServerCoresResult { config, .. } => Ok(config),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn set_numa_policy(
        &mut self,
        server_name: String,
        policy: craft_core::NumaPolicy,
    ) -> Result<craft_core::ServerPinningConfig> {
        match self.request(IpcRequest::SetNumaPolicy { server_name, policy }).await? {
            IpcResponse::NumaPolicyResult { config, .. } => Ok(config),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn benchmark_numa_memory(&mut self, node_id: u32, size_mb: usize) -> Result<craft_core::NumaBenchmarkReport> {
        match self.request(IpcRequest::BenchmarkNumaMemory { node_id, size_mb }).await? {
            IpcResponse::NumaBenchmarkResult { report } => Ok(report),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_dpdk_status(&mut self, bench_count: Option<usize>) -> Result<craft_net::DpdkDriverStats> {
        match self.request(IpcRequest::GetDpdkStatus { bench_count }).await? {
            IpcResponse::DpdkStatusResult { stats } => Ok(stats),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn start_live_migration(
        &mut self,
        plan: craft_core::LiveMigrationPlan,
    ) -> Result<craft_core::LiveMigrationPlan> {
        match self.request(IpcRequest::MigrationStartLive { plan }).await? {
            IpcResponse::MigrationStarted { plan } => Ok(plan),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_migration_status(
        &mut self,
        migration_id: Option<String>,
    ) -> Result<Vec<craft_core::LiveMigrationPlan>> {
        match self.request(IpcRequest::MigrationGetStatus { migration_id }).await? {
            IpcResponse::MigrationStatus { plans } => Ok(plans),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn abort_migration(
        &mut self,
        migration_id: String,
        reason: Option<String>,
    ) -> Result<craft_core::LiveMigrationPlan> {
        match self.request(IpcRequest::MigrationAbort { migration_id, reason }).await? {
            IpcResponse::MigrationAborted { plan, .. } => Ok(plan),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn list_migrations(&mut self) -> Result<Vec<craft_core::LiveMigrationPlan>> {
        match self.request(IpcRequest::MigrationList).await? {
            IpcResponse::MigrationListResult { plans } => Ok(plans),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn manage_anycast_route(
        &mut self,
        action: String,
        route: craft_core::AnycastRouteAnnouncement,
    ) -> Result<(bool, String, Vec<craft_core::AnycastRouteAnnouncement>)> {
        match self.request(IpcRequest::AnycastRouteManage { action, route }).await? {
            IpcResponse::AnycastRouteManageResult { success, message, routes } => Ok((success, message, routes)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn start_ebpf_profiling(
        &mut self,
        server_name: String,
        probe_type: craft_core::EbpfProbeType,
        duration_secs: u64,
        sample_rate_hz: u32,
    ) -> Result<craft_core::EbpfProbeDescriptor> {
        match self.request(IpcRequest::EbpfStartProfiling {
            server_name,
            probe_type,
            duration_secs,
            sample_rate_hz,
        }).await? {
            IpcResponse::EbpfProfilingStarted { descriptor } => Ok(descriptor),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_ebpf_status(
        &mut self,
        server_name: String,
    ) -> Result<(
        Option<craft_core::EbpfProbeDescriptor>,
        Option<craft_net::SocketBufferTelemetry>,
        std::collections::HashMap<String, (u64, f64)>,
    )> {
        match self.request(IpcRequest::EbpfGetStatus { server_name }).await? {
            IpcResponse::EbpfStatusResult {
                descriptor,
                socket_telemetry,
                syscall_aggregations,
            } => Ok((descriptor, socket_telemetry, syscall_aggregations)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_ebpf_flamegraph(
        &mut self,
        server_name: String,
        format: String,
    ) -> Result<(String, craft_core::FlameGraphNode)> {
        match self.request(IpcRequest::EbpfGetFlameGraph { server_name, format }).await? {
            IpcResponse::EbpfFlameGraphResult { content, root_node, .. } => Ok((content, root_node)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_ebpf_gc_telemetry(
        &mut self,
        server_name: String,
        limit: usize,
    ) -> Result<Vec<craft_core::JvmGcEvent>> {
        match self.request(IpcRequest::EbpfGetGcTelemetry { server_name, limit }).await? {
            IpcResponse::EbpfGcTelemetryResult { events, .. } => Ok(events),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn stop_ebpf_profiling(
        &mut self,
        server_name: String,
        probe_id: Option<String>,
    ) -> Result<(craft_core::EbpfProbeDescriptor, String)> {
        match self.request(IpcRequest::EbpfStopProfiling { server_name, probe_id }).await? {
            IpcResponse::EbpfProfilingStopped { descriptor, message } => Ok((descriptor, message)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn verify_supply_chain_artifact(
        &mut self,
        artifact_path: std::path::PathBuf,
        attestation_path: Option<std::path::PathBuf>,
        strict: bool,
    ) -> Result<craft_core::VerificationVerdict> {
        match self
            .request(IpcRequest::SupplyChainVerify {
                artifact_path,
                attestation_path,
                strict,
            })
            .await?
        {
            IpcResponse::SupplyChainVerdict { verdict } => Ok(verdict),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_supply_chain_policy(
        &mut self,
    ) -> Result<(craft_core::SupplyChainPolicy, usize)> {
        match self.request(IpcRequest::SupplyChainGetPolicy).await? {
            IpcResponse::SupplyChainPolicyResult {
                policy,
                trust_anchors_count,
            } => Ok((policy, trust_anchors_count)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn set_supply_chain_policy(
        &mut self,
        policy: craft_core::SupplyChainPolicy,
    ) -> Result<String> {
        match self
            .request(IpcRequest::SupplyChainSetPolicy { policy })
            .await?
        {
            IpcResponse::Success { message } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn inspect_supply_chain_attestation(
        &mut self,
        identifier: String,
    ) -> Result<Option<craft_core::InTotoStatement>> {
        match self
            .request(IpcRequest::SupplyChainInspectAttestation { identifier })
            .await?
        {
            IpcResponse::SupplyChainAttestationResult { attestation } => Ok(attestation),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn run_hermetic_build(
        &mut self,
        build_dir: std::path::PathBuf,
        command: String,
        args: Vec<String>,
        allow_network: bool,
    ) -> Result<craft_core::HermeticBuildManifest> {
        match self
            .request(IpcRequest::HermeticBuildRun {
                build_dir,
                command,
                args,
                allow_network,
            })
            .await?
        {
            IpcResponse::HermeticBuildResult { manifest } => Ok(manifest),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_pqc_status(&mut self) -> Result<craft_core::pqc::PqcStatusSummary> {
        match self.request(IpcRequest::PqcGetStatus).await? {
            IpcResponse::PqcStatus { summary } => Ok(summary),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn set_pqc_policy(
        &mut self,
        policy: craft_core::pqc::PqcPolicy,
    ) -> Result<(craft_core::pqc::PqcPolicy, String)> {
        match self.request(IpcRequest::PqcSetPolicy { policy }).await? {
            IpcResponse::PqcPolicyResult { policy, message } => Ok((policy, message)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn generate_pqc_keypair(
        &mut self,
        suite: Option<craft_core::pqc::PqcCipherSuite>,
        algorithm: Option<craft_core::pqc::PqcSigningAlgorithm>,
    ) -> Result<craft_core::pqc::PqcKeyPair> {
        match self
            .request(IpcRequest::PqcGenerateKeyPair { suite, algorithm })
            .await?
        {
            IpcResponse::PqcKeyPairResult { keypair } => Ok(keypair),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn benchmark_pqc(
        &mut self,
        iterations: usize,
    ) -> Result<craft_core::pqc::PqcBenchmarkReport> {
        match self
            .request(IpcRequest::PqcBenchmark { iterations })
            .await?
        {
            IpcResponse::PqcBenchmarkResult { report } => Ok(report),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn migrate_pqc_node(
        &mut self,
        target_phase: craft_core::pqc::PqcMigrationPhase,
    ) -> Result<(craft_core::pqc::PqcMigrationPhase, String)> {
        match self
            .request(IpcRequest::PqcMigrateNode { target_phase })
            .await?
        {
            IpcResponse::PqcMigrationResult { new_phase, message } => Ok((new_phase, message)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_hsm_status(&mut self) -> Result<craft_core::hsm::HsmStatusSummary> {
        match self.request(IpcRequest::HsmGetStatus).await? {
            IpcResponse::HsmStatus { summary } => Ok(summary),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn generate_hsm_key(
        &mut self,
        label: &str,
        key_type: craft_core::hsm::HsmKeyType,
    ) -> Result<craft_core::hsm::HsmKeyHandle> {
        match self
            .request(IpcRequest::HsmGenerateKey {
                label: label.to_string(),
                key_type,
            })
            .await?
        {
            IpcResponse::HsmKeyResult { key } => Ok(key),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn sign_with_hsm(&mut self, label: &str, data: &[u8]) -> Result<Vec<u8>> {
        match self
            .request(IpcRequest::HsmSign {
                label: label.to_string(),
                data: data.to_vec(),
            })
            .await?
        {
            IpcResponse::HsmSignResult { signature } => Ok(signature),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn attest_hsm(
        &mut self,
        pcr_mask: u32,
        nonce: Option<[u8; 32]>,
    ) -> Result<craft_core::hsm::PcrQuote> {
        match self.request(IpcRequest::HsmAttest { nonce, pcr_mask }).await? {
            IpcResponse::HsmAttestResult { quote } => Ok(quote),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn prove_hsm_zk(
        &mut self,
        cluster_id: &str,
    ) -> Result<craft_core::hsm::ZkMembershipProof> {
        match self
            .request(IpcRequest::HsmZkProve {
                cluster_id: cluster_id.to_string(),
            })
            .await?
        {
            IpcResponse::HsmZkProveResult { proof } => Ok(proof),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn verify_hsm_zk(
        &mut self,
        cluster_id: &str,
        proof: craft_core::hsm::ZkMembershipProof,
    ) -> Result<(bool, String)> {
        match self
            .request(IpcRequest::HsmZkVerify {
                cluster_id: cluster_id.to_string(),
                proof,
            })
            .await?
        {
            IpcResponse::HsmZkVerifyResult { valid, message } => Ok((valid, message)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_compaction_status(&mut self) -> Result<craft_core::CompactionStatusSummary> {
        match self.request(IpcRequest::CompactionGetStatus).await? {
            IpcResponse::CompactionStatusResult { summary } => Ok(summary),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn trigger_compaction(&mut self) -> Result<craft_core::CompactionCycleResult> {
        match self.request(IpcRequest::CompactionTriggerNow).await? {
            IpcResponse::CompactionCycleResult { result } => Ok(result),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn configure_thp(
        &mut self,
        mode: craft_core::ThpMode,
        defrag: craft_core::ThpDefragMode,
    ) -> Result<(craft_core::ThpStatus, String)> {
        match self.request(IpcRequest::CompactionConfigureThp { mode, defrag }).await? {
            IpcResponse::CompactionThpConfigured { status, message } => Ok((status, message)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_compaction_pool_stats(&mut self) -> Result<craft_core::PagePoolStats> {
        match self.request(IpcRequest::CompactionGetPoolStats).await? {
            IpcResponse::CompactionPoolStatsResult { stats } => Ok(stats),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn reset_compaction_metrics(&mut self) -> Result<String> {
        match self.request(IpcRequest::CompactionResetMetrics).await? {
            IpcResponse::CompactionMetricsResetResult { message } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_xdp_status(&mut self) -> Result<craft_core::XdpInterfaceStatus> {
        match self.request(IpcRequest::XdpGetStatus).await? {
            IpcResponse::XdpStatusResult { status } => Ok(status),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn attach_xdp_interface(
        &mut self,
        interface_name: &str,
        mode: craft_core::XdpAttachMode,
    ) -> Result<craft_core::XdpInterfaceStatus> {
        match self
            .request(IpcRequest::XdpAttachInterface {
                interface_name: interface_name.to_string(),
                mode,
            })
            .await?
        {
            IpcResponse::XdpStatusResult { status } => Ok(status),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn detach_xdp_interface(&mut self) -> Result<craft_core::XdpInterfaceStatus> {
        match self.request(IpcRequest::XdpDetachInterface).await? {
            IpcResponse::XdpStatusResult { status } => Ok(status),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn add_xdp_rule(
        &mut self,
        rule: craft_core::XdpFilterRule,
    ) -> Result<(craft_core::XdpInterfaceStatus, String)> {
        match self.request(IpcRequest::XdpAddRule { rule }).await? {
            IpcResponse::XdpRuleModified { status, message } => Ok((status, message)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn remove_xdp_rule(
        &mut self,
        rule_id: &str,
    ) -> Result<(craft_core::XdpInterfaceStatus, String)> {
        match self
            .request(IpcRequest::XdpRemoveRule {
                rule_id: rule_id.to_string(),
            })
            .await?
        {
            IpcResponse::XdpRuleModified { status, message } => Ok((status, message)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn reset_xdp_metrics(&mut self) -> Result<String> {
        match self.request(IpcRequest::XdpResetMetrics).await? {
            IpcResponse::XdpMetricsResetResult { message } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_pmu_status(&mut self) -> Result<craft_core::pmu::PmuMetricsSummary> {
        match self.request(IpcRequest::PmuGetStatus).await? {
            IpcResponse::PmuStatusResult { summary } => Ok(summary),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn start_pmu_sampling(
        &mut self,
        target_pid: Option<u32>,
        sample_rate_hz: u32,
    ) -> Result<craft_core::pmu::PmuMetricsSummary> {
        match self
            .request(IpcRequest::PmuStartSampling {
                target_pid,
                sample_rate_hz,
            })
            .await?
        {
            IpcResponse::PmuStatusResult { summary } => Ok(summary),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn stop_pmu_sampling(&mut self) -> Result<craft_core::pmu::PmuMetricsSummary> {
        match self.request(IpcRequest::PmuStopSampling).await? {
            IpcResponse::PmuStatusResult { summary } => Ok(summary),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn sample_pmu_now(
        &mut self,
        target_pid: Option<u32>,
    ) -> Result<craft_core::pmu::PmuSampleRecord> {
        match self.request(IpcRequest::PmuSampleNow { target_pid }).await? {
            IpcResponse::PmuSampleResult { sample } => Ok(sample),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_pmu_hotspots(
        &mut self,
        limit: usize,
    ) -> Result<Vec<craft_core::pmu::HotspotSymbol>> {
        match self.request(IpcRequest::PmuGetHotspots { limit }).await? {
            IpcResponse::PmuHotspotsResult { hotspots } => Ok(hotspots),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn run_pmu_bench(
        &mut self,
        iterations: usize,
    ) -> Result<craft_net::MemoryChurnReport> {
        match self.request(IpcRequest::PmuRunBench { iterations }).await? {
            IpcResponse::PmuBenchResult { report } => Ok(report),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn reset_pmu_metrics(&mut self) -> Result<String> {
        match self.request(IpcRequest::PmuResetMetrics).await? {
            IpcResponse::PmuMetricsResetResult { message } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_shm_status(
        &mut self,
        server: Option<String>,
    ) -> Result<craft_core::shm::ShmStatusSummary> {
        match self.request(IpcRequest::ShmGetStatus { server }).await? {
            IpcResponse::ShmStatusResult { summary } => Ok(summary),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn create_shm_channel(
        &mut self,
        server: String,
        channel: String,
        slot_size: usize,
        slot_count: usize,
    ) -> Result<craft_core::shm::ShmSegmentMeta> {
        match self
            .request(IpcRequest::ShmCreateChannel {
                server,
                channel,
                slot_size,
                slot_count,
            })
            .await?
        {
            IpcResponse::ShmChannelCreatedResult { meta } => Ok(meta),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn close_shm_channel(
        &mut self,
        server: String,
        channel: String,
    ) -> Result<bool> {
        match self.request(IpcRequest::ShmCloseChannel { server, channel }).await? {
            IpcResponse::ShmChannelClosedResult { closed } => Ok(closed),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn write_shm_event(
        &mut self,
        server: String,
        channel: String,
        payload: Vec<u8>,
    ) -> Result<u64> {
        match self.request(IpcRequest::ShmWriteEvent { server, channel, payload }).await? {
            IpcResponse::ShmEventWrittenResult { sequence } => Ok(sequence),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn read_shm_events(
        &mut self,
        server: String,
        channel: String,
        limit: usize,
    ) -> Result<Vec<Vec<u8>>> {
        match self.request(IpcRequest::ShmReadEvents { server, channel, limit }).await? {
            IpcResponse::ShmEventsReadResult { events } => Ok(events),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn run_shm_bench(
        &mut self,
        message_count: usize,
        payload_size: usize,
    ) -> Result<craft_core::shm::ShmBenchmarkMetrics> {
        match self.request(IpcRequest::ShmRunBench { message_count, payload_size }).await? {
            IpcResponse::ShmBenchResult { metrics } => Ok(metrics),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn reset_shm_metrics(&mut self, server: Option<String>) -> Result<String> {
        match self.request(IpcRequest::ShmResetMetrics { server }).await? {
            IpcResponse::ShmMetricsResetResult { message } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_patch_status(
        &mut self,
        server: Option<String>,
    ) -> Result<craft_core::patch::PatchStatusSummary> {
        match self.request(IpcRequest::PatchGetStatus { server }).await? {
            IpcResponse::PatchStatusResult { summary } => Ok(summary),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn apply_patch(
        &mut self,
        server: String,
        patch_name: String,
        target_symbol: String,
        shadow_bytes_hex: String,
    ) -> Result<craft_core::patch::PatchManifest> {
        match self
            .request(IpcRequest::PatchApply {
                server,
                patch_name,
                target_symbol,
                shadow_bytes_hex,
            })
            .await?
        {
            IpcResponse::PatchAppliedResult { manifest } => Ok(manifest),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn rollback_patch(
        &mut self,
        server: String,
        patch_name: String,
    ) -> Result<bool> {
        match self
            .request(IpcRequest::PatchRollback { server, patch_name })
            .await?
        {
            IpcResponse::PatchRolledBackResult { rolled_back } => Ok(rolled_back),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_patch_diff(
        &mut self,
        server: String,
        patch_name: String,
    ) -> Result<String> {
        match self
            .request(IpcRequest::PatchGetDiff { server, patch_name })
            .await?
        {
            IpcResponse::PatchDiffResult { diff } => Ok(diff),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn run_patch_bench(
        &mut self,
        iterations: usize,
    ) -> Result<craft_core::patch::PatchBenchmarkMetrics> {
        match self.request(IpcRequest::PatchRunBench { iterations }).await? {
            IpcResponse::PatchBenchResult { metrics } => Ok(metrics),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn reset_patch_metrics(&mut self, server: Option<String>) -> Result<String> {
        match self.request(IpcRequest::PatchResetMetrics { server }).await? {
            IpcResponse::PatchMetricsResetResult { message } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_vm_status(
        &mut self,
        vm_id: Option<String>,
    ) -> Result<craft_core::vm::MicroVmStatusSummary> {
        match self.request(IpcRequest::VmGetStatus { vm_id }).await? {
            IpcResponse::VmStatusResult { summary } => Ok(summary),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn spawn_vm(
        &mut self,
        name: String,
        vcpus: u32,
        memory_mb: u64,
        vsock_cid: Option<u32>,
        virtio_devices: Vec<String>,
    ) -> Result<craft_core::vm::MicroVmDescriptor> {
        match self
            .request(IpcRequest::VmSpawn {
                name,
                vcpus,
                memory_mb,
                vsock_cid,
                virtio_devices,
            })
            .await?
        {
            IpcResponse::VmSpawnResult { descriptor } => Ok(descriptor),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn stop_vm(&mut self, vm_id: String, force: bool) -> Result<bool> {
        match self.request(IpcRequest::VmStop { vm_id, force }).await? {
            IpcResponse::VmStopResult { stopped } => Ok(stopped),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn inspect_vm(&mut self, vm_id: String) -> Result<craft_core::vm::MicroVmDescriptor> {
        match self.request(IpcRequest::VmInspect { vm_id }).await? {
            IpcResponse::VmInspectResult { descriptor } => Ok(descriptor),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn run_vm_bench(
        &mut self,
        concurrency: usize,
        iterations: usize,
    ) -> Result<craft_core::vm::MicroVmBenchmarkMetrics> {
        match self.request(IpcRequest::VmRunBench { concurrency, iterations }).await? {
            IpcResponse::VmBenchResult { metrics } => Ok(metrics),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn reset_vm_metrics(&mut self, vm_id: Option<String>) -> Result<String> {
        match self.request(IpcRequest::VmResetMetrics { vm_id }).await? {
            IpcResponse::VmResetMetricsResult { message } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_crash_status(
        &mut self,
        server: Option<String>,
    ) -> Result<craft_core::crash::CrashTriageStatusSummary> {
        match self.request(IpcRequest::CrashGetStatus { server }).await? {
            IpcResponse::CrashStatusResult { summary } => Ok(summary),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn triage_crash_file(
        &mut self,
        server: Option<String>,
        file_path: String,
    ) -> Result<craft_core::crash::CrashTriageReport> {
        match self
            .request(IpcRequest::CrashTriageFile {
                server,
                file_path,
            })
            .await?
        {
            IpcResponse::CrashTriageReportResult { report } => Ok(report),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn list_crash_reports(
        &mut self,
        server: Option<String>,
        limit: Option<usize>,
    ) -> Result<Vec<craft_core::crash::CrashTriageReport>> {
        match self
            .request(IpcRequest::CrashListReports { server, limit })
            .await?
        {
            IpcResponse::CrashReportsListResult { reports } => Ok(reports),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_crash_report(
        &mut self,
        report_id: String,
    ) -> Result<craft_core::crash::CrashTriageReport> {
        match self.request(IpcRequest::CrashGetReport { report_id }).await? {
            IpcResponse::CrashTriageReportResult { report } => Ok(report),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn run_crash_bench(
        &mut self,
        iterations: usize,
    ) -> Result<craft_core::crash::CrashTriageBenchmarkMetrics> {
        match self.request(IpcRequest::CrashRunBench { iterations }).await? {
            IpcResponse::CrashBenchResult { metrics } => Ok(metrics),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn reset_crash_metrics(
        &mut self,
        server: Option<String>,
    ) -> Result<String> {
        match self.request(IpcRequest::CrashResetMetrics { server }).await? {
            IpcResponse::CrashMetricsResetResult { message } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_rdma_status(
        &mut self,
        server: Option<String>,
    ) -> Result<craft_core::rdma::RdmaStatusSummary> {
        match self.request(IpcRequest::RdmaGetStatus { server }).await? {
            IpcResponse::RdmaStatusResult { summary } => Ok(summary),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn register_rdma_mr(
        &mut self,
        server: Option<String>,
        size: usize,
        read_only: bool,
    ) -> Result<craft_core::rdma::MemoryRegionDescriptor> {
        match self
            .request(IpcRequest::RdmaRegisterMr {
                server,
                size,
                read_only,
            })
            .await?
        {
            IpcResponse::RdmaMrResult { mr } => Ok(mr),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn connect_rdma_peer(
        &mut self,
        server: Option<String>,
        peer_address: String,
        qp_num: u32,
    ) -> Result<craft_core::rdma::RdmaPeerEndpoint> {
        match self
            .request(IpcRequest::RdmaConnectPeer {
                server,
                peer_address,
                qp_num,
            })
            .await?
        {
            IpcResponse::RdmaPeerResult { peer } => Ok(peer),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn list_rdma_peers(
        &mut self,
        server: Option<String>,
    ) -> Result<Vec<craft_core::rdma::RdmaPeerEndpoint>> {
        match self.request(IpcRequest::RdmaListPeers { server }).await? {
            IpcResponse::RdmaPeersListResult { peers } => Ok(peers),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn run_rdma_bench(
        &mut self,
        iterations: usize,
        buffer_size: usize,
    ) -> Result<craft_core::rdma::RdmaBenchmarkMetrics> {
        match self
            .request(IpcRequest::RdmaRunBench {
                iterations,
                buffer_size,
            })
            .await?
        {
            IpcResponse::RdmaBenchResult { metrics } => Ok(metrics),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn reset_rdma_metrics(
        &mut self,
        server: Option<String>,
    ) -> Result<String> {
        match self.request(IpcRequest::RdmaResetMetrics { server }).await? {
            IpcResponse::RdmaMetricsResetResult { message } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_smartnic_status(
        &mut self,
        server: Option<String>,
    ) -> Result<(craft_core::SmartNicStatusSummary, Vec<craft_core::SmartNicDeviceInfo>)> {
        match self.request(IpcRequest::SmartNicGetStatus { server }).await? {
            IpcResponse::SmartNicStatusResult { summary, devices } => Ok((summary, devices)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn install_smartnic_rule(
        &mut self,
        server: Option<String>,
        rule: craft_core::SmartNicOffloadRule,
    ) -> Result<craft_core::SmartNicOffloadRule> {
        match self.request(IpcRequest::SmartNicInstallRule { server, rule }).await? {
            IpcResponse::SmartNicRuleResult { rule } => Ok(rule),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn remove_smartnic_rule(
        &mut self,
        server: Option<String>,
        rule_id: String,
    ) -> Result<bool> {
        match self.request(IpcRequest::SmartNicRemoveRule { server, rule_id }).await? {
            IpcResponse::SmartNicRemoveResult { success } => Ok(success),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn list_smartnic_rules(
        &mut self,
        server: Option<String>,
    ) -> Result<Vec<craft_core::SmartNicOffloadRule>> {
        match self.request(IpcRequest::SmartNicListRules { server }).await? {
            IpcResponse::SmartNicRulesListResult { rules } => Ok(rules),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn run_smartnic_bench(
        &mut self,
        iterations: usize,
        packet_size: usize,
    ) -> Result<craft_core::SmartNicBenchmarkMetrics> {
        match self.request(IpcRequest::SmartNicRunBench { iterations, packet_size }).await? {
            IpcResponse::SmartNicBenchResult { metrics } => Ok(metrics),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn reset_smartnic_metrics(
        &mut self,
        server: Option<String>,
    ) -> Result<String> {
        match self.request(IpcRequest::SmartNicResetMetrics { server }).await? {
            IpcResponse::SmartNicResetResult { message } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_memfabric_status(
        &mut self,
        server: Option<String>,
    ) -> Result<(
        craft_core::memfabric::MemFabricStatusSummary,
        Vec<craft_core::memfabric::MemFabricNodeInfo>,
    )> {
        match self.request(IpcRequest::MemFabricGetStatus { server }).await? {
            IpcResponse::MemFabricStatusResult { summary, nodes } => Ok((summary, nodes)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn allocate_memfabric_page(
        &mut self,
        server: Option<String>,
        page_id: String,
        size: usize,
        tier: craft_core::memfabric::MemoryTier,
        dimension: Option<String>,
    ) -> Result<craft_core::memfabric::RemotePageDescriptor> {
        match self
            .request(IpcRequest::MemFabricAllocatePage {
                server,
                page_id,
                size,
                tier,
                dimension,
            })
            .await?
        {
            IpcResponse::MemFabricPageResult { page } => Ok(page),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn evict_memfabric_dimension(
        &mut self,
        server: Option<String>,
        dimension: String,
        target_node: Option<String>,
    ) -> Result<(usize, u64, String)> {
        match self
            .request(IpcRequest::MemFabricEvictDimension {
                server,
                dimension,
                target_node,
            })
            .await?
        {
            IpcResponse::MemFabricEvictResult {
                pages_evicted,
                bytes_freed,
                dimension,
            } => Ok((pages_evicted, bytes_freed, dimension)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn list_memfabric_pages(
        &mut self,
        server: Option<String>,
    ) -> Result<Vec<craft_core::memfabric::RemotePageDescriptor>> {
        match self.request(IpcRequest::MemFabricListPages { server }).await? {
            IpcResponse::MemFabricPagesListResult { pages } => Ok(pages),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn run_memfabric_bench(
        &mut self,
        iterations: usize,
        page_size: usize,
    ) -> Result<craft_core::memfabric::MemFabricBenchmarkMetrics> {
        match self
            .request(IpcRequest::MemFabricRunBench {
                iterations,
                page_size,
            })
            .await?
        {
            IpcResponse::MemFabricBenchResult { metrics } => Ok(metrics),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn reset_memfabric_metrics(
        &mut self,
        server: Option<String>,
    ) -> Result<String> {
        match self.request(IpcRequest::MemFabricResetMetrics { server }).await? {
            IpcResponse::MemFabricResetResult { message } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_nvme_status(
        &mut self,
        server: Option<String>,
    ) -> Result<(craft_core::nvme::NvmeStatusSummary, Vec<craft_core::nvme::NvmeSubsystemDescriptor>)> {
        match self.request(IpcRequest::NvmeGetStatus { server }).await? {
            IpcResponse::NvmeStatusResult { summary, subsystems } => Ok((summary, subsystems)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn create_nvme_namespace(
        &mut self,
        nsid: u32,
        size_mb: u64,
        block_size: u32,
        server_id: Option<String>,
        dimension: Option<String>,
    ) -> Result<craft_core::nvme::NvmeNamespaceDescriptor> {
        match self
            .request(IpcRequest::NvmeCreateNamespace {
                nsid,
                size_mb,
                block_size,
                server_id,
                dimension,
            })
            .await?
        {
            IpcResponse::NvmeNamespaceResult { namespace } => Ok(namespace),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn delete_nvme_namespace(&mut self, nsid: u32) -> Result<bool> {
        match self.request(IpcRequest::NvmeDeleteNamespace { nsid }).await? {
            IpcResponse::NvmeDeleteResult { success, .. } => Ok(success),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn list_nvme_namespaces(
        &mut self,
        server: Option<String>,
    ) -> Result<Vec<craft_core::nvme::NvmeNamespaceDescriptor>> {
        match self.request(IpcRequest::NvmeListNamespaces { server }).await? {
            IpcResponse::NvmeNamespacesListResult { namespaces } => Ok(namespaces),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn list_nvme_subsystems(
        &mut self,
    ) -> Result<Vec<craft_core::nvme::NvmeSubsystemDescriptor>> {
        match self.request(IpcRequest::NvmeListSubsystems).await? {
            IpcResponse::NvmeSubsystemsListResult { subsystems } => Ok(subsystems),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn run_nvme_bench(
        &mut self,
        block_size: usize,
        iterations: usize,
    ) -> Result<craft_core::nvme::NvmeBenchmarkMetrics> {
        match self
            .request(IpcRequest::NvmeRunBench {
                block_size,
                iterations,
            })
            .await?
        {
            IpcResponse::NvmeBenchResult { metrics } => Ok(metrics),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn reset_nvme_metrics(&mut self) -> Result<String> {
        match self.request(IpcRequest::NvmeResetMetrics).await? {
            IpcResponse::NvmeResetResult { message } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_vpn_status(
        &mut self,
        server: Option<String>,
    ) -> Result<(craft_core::vpn::VpnStatusSummary, Vec<craft_core::vpn::VpnTunnelDescriptor>)> {
        match self.request(IpcRequest::VpnGetStatus { server }).await? {
            IpcResponse::VpnStatusResult { summary, tunnels } => Ok((summary, tunnels)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn create_vpn_tunnel(
        &mut self,
        tunnel_id: String,
        address: String,
        port: u16,
        crypto_mode: Option<String>,
    ) -> Result<craft_core::vpn::VpnTunnelDescriptor> {
        match self
            .request(IpcRequest::VpnCreateTunnel {
                tunnel_id,
                address,
                port,
                crypto_mode,
            })
            .await?
        {
            IpcResponse::VpnTunnelCreatedResult { tunnel } => Ok(tunnel),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn delete_vpn_tunnel(&mut self, tunnel_id: String) -> Result<bool> {
        match self.request(IpcRequest::VpnDeleteTunnel { tunnel_id }).await? {
            IpcResponse::VpnTunnelDeletedResult { success, .. } => Ok(success),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn add_vpn_peer(
        &mut self,
        tunnel_id: String,
        peer_id: String,
        endpoint: String,
        allowed_ips: Vec<String>,
    ) -> Result<craft_core::vpn::VpnPeerConfig> {
        match self
            .request(IpcRequest::VpnAddPeer {
                tunnel_id,
                peer_id,
                endpoint,
                allowed_ips,
            })
            .await?
        {
            IpcResponse::VpnPeerAddedResult { peer, .. } => Ok(peer),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn remove_vpn_peer(&mut self, tunnel_id: String, peer_id: String) -> Result<bool> {
        match self
            .request(IpcRequest::VpnRemovePeer { tunnel_id, peer_id })
            .await?
        {
            IpcResponse::VpnPeerRemovedResult { success, .. } => Ok(success),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn rotate_vpn_key(
        &mut self,
        tunnel_id: String,
        peer_id: Option<String>,
    ) -> Result<u64> {
        match self
            .request(IpcRequest::VpnRotateKey { tunnel_id, peer_id })
            .await?
        {
            IpcResponse::VpnKeyRotatedResult {
                renegotiation_latency_micros,
                ..
            } => Ok(renegotiation_latency_micros),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn run_vpn_bench(
        &mut self,
        iterations: usize,
        packet_size: usize,
    ) -> Result<craft_core::vpn::VpnBenchmarkMetrics> {
        match self
            .request(IpcRequest::VpnRunBench {
                iterations,
                packet_size,
            })
            .await?
        {
            IpcResponse::VpnBenchResult { metrics } => Ok(metrics),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn reset_vpn_metrics(&mut self) -> Result<String> {
        match self.request(IpcRequest::VpnResetMetrics).await? {
            IpcResponse::VpnResetResult { message } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn get_bft_status(&mut self, server: Option<String>) -> Result<craft_core::BftStatusSummary> {
        match self.request(IpcRequest::BftGetStatus { server }).await? {
            IpcResponse::BftStatusResult { summary } => Ok(summary),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn submit_bft_transaction(
        &mut self,
        tx_type: String,
        payload: String,
        sender: Option<String>,
    ) -> Result<(String, u64, String)> {
        match self
            .request(IpcRequest::BftSubmitTransaction {
                tx_type,
                payload,
                sender,
            })
            .await?
        {
            IpcResponse::BftTransactionSubmittedResult { tx_id, view, status } => Ok((tx_id, view, status)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn list_bft_validators(&mut self, server: Option<String>) -> Result<Vec<craft_core::BftValidator>> {
        match self.request(IpcRequest::BftListValidators { server }).await? {
            IpcResponse::BftValidatorsListResult { validators } => Ok(validators),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn add_bft_validator(
        &mut self,
        node_id: String,
        address: String,
        public_key: String,
        stake: u64,
    ) -> Result<String> {
        match self
            .request(IpcRequest::BftAddValidator {
                node_id,
                address,
                public_key,
                stake,
            })
            .await?
        {
            IpcResponse::BftValidatorAddedResult { message, .. } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn remove_bft_validator(&mut self, node_id: String) -> Result<String> {
        match self.request(IpcRequest::BftRemoveValidator { node_id }).await? {
            IpcResponse::BftValidatorRemovedResult { message, .. } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn trigger_bft_view_change(&mut self, reason: String) -> Result<(u64, u64, String)> {
        match self.request(IpcRequest::BftTriggerViewChange { reason }).await? {
            IpcResponse::BftViewChangedResult { from_view, to_view, new_proposer } => {
                Ok((from_view, to_view, new_proposer))
            }
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn verify_bft_zk_proof(&mut self, proof_json: String) -> Result<(bool, String)> {
        match self.request(IpcRequest::BftVerifyZkProof { proof_json }).await? {
            IpcResponse::BftZkProofVerifiedResult { valid, message } => Ok((valid, message)),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn run_bft_bench(
        &mut self,
        iterations: u32,
        validators: u32,
    ) -> Result<craft_core::BftBenchmarkMetrics> {
        match self
            .request(IpcRequest::BftRunBench {
                iterations,
                validators,
            })
            .await?
        {
            IpcResponse::BftBenchResult { metrics } => Ok(metrics),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }

    pub async fn reset_bft_metrics(&mut self) -> Result<String> {
        match self.request(IpcRequest::BftResetMetrics).await? {
            IpcResponse::BftResetResult { message } => Ok(message),
            IpcResponse::Error { error } => Err(CraftError::Other(error)),
            _ => Err(CraftError::Ipc("Unexpected response from daemon".to_string())),
        }
    }
}


async fn run_storage_monitor(paths: CraftPaths, threshold_bytes: u64, threshold_percent: f64) {
    use sysinfo::Disks;
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(300));
    let mut last_alert: Option<tokio::time::Instant> = None;

    loop {
        interval.tick().await;

        let disks = Disks::new_with_refreshed_list();
        let craft_dir = paths.home.clone();

        let mut matched_mount: Option<std::path::PathBuf> = None;
        let mut matched_avail = 0u64;
        let mut matched_total = 0u64;
        let mut longest_match = 0;

        for disk in &disks {
            let mount = disk.mount_point();
            if craft_dir.starts_with(mount) {
                let len = mount.as_os_str().len();
                if len >= longest_match {
                    longest_match = len;
                    matched_mount = Some(mount.to_path_buf());
                    matched_avail = disk.available_space();
                    matched_total = disk.total_space();
                }
            }
        }

        if let Some(mount) = matched_mount {
            let percent_free = if matched_total > 0 {
                (matched_avail as f64 / matched_total as f64) * 100.0
            } else {
                100.0
            };

            let is_low_bytes = matched_avail < threshold_bytes;
            let is_low_percent = percent_free < threshold_percent;

            if is_low_bytes || is_low_percent {
                let should_alert = match last_alert {
                    None => true,
                    Some(last) => last.elapsed() >= std::time::Duration::from_secs(3600),
                };

                if should_alert {
                    warn!(
                        "[WARN] Storage exhaustion alert: Disk '{}' has only {} ({:.1}%) free space remaining",
                        mount.display(),
                        craft_core::format_size(matched_avail),
                        percent_free
                    );

                    let payload = crate::webhooks::WebhookPayload::storage_exhaustion(
                        &mount,
                        matched_avail,
                        matched_total,
                        percent_free,
                    );
                    crate::webhooks::WebhookDispatcher::dispatch(payload, &paths);

                    let mut storage_ctx = craft_scripting::HookContext::new(craft_scripting::LifecycleEvent::StorageLow);
                    storage_ctx.free_bytes = Some(matched_avail);
                    storage_ctx.total_bytes = Some(matched_total);
                    storage_ctx.details = Some(format!(
                        "Disk '{}' has only {:.1}% free space remaining",
                        mount.display(),
                        percent_free
                    ));
                    craft_scripting::HookBus::dispatch_async(
                        paths.clone(),
                        craft_scripting::LifecycleEvent::StorageLow,
                        storage_ctx,
                        10,
                    );

                    last_alert = Some(tokio::time::Instant::now());
                }
            }
        }
    }
}
