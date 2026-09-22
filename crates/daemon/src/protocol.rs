use crate::circuit_breaker::CircuitBreakerInfo;
use crate::scheduler::BackupScheduleInfo;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum IpcRequest {
    Ping,
    StartServer { path: PathBuf },
    StopServer { path: PathBuf, force: bool },
    GetRunning,
    AttachConsole { path: PathBuf },
    SendInput { path: PathBuf, input: String },
    DetachConsole { path: PathBuf },
    ShutdownDaemon,
    GetCircuitBreakers,
    ResetCircuitBreaker { path: PathBuf },
    GetBackupSchedules,
    HibernateServer { server_name: String },
    WakeServer { server_name: String },
    GetAutoscaleStatus,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AutoscaleServerStatus {
    pub server_name: String,
    pub enabled: bool,
    pub is_sleeping: bool,
    pub idle_timeout_mins: u64,
    pub idle_seconds: u64,
    pub player_count: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "payload")]
pub enum IpcResponse {
    Pong,
    Success { message: String },
    AlreadyRunning { path: PathBuf },
    NotRunning { path: PathBuf },
    RunningList { paths: Vec<PathBuf> },
    LogBacklog { path: PathBuf, data: String },
    LogChunk { path: PathBuf, data: String },
    CircuitBreakersList { items: Vec<CircuitBreakerInfo> },
    BackupSchedulesList { items: Vec<BackupScheduleInfo> },
    AutoscaleStatusList { items: Vec<AutoscaleServerStatus> },
    Error { error: String },
}
