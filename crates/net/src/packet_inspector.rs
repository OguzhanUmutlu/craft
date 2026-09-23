use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::fmt;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

/// Severity of detected anomalous packet traffic.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum AnomalySeverity {
    Elevated,
    Critical,
}

impl AnomalySeverity {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Elevated => "[ELEVATED]",
            Self::Critical => "[CRITICAL]",
        }
    }
}

impl fmt::Display for AnomalySeverity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

/// Anomaly descriptor when an abnormal packet flood or burst occurs.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacketFloodAnomaly {
    pub timestamp_ms: u64,
    pub severity: AnomalySeverity,
    pub current_pps: u64,
    pub threshold_pps: u64,
    pub message: String,
}

/// Snapshot summary of network packet rates.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PacketRateSummary {
    pub rx_pps: u64,
    pub tx_pps: u64,
    pub total_pps: u64,
    pub rx_bytes_sec: u64,
    pub tx_bytes_sec: u64,
    pub total_bytes_sec: u64,
    pub burst_detected: bool,
    pub flood_warning: bool,
    pub anomalous_threshold_pps: u64,
}

/// Historical rate sample point.
#[derive(Debug, Clone, Copy)]
struct RateSample {
    rx_packets: u64,
    tx_packets: u64,
    rx_bytes: u64,
    tx_bytes: u64,
}

/// Non-intrusive Netty and transport packet rate inspector.
#[derive(Debug, Clone)]
pub struct NettyPacketInspector {
    /// Accumulated packets received in the current sampling window.
    curr_rx_packets: u64,
    /// Accumulated packets transmitted in the current sampling window.
    curr_tx_packets: u64,
    /// Accumulated bytes received in the current sampling window.
    curr_rx_bytes: u64,
    /// Accumulated bytes transmitted in the current sampling window.
    curr_tx_bytes: u64,
    /// Last rate sample time.
    last_sample_instant: Instant,
    /// Historical rates for rolling average and burst detection (last 30 samples).
    history: VecDeque<RateSample>,
    /// Configured threshold for packet flood warnings (in total PPS).
    pub flood_threshold_pps: u64,
    /// Multiplier against rolling baseline to flag traffic burst (e.g. 4.0x).
    pub burst_multiplier: f64,
    /// Most recent evaluated rate summary.
    pub last_summary: PacketRateSummary,
}

impl Default for NettyPacketInspector {
    fn default() -> Self {
        Self::new(5_000, 3.5)
    }
}

impl NettyPacketInspector {
    /// Creates a new `NettyPacketInspector`.
    pub fn new(flood_threshold_pps: u64, burst_multiplier: f64) -> Self {
        Self {
            curr_rx_packets: 0,
            curr_tx_packets: 0,
            curr_rx_bytes: 0,
            curr_tx_bytes: 0,
            last_sample_instant: Instant::now(),
            history: VecDeque::with_capacity(32),
            flood_threshold_pps: flood_threshold_pps.max(100),
            burst_multiplier: burst_multiplier.max(1.5),
            last_summary: PacketRateSummary {
                rx_pps: 0,
                tx_pps: 0,
                total_pps: 0,
                rx_bytes_sec: 0,
                tx_bytes_sec: 0,
                total_bytes_sec: 0,
                burst_detected: false,
                flood_warning: false,
                anomalous_threshold_pps: flood_threshold_pps,
            },
        }
    }

    /// Records ingress (received) packets and bytes.
    pub fn record_ingress(&mut self, packets: u64, bytes: u64) {
        self.curr_rx_packets = self.curr_rx_packets.saturating_add(packets);
        self.curr_rx_bytes = self.curr_rx_bytes.saturating_add(bytes);
    }

    /// Records egress (transmitted) packets and bytes.
    pub fn record_egress(&mut self, packets: u64, bytes: u64) {
        self.curr_tx_packets = self.curr_tx_packets.saturating_add(packets);
        self.curr_tx_bytes = self.curr_tx_bytes.saturating_add(bytes);
    }

    /// Evaluates current traffic rates, commits the sample window, and returns a summary.
    pub fn tick(&mut self) -> PacketRateSummary {
        let elapsed = self.last_sample_instant.elapsed().as_secs_f64().max(0.001);
        self.last_sample_instant = Instant::now();
        self.tick_with_elapsed(elapsed)
    }

    /// Evaluates traffic rates over a specified elapsed duration in seconds.
    pub fn tick_with_elapsed(&mut self, elapsed_secs: f64) -> PacketRateSummary {
        let elapsed = elapsed_secs.max(0.001);
        let rx_pps = (self.curr_rx_packets as f64 / elapsed).round() as u64;
        let tx_pps = (self.curr_tx_packets as f64 / elapsed).round() as u64;
        let total_pps = rx_pps.saturating_add(tx_pps);

        let rx_bytes_sec = (self.curr_rx_bytes as f64 / elapsed).round() as u64;
        let tx_bytes_sec = (self.curr_tx_bytes as f64 / elapsed).round() as u64;
        let total_bytes_sec = rx_bytes_sec.saturating_add(tx_bytes_sec);

        // Check against rolling baseline for burst detection
        let baseline_pps = self.average_historical_pps();
        let burst_detected = if baseline_pps > 10.0 {
            total_pps as f64 >= baseline_pps * self.burst_multiplier
        } else {
            total_pps > 500
        };

        let flood_warning = total_pps >= self.flood_threshold_pps;

        if self.history.len() >= 30 {
            self.history.pop_front();
        }
        self.history.push_back(RateSample {
            rx_packets: rx_pps,
            tx_packets: tx_pps,
            rx_bytes: rx_bytes_sec,
            tx_bytes: tx_bytes_sec,
        });

        // Reset accumulators
        self.curr_rx_packets = 0;
        self.curr_tx_packets = 0;
        self.curr_rx_bytes = 0;
        self.curr_tx_bytes = 0;

        self.last_summary = PacketRateSummary {
            rx_pps,
            tx_pps,
            total_pps,
            rx_bytes_sec,
            tx_bytes_sec,
            total_bytes_sec,
            burst_detected,
            flood_warning,
            anomalous_threshold_pps: self.flood_threshold_pps,
        };

        self.last_summary.clone()
    }

    /// Returns average total PPS across the recorded historical window.
    pub fn average_historical_pps(&self) -> f64 {
        if self.history.is_empty() {
            return 0.0;
        }
        let total: u64 = self.history.iter().map(|s| s.rx_packets + s.tx_packets).sum();
        total as f64 / self.history.len() as f64
    }

    /// Returns average total bandwidth in bytes per second across the recorded historical window.
    pub fn average_historical_bandwidth(&self) -> f64 {
        if self.history.is_empty() {
            return 0.0;
        }
        let total: u64 = self.history.iter().map(|s| s.rx_bytes + s.tx_bytes).sum();
        total as f64 / self.history.len() as f64
    }

    /// Evaluates if an anomalous packet flood event is in progress.
    pub fn check_anomaly(&self) -> Option<PacketFloodAnomaly> {
        let summary = &self.last_summary;
        let timestamp_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);

        if summary.total_pps >= self.flood_threshold_pps * 2 {
            Some(PacketFloodAnomaly {
                timestamp_ms,
                severity: AnomalySeverity::Critical,
                current_pps: summary.total_pps,
                threshold_pps: self.flood_threshold_pps,
                message: format!(
                    "Critical packet flood detected: {} PPS (threshold: {} PPS)",
                    summary.total_pps, self.flood_threshold_pps
                ),
            })
        } else if summary.total_pps >= self.flood_threshold_pps || summary.burst_detected {
            Some(PacketFloodAnomaly {
                timestamp_ms,
                severity: AnomalySeverity::Elevated,
                current_pps: summary.total_pps,
                threshold_pps: self.flood_threshold_pps,
                message: format!(
                    "Elevated packet traffic / burst: {} PPS (baseline: {:.1} PPS)",
                    summary.total_pps,
                    self.average_historical_pps()
                ),
            })
        } else {
            None
        }
    }

    /// Helper to format bandwidth into human-readable rate string.
    pub fn format_bandwidth(bytes_sec: u64) -> String {
        if bytes_sec < 1024 {
            format!("{} B/s", bytes_sec)
        } else if bytes_sec < 1024 * 1024 {
            format!("{:.2} KB/s", bytes_sec as f64 / 1024.0)
        } else {
            format!("{:.2} MB/s", bytes_sec as f64 / (1024.0 * 1024.0))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_packet_inspector_rate_calculation_and_burst() {
        let mut inspector = NettyPacketInspector::new(1000, 3.0);

        // Record normal baseline traffic: 50 packets, 5000 bytes over 1 second
        inspector.record_ingress(50, 5000);
        inspector.record_egress(50, 5000);
        let sum1 = inspector.tick_with_elapsed(1.0);
        assert_eq!(sum1.total_pps, 100);
        assert!(!sum1.flood_warning);

        // Record a massive packet flood: 2000 packets over 1 second
        inspector.record_ingress(1500, 150000);
        inspector.record_egress(500, 50000);
        let sum2 = inspector.tick_with_elapsed(1.0);
        assert_eq!(sum2.total_pps, 2000);
        assert!(sum2.flood_warning);

        let anomaly = inspector.check_anomaly();
        assert!(anomaly.is_some());
        assert_eq!(anomaly.unwrap().severity, AnomalySeverity::Critical);
    }

    #[test]
    fn test_format_bandwidth_units() {
        assert_eq!(NettyPacketInspector::format_bandwidth(512), "512 B/s");
        assert_eq!(NettyPacketInspector::format_bandwidth(2048), "2.00 KB/s");
        assert_eq!(NettyPacketInspector::format_bandwidth(5 * 1024 * 1024), "5.00 MB/s");
    }
}
