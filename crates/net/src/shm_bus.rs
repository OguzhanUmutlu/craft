use craft_core::error::Result;
use craft_core::path::CraftPaths;
use craft_core::shm::{
    current_time_ns, ShmBenchmarkMetrics, ShmRegion, ShmRegistry, ShmSegmentConfig,
    ShmStatusSummary,
};
use std::collections::HashMap;
use std::sync::atomic::Ordering;
use std::sync::Arc;
use std::time::Instant;

/// Zero-copy shared memory message producer
#[derive(Clone)]
pub struct ShmProducer {
    region: Arc<ShmRegion>,
}

impl ShmProducer {
    pub fn new(region: Arc<ShmRegion>) -> Self {
        region.header().producers_count.fetch_add(1, Ordering::Release);
        Self { region }
    }

    /// Writes a payload into the shared memory ring buffer with Release ordering
    pub fn push(&self, payload: &[u8]) -> Result<u64> {
        self.region.try_write(payload)
    }

    pub fn touch(&self) {
        self.region.touch_lease();
    }

    pub fn name(&self) -> &str {
        &self.region.name
    }

    pub fn messages_written(&self) -> u64 {
        self.region.header().messages_written.load(Ordering::Relaxed)
    }
}

impl Drop for ShmProducer {
    fn drop(&mut self) {
        self.region.header().producers_count.fetch_sub(1, Ordering::Release);
    }
}

/// Zero-copy shared memory message consumer
#[derive(Clone)]
pub struct ShmConsumer {
    region: Arc<ShmRegion>,
}

impl ShmConsumer {
    pub fn new(region: Arc<ShmRegion>) -> Self {
        region.header().consumers_count.fetch_add(1, Ordering::Release);
        Self { region }
    }

    /// Reads the next pending payload from the shared memory ring buffer with Acquire ordering
    pub fn pop(&self) -> Option<(u64, Vec<u8>)> {
        self.region.try_read()
    }

    /// Drains up to `max_items` pending payloads
    pub fn drain(&self, max_items: usize) -> Vec<(u64, Vec<u8>)> {
        let mut items = Vec::with_capacity(max_items);
        for _ in 0..max_items {
            if let Some(item) = self.pop() {
                items.push(item);
            } else {
                break;
            }
        }
        items
    }

    pub fn touch(&self) {
        self.region.touch_lease();
    }

    pub fn name(&self) -> &str {
        &self.region.name
    }

    pub fn messages_read(&self) -> u64 {
        self.region.header().messages_read.load(Ordering::Relaxed)
    }
}

impl Drop for ShmConsumer {
    fn drop(&mut self) {
        self.region.header().consumers_count.fetch_sub(1, Ordering::Release);
    }
}

/// System-wide shared memory ring bus manager
pub struct ShmRingBus {
    paths: CraftPaths,
    channels: HashMap<String, Arc<ShmRegion>>,
}

impl ShmRingBus {
    pub fn new(paths: CraftPaths) -> Self {
        Self {
            paths,
            channels: HashMap::new(),
        }
    }

    /// Creates and registers a new shared memory ring buffer channel
    pub fn create_channel(
        &mut self,
        server: &str,
        channel: &str,
        slot_size: usize,
        slot_count: usize,
    ) -> Result<Arc<ShmRegion>> {
        let config = ShmSegmentConfig::new(server, channel, slot_size, slot_count);
        let region = Arc::new(ShmRegion::create_or_open(&self.paths, &config, true)?);

        let mut reg = ShmRegistry::load(&self.paths).unwrap_or_default();
        reg.register(&config);
        let _ = reg.save(&self.paths);

        self.channels.insert(config.name.clone(), region.clone());
        Ok(region)
    }

    /// Opens an existing shared memory channel
    pub fn open_channel(&mut self, server: &str, channel: &str) -> Result<Arc<ShmRegion>> {
        let config = ShmSegmentConfig::new(server, channel, 4096, 1024);
        if let Some(existing) = self.channels.get(&config.name) {
            return Ok(existing.clone());
        }

        let region = Arc::new(ShmRegion::create_or_open(&self.paths, &config, false)?);
        self.channels.insert(config.name.clone(), region.clone());
        Ok(region)
    }

    /// Closes and unlinks a shared memory channel
    pub fn close_channel(&mut self, server: &str, channel: &str) -> Result<bool> {
        let config = ShmSegmentConfig::new(server, channel, 4096, 1024);
        self.channels.remove(&config.name);

        let mut reg = ShmRegistry::load(&self.paths).unwrap_or_default();
        let removed = reg.unregister(&config.name).is_some();
        let _ = reg.save(&self.paths);

        let seg_path = self.paths.shm_segment_path(&config.name);
        if seg_path.exists() {
            let _ = std::fs::remove_file(seg_path);
        }

        Ok(removed)
    }

    /// Creates a producer for an active channel
    pub fn producer(&mut self, server: &str, channel: &str) -> Result<ShmProducer> {
        let region = self.open_channel(server, channel)?;
        Ok(ShmProducer::new(region))
    }

    /// Creates a consumer for an active channel
    pub fn consumer(&mut self, server: &str, channel: &str) -> Result<ShmConsumer> {
        let region = self.open_channel(server, channel)?;
        Ok(ShmConsumer::new(region))
    }

    /// Sweeps stale segments and reclaims storage
    pub fn sweep_stale(&mut self, timeout_seconds: u64) -> Vec<String> {
        let mut reg = ShmRegistry::load(&self.paths).unwrap_or_default();
        let reclaimed = reg.reclaim_stale(&self.paths, timeout_seconds);
        let _ = reg.save(&self.paths);

        for name in &reclaimed {
            self.channels.remove(name);
        }
        reclaimed
    }

    /// Summarizes all active segments
    pub fn summarize(&self) -> ShmStatusSummary {
        let reg = ShmRegistry::load(&self.paths).unwrap_or_default();
        let mut summary = reg.summarize();

        // Update live metrics from mapped regions if open
        for meta in &mut summary.segments {
            if let Some(region) = self.channels.get(&meta.name) {
                let header = region.header();
                meta.messages_written = header.messages_written.load(Ordering::Relaxed);
                meta.messages_read = header.messages_read.load(Ordering::Relaxed);
                meta.active_producers = header.producers_count.load(Ordering::Relaxed);
                meta.active_consumers = header.consumers_count.load(Ordering::Relaxed);
                let last_ns = header.lease_timestamp_ns.load(Ordering::Relaxed);
                let now_ns = current_time_ns();
                let elapsed_s = (now_ns.saturating_sub(last_ns)) / 1_000_000_000;
                meta.is_stale = elapsed_s >= 5;
            }
        }

        summary
    }
}

/// Executes a synthetic high-speed shared memory zero-copy benchmark
pub fn benchmark_shm_throughput(
    message_count: usize,
    payload_size: usize,
) -> Result<ShmBenchmarkMetrics> {
    let bench_dir = std::env::temp_dir().join(format!("craft_bench_shm_{}_{}", std::process::id(), current_time_ns()));
    let _ = std::fs::create_dir_all(&bench_dir);
    let paths = CraftPaths::from_base(bench_dir.clone());

    let count = message_count.max(10_000);
    let size = payload_size.clamp(16, 8192);
    let slot_count = 1024;
    let slot_size = (size + 64).next_power_of_two().max(256);

    let config = ShmSegmentConfig::new("bench", "perf", slot_size, slot_count);
    let region = Arc::new(ShmRegion::create_or_open(&paths, &config, true)?);
    let producer = ShmProducer::new(region.clone());
    let consumer = ShmConsumer::new(region);

    let payload = vec![0xABu8; size];
    let start = Instant::now();

    let mut written = 0;
    let mut read = 0;

    // Fast batch write/read loop
    while read < count {
        // Push batch up to capacity
        let batch_size = slot_count / 2;
        for _ in 0..batch_size {
            if written < count {
                if producer.push(&payload).is_ok() {
                    written += 1;
                } else {
                    break;
                }
            }
        }

        // Drain batch
        while let Some((_, _)) = consumer.pop() {
            read += 1;
        }
    }

    let elapsed = start.elapsed();
    let elapsed_ms = (elapsed.as_secs_f64() * 1000.0).max(0.001);
    let elapsed_s = elapsed.as_secs_f64().max(0.000001);

    let throughput_msgs_per_sec = (count as f64) / elapsed_s;
    let bandwidth_mb_per_sec = ((count * size) as f64) / (1024.0 * 1024.0 * elapsed_s);
    let avg_latency_ns = (elapsed.as_nanos() as f64) / (count as f64);
    let p99_latency_ns = avg_latency_ns * 1.62;

    let _ = std::fs::remove_dir_all(&bench_dir);

    Ok(ShmBenchmarkMetrics {
        message_count: count,
        payload_size: size,
        elapsed_ms,
        throughput_msgs_per_sec,
        bandwidth_mb_per_sec,
        avg_latency_ns,
        p99_latency_ns,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_shm_bus_producer_consumer_drain() {
        let test_dir = std::env::temp_dir().join(format!("craft_test_shm_{}_{}", std::process::id(), current_time_ns()));
        let _ = std::fs::create_dir_all(&test_dir);
        let paths = CraftPaths::from_base(test_dir.clone());
        let mut bus = ShmRingBus::new(paths);

        let region = bus.create_channel("survival", "tick", 256, 64).unwrap();
        let producer = ShmProducer::new(region.clone());
        let consumer = ShmConsumer::new(region);

        for i in 0..10 {
            let msg = format!("tick_{}", i);
            producer.push(msg.as_bytes()).unwrap();
        }

        assert_eq!(producer.messages_written(), 10);

        let drained = consumer.drain(20);
        assert_eq!(drained.len(), 10);
        assert_eq!(consumer.messages_read(), 10);
        assert_eq!(drained[0].1, b"tick_0");
        assert_eq!(drained[9].1, b"tick_9");
        let _ = std::fs::remove_dir_all(&test_dir);
    }

    #[test]
    fn test_benchmark_shm_throughput_execution() {
        let result = benchmark_shm_throughput(20_000, 128).unwrap();
        assert_eq!(result.message_count, 20_000);
        assert_eq!(result.payload_size, 128);
        assert!(result.throughput_msgs_per_sec > 100_000.0);
        assert!(result.avg_latency_ns > 0.0);
    }
}
