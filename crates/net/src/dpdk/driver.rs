use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use bytes::Bytes;
use serde::{Deserialize, Serialize};

use crate::dpdk::jitter::JitterCalculator;
use crate::dpdk::ring_buffer::{PacketDescriptor, PacketRingBuffer};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DpdkDriverConfig {
    pub port_id: u16,
    pub ring_size: usize,
    pub burst_size: usize,
    pub promiscuous: bool,
    pub use_hardware_driver: bool,
}

impl Default for DpdkDriverConfig {
    fn default() -> Self {
        Self {
            port_id: 0,
            ring_size: 4096,
            burst_size: 32,
            promiscuous: true,
            use_hardware_driver: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DpdkDriverStats {
    pub rx_packets: u64,
    pub tx_packets: u64,
    pub rx_bytes: u64,
    pub tx_bytes: u64,
    pub rx_dropped: u64,
    pub tx_dropped: u64,
    pub avg_jitter_micros: f64,
    pub p99_jitter_micros: f64,
    pub throughput_pps: f64,
    pub throughput_mb_s: f64,
    pub is_hardware_driver_active: bool,
}

pub struct DpdkDriver {
    config: DpdkDriverConfig,
    rx_ring: Arc<PacketRingBuffer>,
    tx_ring: Arc<PacketRingBuffer>,
    jitter: Arc<Mutex<JitterCalculator>>,
    rx_packets: AtomicU64,
    tx_packets: AtomicU64,
    rx_bytes: AtomicU64,
    tx_bytes: AtomicU64,
    running: Arc<AtomicBool>,
    is_hardware: bool,
}

impl DpdkDriver {
    pub fn new(config: DpdkDriverConfig) -> Self {
        let is_hw = if config.use_hardware_driver {
            Self::probe_hardware_driver()
        } else {
            false
        };

        let rx_ring = Arc::new(PacketRingBuffer::new(config.ring_size));
        let tx_ring = Arc::new(PacketRingBuffer::new(config.ring_size));
        let jitter = Arc::new(Mutex::new(JitterCalculator::new(2048)));

        Self {
            config,
            rx_ring,
            tx_ring,
            jitter,
            rx_packets: AtomicU64::new(0),
            tx_packets: AtomicU64::new(0),
            rx_bytes: AtomicU64::new(0),
            tx_bytes: AtomicU64::new(0),
            running: Arc::new(AtomicBool::new(false)),
            is_hardware: is_hw,
        }
    }

    pub fn config(&self) -> &DpdkDriverConfig {
        &self.config
    }

    pub fn probe_hardware_driver() -> bool {
        let vfio_path = Path::new("/sys/bus/pci/drivers/vfio-pci");
        let uio_path = Path::new("/sys/class/uio");
        vfio_path.exists() || uio_path.exists()
    }

    pub fn start(&self) {
        self.running.store(true, Ordering::SeqCst);
    }

    pub fn stop(&self) {
        self.running.store(false, Ordering::SeqCst);
    }

    pub fn is_running(&self) -> bool {
        self.running.load(Ordering::Relaxed)
    }

    pub fn send_packet(&self, packet_id: u64, data: Bytes, port: u16) -> Result<(), Bytes> {
        let len = data.len() as u64;
        let desc = PacketDescriptor::new(packet_id, data, port);
        match self.tx_ring.enqueue(desc) {
            Ok(()) => {
                self.tx_packets.fetch_add(1, Ordering::Relaxed);
                self.tx_bytes.fetch_add(len, Ordering::Relaxed);
                Ok(())
            }
            Err(d) => Err(d.data),
        }
    }

    pub fn receive_packet(&self) -> Option<PacketDescriptor> {
        if let Some(desc) = self.rx_ring.dequeue() {
            self.rx_packets.fetch_add(1, Ordering::Relaxed);
            self.rx_bytes.fetch_add(desc.length as u64, Ordering::Relaxed);

            if let Ok(mut j) = self.jitter.lock() {
                j.record_packet(desc.rx_timestamp_nanos);
            }
            Some(desc)
        } else {
            None
        }
    }

    pub fn get_stats(&self) -> DpdkDriverStats {
        let rx_pkts = self.rx_packets.load(Ordering::Relaxed);
        let tx_pkts = self.tx_packets.load(Ordering::Relaxed);
        let rx_b = self.rx_bytes.load(Ordering::Relaxed);
        let tx_b = self.tx_bytes.load(Ordering::Relaxed);

        let (avg_jitter, p99_jitter) = if let Ok(j) = self.jitter.lock() {
            (j.mean_jitter_micros(), j.p99_micros())
        } else {
            (0.0, 0.0)
        };

        DpdkDriverStats {
            rx_packets: rx_pkts,
            tx_packets: tx_pkts,
            rx_bytes: rx_b,
            tx_bytes: tx_b,
            rx_dropped: self.rx_ring.dropped_count(),
            tx_dropped: self.tx_ring.dropped_count(),
            avg_jitter_micros: avg_jitter,
            p99_jitter_micros: p99_jitter,
            throughput_pps: 0.0,
            throughput_mb_s: 0.0,
            is_hardware_driver_active: self.is_hardware,
        }
    }

    pub fn benchmark_synthetic_stream(&self, count: usize) -> DpdkDriverStats {
        let payload = Bytes::from_static(b"DPDK_MINECRAFT_PACKET_BURST_TEST_PAYLOAD_1024_BYTES");
        let start = Instant::now();

        // Enqueue synthetic packets
        for i in 0..count {
            let desc = PacketDescriptor::new(i as u64, payload.clone(), 25565);
            let _ = self.rx_ring.enqueue(desc);
        }

        // Dequeue packets and measure
        let mut processed = 0u64;
        let mut total_bytes = 0u64;
        while let Some(desc) = self.receive_packet() {
            processed += 1;
            total_bytes += desc.length as u64;
        }

        let elapsed_secs = start.elapsed().as_secs_f64().max(0.000_001);
        let pps = (processed as f64) / elapsed_secs;
        let mb_s = ((total_bytes as f64) / (1024.0 * 1024.0)) / elapsed_secs;

        let (avg_jitter, p99_jitter) = if let Ok(j) = self.jitter.lock() {
            (j.mean_jitter_micros(), j.p99_micros())
        } else {
            (0.0, 0.0)
        };

        DpdkDriverStats {
            rx_packets: processed,
            tx_packets: processed,
            rx_bytes: total_bytes,
            tx_bytes: total_bytes,
            rx_dropped: self.rx_ring.dropped_count(),
            tx_dropped: self.tx_ring.dropped_count(),
            avg_jitter_micros: avg_jitter,
            p99_jitter_micros: p99_jitter,
            throughput_pps: pps,
            throughput_mb_s: mb_s,
            is_hardware_driver_active: self.is_hardware,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dpdk_synthetic_benchmark() {
        let driver = DpdkDriver::new(DpdkDriverConfig::default());
        driver.start();
        let stats = driver.benchmark_synthetic_stream(100);

        assert_eq!(stats.rx_packets, 100);
        assert!(stats.throughput_pps > 0.0);
        assert!(stats.throughput_mb_s > 0.0);
    }
}
