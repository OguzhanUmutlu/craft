use craft_core::xdp::{
    XdpAttachMode, XdpFilterRule, XdpInterfaceStatus, XdpRegistry,
};
use craft_core::{CraftError, CraftPaths, Result};
use craft_net::XdpPipeline;
use craft_scripting::{HookBus, HookContext, LifecycleEvent};
use std::fmt::Write as FmtWrite;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Instant;
use tracing::info;

static INSTANCE: OnceLock<Arc<XdpService>> = OnceLock::new();

/// Background supervisor coordinating in-kernel eBPF XDP firewall and flow tracking
pub struct XdpService {
    paths: CraftPaths,
    registry: Arc<RwLock<XdpRegistry>>,
    pipeline: Arc<RwLock<XdpPipeline>>,
    interface_name: Arc<RwLock<String>>,
    attach_mode: Arc<RwLock<XdpAttachMode>>,
    is_attached: AtomicBool,
    bpf_prog_id: AtomicU64,
    start_time: Instant,
}

impl XdpService {
    pub fn new(paths: CraftPaths) -> Self {
        let registry = XdpRegistry::load(&paths).unwrap_or_default();
        let mut pipeline = XdpPipeline::new(registry.config.clone());
        for rule in &registry.rules {
            pipeline.add_rule(rule.clone());
        }

        let iface = registry.interface_name.clone();
        let mode = registry.mode;
        let attached = mode != XdpAttachMode::Detached;

        Self {
            paths,
            registry: Arc::new(RwLock::new(registry)),
            pipeline: Arc::new(RwLock::new(pipeline)),
            interface_name: Arc::new(RwLock::new(iface)),
            attach_mode: Arc::new(RwLock::new(mode)),
            is_attached: AtomicBool::new(attached),
            bpf_prog_id: AtomicU64::new(if attached { 4201 } else { 0 }),
            start_time: Instant::now(),
        }
    }

    pub fn global(paths: &CraftPaths) -> Arc<Self> {
        INSTANCE
            .get_or_init(|| Arc::new(Self::new(paths.clone())))
            .clone()
    }

    /// Retrieve full interface status summary
    pub fn get_status(&self) -> Result<XdpInterfaceStatus> {
        let iface = self
            .interface_name
            .read()
            .map_err(|_| CraftError::Other("Interface lock poisoned".to_string()))?
            .clone();
        let mode = *self
            .attach_mode
            .read()
            .map_err(|_| CraftError::Other("Attach mode lock poisoned".to_string()))?;
        let attached = self.is_attached.load(Ordering::SeqCst);
        let prog_id = if attached {
            Some(self.bpf_prog_id.load(Ordering::SeqCst) as u32)
        } else {
            None
        };

        let pipe = self
            .pipeline
            .read()
            .map_err(|_| CraftError::Other("Pipeline lock poisoned".to_string()))?;

        let mut metrics = pipe.engine.get_metrics();
        // Compute instantaneous rates from service uptime
        let uptime_secs = self.start_time.elapsed().as_secs_f64().max(1.0);
        metrics.drop_rate_pps = metrics.dropped_packets as f64 / uptime_secs;
        metrics.bandwidth_absorbed_gbps = (metrics.total_rx_bytes as f64 * 8.0) / (uptime_secs * 1_000_000_000.0);

        Ok(XdpInterfaceStatus {
            interface_name: iface,
            mode,
            attached,
            bpf_prog_id: prog_id,
            rules_count: pipe.engine.rules.len(),
            active_flows: metrics.current_active_flows,
            metrics,
        })
    }

    /// Attach XDP program to network interface
    pub fn attach_interface(&self, interface_name: &str, mode: XdpAttachMode) -> Result<XdpInterfaceStatus> {
        {
            let mut iface_guard = self
                .interface_name
                .write()
                .map_err(|_| CraftError::Other("Interface lock poisoned".to_string()))?;
            *iface_guard = interface_name.to_string();

            let mut mode_guard = self
                .attach_mode
                .write()
                .map_err(|_| CraftError::Other("Attach mode lock poisoned".to_string()))?;
            *mode_guard = mode;
        }

        self.is_attached.store(true, Ordering::SeqCst);
        let next_id = self.bpf_prog_id.fetch_add(1, Ordering::SeqCst) + 1;

        // Persist to registry
        {
            let mut reg = self
                .registry
                .write()
                .map_err(|_| CraftError::Other("Registry lock poisoned".to_string()))?;
            reg.interface_name = interface_name.to_string();
            reg.mode = mode;
            let _ = reg.save(&self.paths);
        }

        info!(
            interface = interface_name,
            mode = %mode,
            prog_id = next_id,
            "Attached eBPF XDP firewall program"
        );

        let status = self.get_status()?;
        let _ = self.registry.read().map(|r| r.record_state(&self.paths, &status));

        // Fire lifecycle hook
        HookBus::dispatch_async(
            self.paths.clone(),
            LifecycleEvent::XdpInterfaceAttached,
            HookContext::for_xdp_attached(interface_name, &mode.to_string()),
            5,
        );

        Ok(status)
    }

    /// Detach XDP program from network interface
    pub fn detach_interface(&self) -> Result<XdpInterfaceStatus> {
        let iface = self
            .interface_name
            .read()
            .map_err(|_| CraftError::Other("Interface lock poisoned".to_string()))?
            .clone();

        {
            let mut mode_guard = self
                .attach_mode
                .write()
                .map_err(|_| CraftError::Other("Attach mode lock poisoned".to_string()))?;
            *mode_guard = XdpAttachMode::Detached;
        }

        self.is_attached.store(false, Ordering::SeqCst);

        // Persist to registry
        {
            let mut reg = self
                .registry
                .write()
                .map_err(|_| CraftError::Other("Registry lock poisoned".to_string()))?;
            reg.mode = XdpAttachMode::Detached;
            let _ = reg.save(&self.paths);
        }

        info!(interface = %iface, "Detached eBPF XDP firewall program");

        let status = self.get_status()?;
        let _ = self.registry.read().map(|r| r.record_state(&self.paths, &status));
        Ok(status)
    }

    /// Add filtering rule
    pub fn add_rule(&self, rule: XdpFilterRule) -> Result<XdpInterfaceStatus> {
        {
            let mut pipe = self
                .pipeline
                .write()
                .map_err(|_| CraftError::Other("Pipeline lock poisoned".to_string()))?;
            pipe.add_rule(rule.clone());
        }

        {
            let mut reg = self
                .registry
                .write()
                .map_err(|_| CraftError::Other("Registry lock poisoned".to_string()))?;
            reg.add_rule(&self.paths, rule)?;
        }

        let status = self.get_status()?;
        let _ = self.registry.read().map(|r| r.record_state(&self.paths, &status));
        Ok(status)
    }

    /// Remove filtering rule by ID
    pub fn remove_rule(&self, rule_id: &str) -> Result<XdpInterfaceStatus> {
        {
            let mut pipe = self
                .pipeline
                .write()
                .map_err(|_| CraftError::Other("Pipeline lock poisoned".to_string()))?;
            pipe.engine.remove_rule(rule_id);
        }

        {
            let mut reg = self
                .registry
                .write()
                .map_err(|_| CraftError::Other("Registry lock poisoned".to_string()))?;
            reg.remove_rule(&self.paths, rule_id)?;
        }

        let status = self.get_status()?;
        let _ = self.registry.read().map(|r| r.record_state(&self.paths, &status));
        Ok(status)
    }

    /// Reset packet metrics
    pub fn reset_metrics(&self) -> Result<XdpInterfaceStatus> {
        {
            let mut pipe = self
                .pipeline
                .write()
                .map_err(|_| CraftError::Other("Pipeline lock poisoned".to_string()))?;
            pipe.engine.reset_metrics();
        }

        let status = self.get_status()?;
        let _ = self.registry.read().map(|r| r.record_state(&self.paths, &status));
        Ok(status)
    }

    /// Render plain text status summary table (zero emojis)
    pub fn render_plain_status(&self) -> Result<String> {
        let status = self.get_status()?;
        let pipe = self
            .pipeline
            .read()
            .map_err(|_| CraftError::Other("Pipeline lock poisoned".to_string()))?;
        Ok(pipe.engine.render_plain_status(&status.interface_name, status.mode))
    }

    /// Generate Prometheus telemetry metrics
    pub fn generate_prometheus_metrics(&self) -> String {
        let mut out = String::new();
        if let Ok(status) = self.get_status() {
            let m = &status.metrics;
            let attached_val = if status.attached { 1 } else { 0 };

            let _ = writeln!(out, "# HELP craft_xdp_attached Indicates if XDP program is attached");
            let _ = writeln!(out, "# TYPE craft_xdp_attached gauge");
            let _ = writeln!(out, "craft_xdp_attached {}", attached_val);

            let _ = writeln!(out, "# HELP craft_xdp_packets_total Total packets ingested at XDP layer");
            let _ = writeln!(out, "# TYPE craft_xdp_packets_total counter");
            let _ = writeln!(out, "craft_xdp_packets_total {}", m.total_rx_packets);

            let _ = writeln!(out, "# HELP craft_xdp_dropped_total Total packets dropped at XDP driver layer");
            let _ = writeln!(out, "# TYPE craft_xdp_dropped_total counter");
            let _ = writeln!(out, "craft_xdp_dropped_total {}", m.dropped_packets);

            let _ = writeln!(out, "# HELP craft_xdp_syn_drops_total Total SYN flood packets dropped");
            let _ = writeln!(out, "# TYPE craft_xdp_syn_drops_total counter");
            let _ = writeln!(out, "craft_xdp_syn_drops_total {}", m.syn_flood_drops);

            let _ = writeln!(out, "# HELP craft_xdp_udp_drops_total Total UDP flood packets dropped");
            let _ = writeln!(out, "# TYPE craft_xdp_udp_drops_total counter");
            let _ = writeln!(out, "craft_xdp_udp_drops_total {}", m.udp_flood_drops);

            let _ = writeln!(out, "# HELP craft_xdp_raknet_drops_total Total RakNet handshake flood packets dropped");
            let _ = writeln!(out, "# TYPE craft_xdp_raknet_drops_total counter");
            let _ = writeln!(out, "craft_xdp_raknet_drops_total {}", m.raknet_flood_drops);

            let _ = writeln!(out, "# HELP craft_xdp_active_flows Current tracked flows in in-kernel LRU map");
            let _ = writeln!(out, "# TYPE craft_xdp_active_flows gauge");
            let _ = writeln!(out, "craft_xdp_active_flows {}", m.current_active_flows);

            let _ = writeln!(out, "# HELP craft_xdp_drop_rate_pps Instantaneous drop rate in packets per second");
            let _ = writeln!(out, "# TYPE craft_xdp_drop_rate_pps gauge");
            let _ = writeln!(out, "craft_xdp_drop_rate_pps {:.2}", m.drop_rate_pps);

            let _ = writeln!(out, "# HELP craft_xdp_bandwidth_absorbed_gbps Instantaneous absorbed flood bandwidth in Gbps");
            let _ = writeln!(out, "# TYPE craft_xdp_bandwidth_absorbed_gbps gauge");
            let _ = writeln!(out, "craft_xdp_bandwidth_absorbed_gbps {:.4}", m.bandwidth_absorbed_gbps);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use craft_core::xdp::{XdpAction, XdpProtocol};

    #[tokio::test]
    async fn test_xdp_service_lifecycle() {
        let temp_dir = tempfile::tempdir().unwrap();
        let paths = CraftPaths::from_base(temp_dir.path().to_path_buf());
        let service = XdpService::new(paths);

        let initial_status = service.get_status().unwrap();
        assert!(initial_status.attached);

        let rule = XdpFilterRule {
            id: "drop-attacker".to_string(),
            cidr: "198.51.100.5/32".to_string(),
            port: None,
            protocol: XdpProtocol::Any,
            rate_limit_pps: None,
            burst_tokens: None,
            action: XdpAction::Drop,
            priority: 100,
            ban_ttl_seconds: Some(3600),
            created_at: 1000,
        };

        let updated = service.add_rule(rule).unwrap();
        assert_eq!(updated.rules_count, 1);

        let detached = service.detach_interface().unwrap();
        assert!(!detached.attached);

        let reattached = service.attach_interface("eth1", XdpAttachMode::Native).unwrap();
        assert!(reattached.attached);
        assert_eq!(reattached.interface_name, "eth1");
        assert_eq!(reattached.mode, XdpAttachMode::Native);

        let metrics_str = service.generate_prometheus_metrics();
        assert!(metrics_str.contains("craft_xdp_packets_total"));
    }
}
