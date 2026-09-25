// crates/daemon/src/ptp_service.rs
//
// Autonomous Sub-Atomic Quantum Clock Synchronization & IEEE 1588 PTP Supervisor Service.
// Pure-Rust PI clock servo, hardware timestamping, TrueTime uncertainty bounds, and leap second smearing.
// Strictly zero emojis.

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use craft_core::error::{CraftError, Result};
use craft_core::path::CraftPaths;
use craft_core::ptp::{
    ClockServoMode, PtpBenchmarkMetrics, PtpPeer, PtpRegistry, PtpStatusSummary, TrueTimeInterval,
};
use craft_net::ptp::{benchmark_ptp_clock_sync, PtpClockServo, TrueTimeEngine};

static INSTANCE: OnceLock<Arc<PtpClockService>> = OnceLock::new();

pub struct PtpClockService {
    paths: CraftPaths,
    servo: Mutex<PtpClockServo>,
    truetime: Mutex<TrueTimeEngine>,
    mode: Mutex<ClockServoMode>,
    master_id: Mutex<String>,
    peers: Mutex<Vec<PtpPeer>>,
    packets_total: Arc<AtomicU64>,
    causality_violations_total: Arc<AtomicU32>,
}

impl PtpClockService {
    pub fn new(paths: CraftPaths) -> Self {
        let registry = PtpRegistry::load_or_default(&paths.ptp_registry_file);
        let servo = PtpClockServo::new(registry.kp, registry.ki);
        let mut truetime = TrueTimeEngine::new(0.85);
        if registry.leap_smear_active {
            truetime.trigger_leap_second_smear(registry.leap_seconds);
        }

        Self {
            paths,
            servo: Mutex::new(servo),
            truetime: Mutex::new(truetime),
            mode: Mutex::new(registry.mode),
            master_id: Mutex::new(registry.master_id),
            peers: Mutex::new(registry.peers),
            packets_total: Arc::new(AtomicU64::new(0)),
            causality_violations_total: Arc::new(AtomicU32::new(0)),
        }
    }

    pub fn global(paths: &CraftPaths) -> Arc<Self> {
        INSTANCE
            .get_or_init(|| Arc::new(Self::new(paths.clone())))
            .clone()
    }

    pub fn get_status(&self, _server: Option<&str>) -> Result<PtpStatusSummary> {
        let servo = self.servo.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        let truetime = self.truetime.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        let mode = *self.mode.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        let master_id = self.master_id.lock().map_err(|e| CraftError::Other(e.to_string()))?.clone();
        let peers = self.peers.lock().map_err(|e| CraftError::Other(e.to_string()))?;

        let mut summary = craft_net::ptp::generate_ptp_status_summary(
            &servo,
            &truetime,
            mode,
            &master_id,
            peers.len(),
        );

        summary.packets_processed_total += self.packets_total.load(Ordering::Relaxed);
        summary.causality_violations_total += self.causality_violations_total.load(Ordering::Relaxed);

        Ok(summary)
    }

    pub fn set_mode(&self, new_mode: ClockServoMode, _server: Option<&str>) -> Result<bool> {
        let mut mode = self.mode.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        *mode = new_mode;

        // Persist to registry
        let mut registry = PtpRegistry::load_or_default(&self.paths.ptp_registry_file);
        registry.mode = new_mode;
        let _ = registry.with_lock(&self.paths, || registry.save(&self.paths.ptp_registry_file));
        Ok(true)
    }

    pub fn query_truetime(&self, _server: Option<&str>) -> Result<TrueTimeInterval> {
        let servo = self.servo.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        let truetime = self.truetime.lock().map_err(|e| CraftError::Other(e.to_string()))?;

        let interval = truetime.get_truetime(servo.phase_offset_ns, 120.0);
        Ok(interval)
    }

    pub fn step_servo(&self, offset_ns: f64, rtt_ns: f64, _server: Option<&str>) -> Result<f64> {
        let mut servo = self.servo.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        let corrected = servo.step(offset_ns, rtt_ns);
        self.packets_total.fetch_add(1, Ordering::Relaxed);
        Ok(corrected)
    }

    pub fn trigger_leap_smear(&self, leap_sec: i32, _server: Option<&str>) -> Result<bool> {
        let mut truetime = self.truetime.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        truetime.trigger_leap_second_smear(leap_sec);

        // Persist to registry
        let mut registry = PtpRegistry::load_or_default(&self.paths.ptp_registry_file);
        registry.leap_smear_active = true;
        registry.leap_seconds = leap_sec;
        let _ = registry.with_lock(&self.paths, || registry.save(&self.paths.ptp_registry_file));

        Ok(true)
    }

    pub fn run_bench(&self, iterations: u32, peer_count: u16) -> Result<PtpBenchmarkMetrics> {
        let metrics = benchmark_ptp_clock_sync(iterations, peer_count);
        self.packets_total.fetch_add(metrics.packets_processed, Ordering::Relaxed);
        if metrics.causality_violations > 0 {
            self.causality_violations_total.fetch_add(metrics.causality_violations, Ordering::Relaxed);
        }
        Ok(metrics)
    }

    pub fn reset_metrics(&self) -> Result<bool> {
        self.packets_total.store(0, Ordering::Relaxed);
        self.causality_violations_total.store(0, Ordering::Relaxed);
        let mut servo = self.servo.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        servo.reset();
        let mut truetime = self.truetime.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        truetime.leap_smear_active = false;
        truetime.leap_smear_total_seconds = 0;

        let mut registry = PtpRegistry::load_or_default(&self.paths.ptp_registry_file);
        registry.leap_smear_active = false;
        registry.leap_seconds = 0;
        let _ = registry.with_lock(&self.paths, || registry.save(&self.paths.ptp_registry_file));

        Ok(true)
    }

    pub fn list_peers(&self) -> Result<Vec<PtpPeer>> {
        let peers = self.peers.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        Ok(peers.clone())
    }

    pub fn generate_prometheus_metrics(&self) -> String {
        let status = match self.get_status(None) {
            Ok(s) => s,
            Err(_) => return String::new(),
        };

        let mut out = String::new();
        out.push_str("# HELP craft_ptp_phase_error_nanos Sub-nanosecond clock phase error\n");
        out.push_str("# TYPE craft_ptp_phase_error_nanos gauge\n");
        out.push_str(&format!("craft_ptp_phase_error_nanos {}\n", status.phase_error_ns));

        out.push_str("# HELP craft_ptp_frequency_drift_ppm Clock oscillator frequency slew in ppm\n");
        out.push_str("# TYPE craft_ptp_frequency_drift_ppm gauge\n");
        out.push_str(&format!("craft_ptp_frequency_drift_ppm {}\n", status.frequency_drift_ppm));

        out.push_str("# HELP craft_ptp_truetime_uncertainty_nanos TrueTime bounded uncertainty epsilon in nanoseconds\n");
        out.push_str("# TYPE craft_ptp_truetime_uncertainty_nanos gauge\n");
        out.push_str(&format!("craft_ptp_truetime_uncertainty_nanos {}\n", status.truetime_epsilon_ns));

        out.push_str("# HELP craft_ptp_synchronized_peers Total synchronized IEEE 1588 PTP peer nodes\n");
        out.push_str("# TYPE craft_ptp_synchronized_peers gauge\n");
        out.push_str(&format!("craft_ptp_synchronized_peers {}\n", status.synchronized_peers));

        out.push_str("# HELP craft_ptp_packets_processed_total Cumulative PTP timestamp packets processed\n");
        out.push_str("# TYPE craft_ptp_packets_processed_total counter\n");
        out.push_str(&format!("craft_ptp_packets_processed_total {}\n", status.packets_processed_total));

        out.push_str("# HELP craft_ptp_causality_violations_total Cumulative causality ordering violations\n");
        out.push_str("# TYPE craft_ptp_causality_violations_total counter\n");
        out.push_str(&format!("craft_ptp_causality_violations_total {}\n", status.causality_violations_total));

        out
    }
}
