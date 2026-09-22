use crate::protocol::AutoscaleServerStatus;
use crate::supervisor::Supervisor;
use craft_core::{AutoscaleRegistry, CraftError, CraftPaths, Result, ServersRegistry};
use craft_net::{ping_server_auto, SleepProxy, SleepProxyConfig, SleepProxyHandle, UniversalPingStatus};
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tokio::time::{sleep, Duration, Instant};
use tracing::{debug, error, info, warn};

pub struct HibernationManager {
    paths: CraftPaths,
    supervisor: Supervisor,
    proxies: Arc<Mutex<HashMap<String, SleepProxyHandle>>>,
    idle_tracker: Arc<Mutex<HashMap<String, (Instant, u32)>>>,
    wake_tx: mpsc::Sender<String>,
}

fn extract_player_count(status: &UniversalPingStatus) -> u32 {
    match status {
        UniversalPingStatus::MinecraftJava(s) => s.online_players,
        UniversalPingStatus::MinecraftBedrock(s) => s.online_players,
        UniversalPingStatus::ValveA2S(s) => s.online_players as u32,
        UniversalPingStatus::PortProbe { .. } => 0,
    }
}

impl HibernationManager {
    pub fn start(paths: CraftPaths, supervisor: Supervisor) -> Arc<Self> {
        let (wake_tx, mut wake_rx) = mpsc::channel::<String>(32);
        let proxies = Arc::new(Mutex::new(HashMap::new()));
        let idle_tracker = Arc::new(Mutex::new(HashMap::new()));

        let manager = Arc::new(Self {
            paths: paths.clone(),
            supervisor: supervisor.clone(),
            proxies: proxies.clone(),
            idle_tracker: idle_tracker.clone(),
            wake_tx: wake_tx.clone(),
        });

        // 1. Packet-triggered wake listener worker
        let sup_wake = supervisor.clone();
        let paths_wake = paths.clone();
        let proxies_wake = proxies.clone();
        tokio::spawn(async move {
            while let Some(server_name) = wake_rx.recv().await {
                info!(
                    server = %server_name,
                    "Wake packet received from SleepProxy! Initiating server wake-up."
                );

                // Stop sleep proxy to release the port
                {
                    let mut px_map = proxies_wake.lock().await;
                    if let Some(handle) = px_map.remove(&server_name) {
                        handle.shutdown();
                    }
                }

                // Short sleep for socket reuse
                sleep(Duration::from_millis(300)).await;

                // Start server
                if let Ok(registry) = ServersRegistry::load(&paths_wake) {
                    if let Some(server) = registry.find_by_name(&server_name) {
                        if let Err(e) = sup_wake.start_server(&server.path).await {
                            error!(
                                server = %server_name,
                                error = %e,
                                "Failed to wake up server via supervisor"
                            );
                        } else {
                            info!(server = %server_name, "Server successfully woken up!");
                        }
                    }
                }
            }
        });

        // 2. Periodic idle inspection and hibernation loop (every 15s)
        let mgr_idle = manager.clone();
        tokio::spawn(async move {
            loop {
                sleep(Duration::from_secs(15)).await;
                mgr_idle.inspect_and_reap_idle_servers().await;
            }
        });

        manager
    }

    pub async fn inspect_and_reap_idle_servers(&self) {
        let autoscale_reg = match AutoscaleRegistry::load(&self.paths) {
            Ok(reg) => reg,
            Err(_) => return,
        };

        let servers_reg = match ServersRegistry::load(&self.paths) {
            Ok(reg) => reg,
            Err(_) => return,
        };

        for policy in autoscale_reg.policies {
            if !policy.hibernation_enabled {
                continue;
            }

            let server_name = policy.server_name.clone();
            let server = match servers_reg.find_by_name(&server_name) {
                Some(s) => s,
                None => continue,
            };

            let is_running = self.supervisor.is_running(&server.path).await;
            let port = server.port.unwrap_or(25565);

            if is_running {
                // Query server online player count
                let player_count = match ping_server_auto("127.0.0.1", port, None).await {
                    Ok(ref status) => extract_player_count(status),
                    Err(_) => 0,
                };

                let mut tracker = self.idle_tracker.lock().await;
                let now = Instant::now();

                if player_count == 0 {
                    let (since, _) = tracker.entry(server_name.clone()).or_insert((now, 0));
                    let idle_duration = now.duration_since(*since);
                    let idle_threshold = Duration::from_secs(policy.idle_timeout_mins * 60);

                    if idle_duration >= idle_threshold {
                        info!(
                            server = %server_name,
                            idle_mins = idle_duration.as_secs() / 60,
                            "Server has been idle exceeding threshold! Hibernating..."
                        );
                        drop(tracker);

                        if let Err(e) = self.hibernate_server(&server_name).await {
                            error!(server = %server_name, error = %e, "Failed to hibernate idle server");
                        }
                    } else {
                        debug!(
                            server = %server_name,
                            idle_secs = idle_duration.as_secs(),
                            threshold_secs = idle_threshold.as_secs(),
                            "Server is idle, waiting for threshold"
                        );
                    }
                } else {
                    // Reset idle tracker
                    tracker.insert(server_name.clone(), (now, player_count));
                }
            }
        }
    }

    pub async fn hibernate_server(&self, server_name: &str) -> Result<()> {
        let servers_reg = ServersRegistry::load(&self.paths)?;
        let server = servers_reg.find_by_name(server_name).ok_or_else(|| {
            CraftError::Other(format!("Server '{}' not found in registry", server_name))
        })?;

        let autoscale_reg = AutoscaleRegistry::load(&self.paths).unwrap_or_default();
        let policy_opt = autoscale_reg.get_policy(server_name);
        let motd = policy_opt
            .and_then(|p| p.sleep_motd.clone())
            .unwrap_or_else(|| "[Craft] Server is sleeping. Connect to wake up!".to_string());

        let port = server.port.unwrap_or(25565);

        // 1. If running, stop via supervisor
        if self.supervisor.is_running(&server.path).await {
            info!(server = %server_name, "Stopping running server for hibernation");
            let _ = self.supervisor.stop_server(&server.path, false).await;
            // Wait up to 10 seconds for process termination
            for _ in 0..20 {
                if !self.supervisor.is_running(&server.path).await {
                    break;
                }
                sleep(Duration::from_millis(500)).await;
            }
        }

        // 2. Start SleepProxy on the server's port
        let mut proxies = self.proxies.lock().await;
        if let Some(existing) = proxies.remove(server_name) {
            existing.shutdown();
            sleep(Duration::from_millis(200)).await;
        }

        let proxy_config = SleepProxyConfig {
            bind_addr: SocketAddr::new(IpAddr::V4(Ipv4Addr::UNSPECIFIED), port),
            server_name: server_name.to_string(),
            motd,
            version_name: "Craft Hibernation".to_string(),
            protocol_version: 765,
            wake_kick_message: "[Craft] Server is starting up! Please reconnect in 15 seconds.".to_string(),
        };

        match SleepProxy::start(proxy_config, self.wake_tx.clone()).await {
            Ok(handle) => {
                proxies.insert(server_name.to_string(), handle);
                info!(
                    server = %server_name,
                    port = port,
                    "SleepProxy bound and active for hibernated server"
                );
                Ok(())
            }
            Err(e) => {
                warn!(
                    server = %server_name,
                    port = port,
                    error = %e,
                    "Failed to bind SleepProxy during hibernation"
                );
                Err(e)
            }
        }
    }

    pub async fn wake_server(&self, server_name: &str) -> Result<()> {
        let servers_reg = ServersRegistry::load(&self.paths)?;
        let server = servers_reg.find_by_name(server_name).ok_or_else(|| {
            CraftError::Other(format!("Server '{}' not found in registry", server_name))
        })?;

        // 1. Shut down SleepProxy if running
        {
            let mut proxies = self.proxies.lock().await;
            if let Some(handle) = proxies.remove(server_name) {
                handle.shutdown();
                sleep(Duration::from_millis(300)).await;
            }
        }

        // 2. Start server via supervisor
        self.supervisor.start_server(&server.path).await?;
        info!(server = %server_name, "Server woken up from hibernation");

        // 3. Reset idle tracker
        let mut tracker = self.idle_tracker.lock().await;
        tracker.insert(server_name.to_string(), (Instant::now(), 1));

        Ok(())
    }

    pub async fn get_autoscale_status(&self) -> Result<Vec<AutoscaleServerStatus>> {
        let autoscale_reg = AutoscaleRegistry::load(&self.paths).unwrap_or_default();
        let servers_reg = ServersRegistry::load(&self.paths).unwrap_or_default();

        let proxies = self.proxies.lock().await;
        let tracker = self.idle_tracker.lock().await;
        let now = Instant::now();

        let mut results = Vec::new();

        for policy in autoscale_reg.policies {
            let name = policy.server_name;
            let is_sleeping = proxies.contains_key(&name);
            let (idle_seconds, player_count) = if let Some((since, count)) = tracker.get(&name) {
                (now.duration_since(*since).as_secs(), *count)
            } else {
                (0, 0)
            };

            results.push(AutoscaleServerStatus {
                server_name: name,
                enabled: policy.hibernation_enabled,
                is_sleeping,
                idle_timeout_mins: policy.idle_timeout_mins,
                idle_seconds,
                player_count,
            });
        }

        // Also check any registered servers not in autoscale config
        for srv in servers_reg.servers {
            if !results.iter().any(|r| r.server_name == srv.name) {
                let is_sleeping = proxies.contains_key(&srv.name);
                let (idle_seconds, player_count) = if let Some((since, count)) = tracker.get(&srv.name) {
                    (now.duration_since(*since).as_secs(), *count)
                } else {
                    (0, 0)
                };

                results.push(AutoscaleServerStatus {
                    server_name: srv.name,
                    enabled: false,
                    is_sleeping,
                    idle_timeout_mins: 15,
                    idle_seconds,
                    player_count,
                });
            }
        }

        Ok(results)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_autoscale_status_struct() {
        let status = AutoscaleServerStatus {
            server_name: "survival".to_string(),
            enabled: true,
            is_sleeping: false,
            idle_timeout_mins: 15,
            idle_seconds: 42,
            player_count: 3,
        };
        assert_eq!(status.server_name, "survival");
        assert!(status.enabled);
        assert!(!status.is_sleeping);
        assert_eq!(status.player_count, 3);
    }
}
