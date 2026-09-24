use craft_core::path::CraftPaths;
use craft_core::shm::{
    ShmBenchmarkMetrics, ShmRegistry, ShmSegmentConfig, ShmSegmentMeta, ShmStatusSummary,
};
use craft_core::{CraftError, Result};
use craft_net::{benchmark_shm_throughput, ShmRingBus};
use craft_scripting::{HookBus, HookContext, LifecycleEvent};
use std::fmt::Write as FmtWrite;
use std::fs;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Instant;

static INSTANCE: OnceLock<Arc<ShmService>> = OnceLock::new();

/// Background supervisor coordinating high-speed POSIX shared memory rings and zero-copy IPC
pub struct ShmService {
    paths: CraftPaths,
    bus: Arc<RwLock<ShmRingBus>>,
    pub start_time: Instant,
}

impl ShmService {
    pub fn new(paths: CraftPaths) -> Self {
        let bus = ShmRingBus::new(paths.clone());
        Self {
            paths,
            bus: Arc::new(RwLock::new(bus)),
            start_time: Instant::now(),
        }
    }

    pub fn global(paths: &CraftPaths) -> Arc<Self> {
        INSTANCE
            .get_or_init(|| Arc::new(Self::new(paths.clone())))
            .clone()
    }

    /// Retrieve shared memory status summary with optional server filtering and state fallback
    pub fn get_status(&self, server_filter: Option<&str>) -> Result<ShmStatusSummary> {
        let mut summary = {
            let bus = self
                .bus
                .read()
                .map_err(|_| CraftError::Other("SHM bus lock poisoned".to_string()))?;
            bus.summarize()
        };

        // Fallback to persisted state file if registry file does not exist yet state file exists
        if summary.segments.is_empty() && !self.paths.shm_registry_file.exists() && self.paths.shm_state_file.exists() {
            if let Ok(content) = fs::read_to_string(&self.paths.shm_state_file) {
                if let Ok(cached) = serde_json::from_str::<ShmStatusSummary>(&content) {
                    if !cached.segments.is_empty() {
                        summary = cached;
                    }
                }
            }
        }

        // Apply server filter if requested
        if let Some(srv) = server_filter {
            summary.segments.retain(|s| s.server == srv);
            summary.active_segments = summary.segments.len();
        }

        // Persist state snapshot
        let _ = self.save_state(&summary);

        Ok(summary)
    }

    /// Creates and registers a new shared memory channel
    pub fn create_channel(
        &self,
        server: &str,
        channel: &str,
        slot_size: usize,
        slot_count: usize,
    ) -> Result<ShmSegmentMeta> {
        let region = {
            let mut bus = self
                .bus
                .write()
                .map_err(|_| CraftError::Other("SHM bus lock poisoned".to_string()))?;
            bus.create_channel(server, channel, slot_size, slot_count)?
        };

        let config = ShmSegmentConfig::new(server, channel, slot_size, slot_count);
        let meta = ShmSegmentMeta {
            name: config.name.clone(),
            server: server.to_string(),
            channel_type: config.channel_type.clone(),
            slot_size: config.slot_size,
            slot_count: config.slot_count,
            capacity_bytes: config.capacity_bytes,
            created_at_epoch_s: craft_core::shm::current_time_epoch_s(),
            last_lease_epoch_s: craft_core::shm::current_time_epoch_s(),
            active_producers: 1,
            active_consumers: 0,
            messages_written: 0,
            messages_read: 0,
            is_stale: false,
        };

        // Dispatch lifecycle event
        let hook_ctx = HookContext::for_shm_channel_opened(
            &config.name,
            &config.channel_type.to_string(),
        );
        HookBus::dispatch_async(
            self.paths.clone(),
            LifecycleEvent::ShmChannelOpened,
            hook_ctx,
            5,
        );

        let summary = self.get_status(None)?;
        let _ = self.save_state(&summary);

        drop(region);
        Ok(meta)
    }

    /// Closes and unlinks a shared memory channel
    pub fn close_channel(&self, server: &str, channel: &str) -> Result<bool> {
        let removed = {
            let mut bus = self
                .bus
                .write()
                .map_err(|_| CraftError::Other("SHM bus lock poisoned".to_string()))?;
            bus.close_channel(server, channel)?
        };

        let config = ShmSegmentConfig::new(server, channel, 4096, 1024);
        let hook_ctx = HookContext::for_shm_channel_closed(
            &config.name,
            &config.channel_type.to_string(),
        );
        HookBus::dispatch_async(
            self.paths.clone(),
            LifecycleEvent::ShmChannelClosed,
            hook_ctx,
            5,
        );

        let summary = self.get_status(None)?;
        let _ = self.save_state(&summary);

        Ok(removed)
    }

    /// Writes a payload to a shared memory channel
    pub fn write_event(&self, server: &str, channel: &str, payload: &[u8]) -> Result<u64> {
        let producer = {
            let mut bus = self
                .bus
                .write()
                .map_err(|_| CraftError::Other("SHM bus lock poisoned".to_string()))?;
            bus.producer(server, channel)?
        };

        let seq = producer.push(payload)?;

        // Update cached state
        let summary = self.get_status(None)?;
        let _ = self.save_state(&summary);

        Ok(seq)
    }

    /// Reads up to limit pending payloads from a shared memory channel
    pub fn read_events(&self, server: &str, channel: &str, limit: usize) -> Result<Vec<Vec<u8>>> {
        let consumer = {
            let mut bus = self
                .bus
                .write()
                .map_err(|_| CraftError::Other("SHM bus lock poisoned".to_string()))?;
            bus.consumer(server, channel)?
        };

        let drained = consumer.drain(limit.max(1));
        let payloads = drained.into_iter().map(|(_, p)| p).collect();

        // Update cached state
        let summary = self.get_status(None)?;
        let _ = self.save_state(&summary);

        Ok(payloads)
    }

    /// Runs a high-throughput microsecond zero-copy benchmark
    pub fn run_bench(&self, message_count: usize, payload_size: usize) -> Result<ShmBenchmarkMetrics> {
        benchmark_shm_throughput(message_count, payload_size)
    }

    /// Sweeps stale channel leases and unlinks orphaned segments
    pub fn sweep_stale(&self) -> Vec<String> {
        let reclaimed = {
            if let Ok(mut bus) = self.bus.write() {
                bus.sweep_stale(5)
            } else {
                Vec::new()
            }
        };

        for name in &reclaimed {
            let hook_ctx = HookContext::for_shm_lease_expired(name);
            HookBus::dispatch_async(
                self.paths.clone(),
                LifecycleEvent::ShmLeaseExpired,
                hook_ctx,
                5,
            );
        }

        if !reclaimed.is_empty() {
            if let Ok(summary) = self.get_status(None) {
                let _ = self.save_state(&summary);
            }
        }

        reclaimed
    }

    /// Resets metrics and active channels
    pub fn reset_metrics(&self, server_filter: Option<&str>) -> Result<()> {
        let mut bus = self
            .bus
            .write()
            .map_err(|_| CraftError::Other("SHM bus lock poisoned".to_string()))?;

        if let Some(srv) = server_filter {
            let reg = ShmRegistry::load(&self.paths).unwrap_or_default();
            for meta in reg.channels {
                if meta.server == srv {
                    let _ = bus.close_channel(&meta.server, &meta.channel_type.to_string());
                }
            }
        } else {
            let reg = ShmRegistry::load(&self.paths).unwrap_or_default();
            for meta in reg.channels {
                let _ = bus.close_channel(&meta.server, &meta.channel_type.to_string());
            }
        }

        let summary = bus.summarize();
        let _ = self.save_state(&summary);
        Ok(())
    }

    fn save_state(&self, summary: &ShmStatusSummary) -> Result<()> {
        if let Some(parent) = self.paths.shm_state_file.parent() {
            if !parent.exists() {
                let _ = fs::create_dir_all(parent);
            }
        }
        let data = serde_json::to_string_pretty(summary)?;
        let tmp = self.paths.shm_dir.join("state.json.tmp");
        let _ = fs::write(&tmp, data);
        let _ = fs::rename(tmp, &self.paths.shm_state_file);
        Ok(())
    }

    /// Expose Prometheus metrics formatted in standard exposition syntax
    pub fn generate_prometheus_metrics(&self) -> String {
        let summary = self.get_status(None).unwrap_or_default();
        let mut out = String::with_capacity(1024);

        let _ = writeln!(out, "# HELP craft_shm_active_segments Number of active POSIX shared memory segments");
        let _ = writeln!(out, "# TYPE craft_shm_active_segments gauge");
        let _ = writeln!(out, "craft_shm_active_segments {}", summary.active_segments);

        let _ = writeln!(out, "# HELP craft_shm_allocated_bytes Total bytes mapped across shared memory segments");
        let _ = writeln!(out, "# TYPE craft_shm_allocated_bytes gauge");
        let _ = writeln!(out, "craft_shm_allocated_bytes {}", summary.total_allocated_bytes);

        let _ = writeln!(out, "# HELP craft_shm_messages_written_total Total messages written to shared memory ring queues");
        let _ = writeln!(out, "# TYPE craft_shm_messages_written_total counter");
        let _ = writeln!(out, "craft_shm_messages_written_total {}", summary.messages_written_total);

        let _ = writeln!(out, "# HELP craft_shm_messages_read_total Total messages read from shared memory ring queues");
        let _ = writeln!(out, "# TYPE craft_shm_messages_read_total counter");
        let _ = writeln!(out, "craft_shm_messages_read_total {}", summary.messages_read_total);

        let _ = writeln!(out, "# HELP craft_shm_latency_nanoseconds Average zero-copy IPC transmission latency in nanoseconds");
        let _ = writeln!(out, "# TYPE craft_shm_latency_nanoseconds gauge");
        let _ = writeln!(out, "craft_shm_latency_nanoseconds {:.2}", summary.avg_latency_ns);

        let _ = writeln!(out, "# HELP craft_shm_stale_reclamations_total Number of orphaned shared memory segments reclaimed by watchdog");
        let _ = writeln!(out, "# TYPE craft_shm_stale_reclamations_total counter");
        let _ = writeln!(out, "craft_shm_stale_reclamations_total {}", summary.watchdog_reclaimed_segments);

        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_shm_service_lifecycle() {
        let dir = tempdir().unwrap();
        let paths = CraftPaths::from_base(dir.path().to_path_buf());
        let service = ShmService::new(paths.clone());

        let status = service.get_status(None).unwrap();
        assert_eq!(status.active_segments, 0);

        let meta = service.create_channel("hub", "tick", 256, 32).unwrap();
        assert_eq!(meta.server, "hub");

        let status_after = service.get_status(None).unwrap();
        assert_eq!(status_after.active_segments, 1);

        let seq = service.write_event("hub", "tick", b"tick_payload").unwrap();
        assert_eq!(seq, 0);

        let read = service.read_events("hub", "tick", 5).unwrap();
        assert_eq!(read.len(), 1);
        assert_eq!(read[0], b"tick_payload");

        let bench = service.run_bench(10_000, 64).unwrap();
        assert_eq!(bench.message_count, 10_000);
        assert!(bench.throughput_msgs_per_sec > 0.0);

        let closed = service.close_channel("hub", "tick").unwrap();
        assert!(closed);

        let status_final = service.get_status(None).unwrap();
        assert_eq!(status_final.active_segments, 0);
    }
}
