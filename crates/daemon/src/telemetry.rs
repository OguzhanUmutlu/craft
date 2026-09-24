use crate::supervisor::Supervisor;
use craft_core::{CraftPaths, ServersRegistry};
use craft_net::{ping_server_auto, UniversalPingStatus};
use std::fmt::Write;
use std::time::{Duration, Instant};
use sysinfo::{Disks, Pid, ProcessesToUpdate, System};

static DAEMON_START_TIME: std::sync::OnceLock<Instant> = std::sync::OnceLock::new();

pub fn init_telemetry_start_time() {
    DAEMON_START_TIME.get_or_init(Instant::now);
}

pub fn get_daemon_uptime_seconds() -> u64 {
    let start = DAEMON_START_TIME.get_or_init(Instant::now);
    start.elapsed().as_secs()
}

pub async fn generate_prometheus_metrics(
    supervisor: &Supervisor,
    paths: &CraftPaths,
) -> String {
    let mut out = String::with_capacity(4096);

    // 1. Host and System Telemetry via sysinfo
    let mut sys = System::new();
    sys.refresh_cpu_all();
    sys.refresh_memory();
    sys.refresh_processes(ProcessesToUpdate::All, true);

    let host_cpu = sys.global_cpu_usage();
    let host_mem_total = sys.total_memory();
    let host_mem_used = sys.used_memory();

    let disks = Disks::new_with_refreshed_list();
    let mut host_disk_total = 0u64;
    let mut host_disk_available = 0u64;
    for disk in &disks {
        host_disk_total += disk.total_space();
        host_disk_available += disk.available_space();
    }

    let uptime = get_daemon_uptime_seconds();

    writeln!(out, "# HELP craft_daemon_uptime_seconds Daemon process uptime in seconds").unwrap();
    writeln!(out, "# TYPE craft_daemon_uptime_seconds counter").unwrap();
    writeln!(out, "craft_daemon_uptime_seconds {}", uptime).unwrap();

    writeln!(out, "# HELP craft_host_cpu_usage_percent Global host CPU utilization percentage").unwrap();
    writeln!(out, "# TYPE craft_host_cpu_usage_percent gauge").unwrap();
    writeln!(out, "craft_host_cpu_usage_percent {:.2}", host_cpu).unwrap();

    writeln!(out, "# HELP craft_host_memory_total_bytes Total host physical memory in bytes").unwrap();
    writeln!(out, "# TYPE craft_host_memory_total_bytes gauge").unwrap();
    writeln!(out, "craft_host_memory_total_bytes {}", host_mem_total).unwrap();

    writeln!(out, "# HELP craft_host_memory_used_bytes Used host physical memory in bytes").unwrap();
    writeln!(out, "# TYPE craft_host_memory_used_bytes gauge").unwrap();
    writeln!(out, "craft_host_memory_used_bytes {}", host_mem_used).unwrap();

    writeln!(out, "# HELP craft_host_disk_total_bytes Total host storage capacity in bytes").unwrap();
    writeln!(out, "# TYPE craft_host_disk_total_bytes gauge").unwrap();
    writeln!(out, "craft_host_disk_total_bytes {}", host_disk_total).unwrap();

    writeln!(out, "# HELP craft_host_disk_available_bytes Available host storage capacity in bytes").unwrap();
    writeln!(out, "# TYPE craft_host_disk_available_bytes gauge").unwrap();
    writeln!(out, "craft_host_disk_available_bytes {}", host_disk_available).unwrap();

    // 2. Server Specific Metrics
    let registry = ServersRegistry::load(paths).unwrap_or_default();
    let running_paths = supervisor.get_running_paths().await;
    let cb_infos = supervisor.get_circuit_breaker_infos().await;

    writeln!(out, "# HELP craft_daemon_managed_servers_total Total registered servers managed by daemon").unwrap();
    writeln!(out, "# TYPE craft_daemon_managed_servers_total gauge").unwrap();
    writeln!(out, "craft_daemon_managed_servers_total {}", registry.servers.len()).unwrap();

    writeln!(out, "# HELP craft_server_status Process execution status (1 = running, 0 = stopped)").unwrap();
    writeln!(out, "# TYPE craft_server_status gauge").unwrap();

    writeln!(out, "# HELP craft_server_cpu_percent Process CPU utilization percentage").unwrap();
    writeln!(out, "# TYPE craft_server_cpu_percent gauge").unwrap();

    writeln!(out, "# HELP craft_server_memory_rss_bytes Process resident memory (RSS) in bytes").unwrap();
    writeln!(out, "# TYPE craft_server_memory_rss_bytes gauge").unwrap();

    writeln!(out, "# HELP craft_server_players_online Active connected player count").unwrap();
    writeln!(out, "# TYPE craft_server_players_online gauge").unwrap();

    writeln!(out, "# HELP craft_server_players_max Maximum player capacity").unwrap();
    writeln!(out, "# TYPE craft_server_players_max gauge").unwrap();

    writeln!(out, "# HELP craft_server_query_latency_ms Protocol ping latency in milliseconds").unwrap();
    writeln!(out, "# TYPE craft_server_query_latency_ms gauge").unwrap();

    writeln!(out, "# HELP craft_server_crash_count Count of recorded process crashes").unwrap();
    writeln!(out, "# TYPE craft_server_crash_count counter").unwrap();

    writeln!(out, "# HELP craft_server_circuit_breaker_state Circuit breaker state (0 = Closed, 1 = HalfOpen, 2 = Open)").unwrap();
    writeln!(out, "# TYPE craft_server_circuit_breaker_state gauge").unwrap();

    for server in &registry.servers {
        let canonical = server.path.canonicalize().unwrap_or_else(|_| server.path.clone());
        let is_running = running_paths.iter().any(|p| {
            p.canonicalize().unwrap_or_else(|_| p.clone()) == canonical
        });

        let status_val = if is_running { 1 } else { 0 };
        let s_name = sanitize_label(&server.name);
        let s_sw = sanitize_label(&server.software);
        let s_ver = sanitize_label(&server.version);

        writeln!(
            out,
            "craft_server_status{{server=\"{}\",software=\"{}\",version=\"{}\"}} {}",
            s_name, s_sw, s_ver, status_val
        ).unwrap();

        if is_running {
            if let Some(pid) = supervisor.get_server_pid(&canonical).await {
                if let Some(proc) = sys.process(Pid::from(pid as usize)) {
                    writeln!(out, "craft_server_cpu_percent{{server=\"{}\"}} {:.2}", s_name, proc.cpu_usage()).unwrap();
                    writeln!(out, "craft_server_memory_rss_bytes{{server=\"{}\"}} {}", s_name, proc.memory()).unwrap();
                }
            }

            if let Some(port) = server.port {
                let game_def = server.game_definition();
                let ping_res = tokio::time::timeout(
                    Duration::from_millis(1500),
                    ping_server_auto("127.0.0.1", port, Some(game_def.query_protocol)),
                ).await;

                if let Ok(Ok(status)) = ping_res {
                    match status {
                        UniversalPingStatus::MinecraftJava(j) => {
                            writeln!(out, "craft_server_players_online{{server=\"{}\"}} {}", s_name, j.online_players).unwrap();
                            writeln!(out, "craft_server_players_max{{server=\"{}\"}} {}", s_name, j.max_players).unwrap();
                            writeln!(out, "craft_server_query_latency_ms{{server=\"{}\"}} {}", s_name, j.latency_ms).unwrap();
                        }
                        UniversalPingStatus::MinecraftBedrock(b) => {
                            writeln!(out, "craft_server_players_online{{server=\"{}\"}} {}", s_name, b.online_players).unwrap();
                            writeln!(out, "craft_server_players_max{{server=\"{}\"}} {}", s_name, b.max_players).unwrap();
                            writeln!(out, "craft_server_query_latency_ms{{server=\"{}\"}} {}", s_name, b.latency_ms).unwrap();
                        }
                        UniversalPingStatus::ValveA2S(a) => {
                            writeln!(out, "craft_server_players_online{{server=\"{}\"}} {}", s_name, a.online_players).unwrap();
                            writeln!(out, "craft_server_players_max{{server=\"{}\"}} {}", s_name, a.max_players).unwrap();
                            writeln!(out, "craft_server_query_latency_ms{{server=\"{}\"}} {}", s_name, a.latency_ms).unwrap();
                        }
                        UniversalPingStatus::PortProbe { latency_ms, .. } => {
                            writeln!(out, "craft_server_query_latency_ms{{server=\"{}\"}} {}", s_name, latency_ms).unwrap();
                        }
                    }
                }
            }
        }

        // Circuit breaker metrics
        let cb_match = cb_infos.iter().find(|cb| cb.server_name == server.name);
        let (crash_count, cb_state) = if let Some(cb) = cb_match {
            let state_num = match cb.state.to_lowercase().as_str() {
                "closed" => 0,
                "halfopen" | "half_open" | "half-open" => 1,
                _ => 2,
            };
            (cb.consecutive_crashes, state_num)
        } else {
            (0, 0)
        };

        writeln!(out, "craft_server_crash_count{{server=\"{}\"}} {}", s_name, crash_count).unwrap();
        writeln!(out, "craft_server_circuit_breaker_state{{server=\"{}\"}} {}", s_name, cb_state).unwrap();
    }

    out.push_str(&crate::tracing_service::TracingService::global(paths).generate_prometheus_metrics());
    out.push_str(&crate::anvil_service::AnvilService::global(paths).generate_prometheus_metrics());
    out.push_str(&crate::dpdk_service::DpdkNumaService::global(paths).format_prometheus_metrics().await);
    out.push_str(&crate::multi_raft_service::MultiRaftService::global(paths).generate_prometheus_metrics());
    out.push_str(&crate::migration_service::MigrationService::global(paths).generate_prometheus_metrics());
    out.push_str(&crate::ebpf_service::EbpfObservabilityService::global(paths).generate_prometheus_metrics());
    out.push_str(&crate::supply_chain_service::SupplyChainService::global(paths).generate_prometheus_metrics());
    out.push_str(&crate::pqc_service::PqcService::global(paths).generate_prometheus_metrics());
    out.push_str(&crate::hsm_service::HsmService::global(paths).generate_prometheus_metrics());
    out.push_str(&crate::compaction_service::CompactionService::global(paths).generate_prometheus_metrics());
    out.push_str(&crate::xdp_service::XdpService::global(paths).generate_prometheus_metrics());
    out.push_str(&crate::pmu_service::PmuService::global(paths).generate_prometheus_metrics());
    out.push_str(&crate::shm_service::ShmService::global(paths).generate_prometheus_metrics());

    out
}

fn sanitize_label(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace('"', "\\\"")
        .replace('\n', "\\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_label_sanitization() {
        assert_eq!(sanitize_label("simple"), "simple");
        assert_eq!(sanitize_label("with \"quotes\""), "with \\\"quotes\\\"");
        assert_eq!(sanitize_label("path\\test"), "path\\\\test");
    }

    #[test]
    fn test_daemon_uptime_counter() {
        init_telemetry_start_time();
        let up = get_daemon_uptime_seconds();
        assert!(up < 1000);
    }
}
