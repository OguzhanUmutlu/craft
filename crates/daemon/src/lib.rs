pub mod circuit_breaker;
pub mod gateway;
pub mod ipc;
pub mod protocol;
pub mod ring_buffer;
pub mod scheduler;
pub mod supervisor;
pub mod telemetry;
pub mod webhooks;

pub use circuit_breaker::{CircuitBreakerInfo, CircuitDecision, CircuitState, CrashCircuitBreaker};
pub use gateway::GatewayServer;
pub use ipc::{DaemonClient, DaemonServer, DAEMON_PORT};
pub use protocol::{IpcRequest, IpcResponse};
pub use ring_buffer::RingBuffer;
pub use scheduler::{BackupSchedule, BackupScheduleInfo, DaemonScheduler};
pub use supervisor::Supervisor;
pub use telemetry::{generate_prometheus_metrics, init_telemetry_start_time};
pub use webhooks::{WebhookDispatcher, WebhookPayload};
