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
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let sup = supervisor.clone();
                        let hib = hib_mgr.clone();
                        let ap = ap_mgr.clone();
                        let eb = eb_mgr.clone();
                        let ts = ts_mgr.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, sup, hib, ap, eb, ts).await {
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
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(client, sup, hib, ap, eb, ts).await {
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
