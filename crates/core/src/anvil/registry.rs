use std::fs::{self, OpenOptions};
use fs2::FileExt;
use serde::{Deserialize, Serialize};

use crate::error::{CraftError, Result};
use crate::path::CraftPaths;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnvilConfig {
    pub enabled: bool,
    pub engine: String,
    pub cache_max_bytes: usize,
    pub cache_max_chunks: usize,
    pub prefetch_radius: u32,
    pub compression_scheme: String,
    pub batch_size: usize,
    pub direct_dma: bool,
}

impl Default for AnvilConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            engine: "io_uring".to_string(),
            cache_max_bytes: 64 * 1024 * 1024, // 64 MB
            cache_max_chunks: 4096,
            prefetch_radius: 4,
            compression_scheme: "zlib".to_string(),
            batch_size: 32,
            direct_dma: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnvilStatusSummary {
    pub engine: String,
    pub active_cached_chunks: usize,
    pub cache_memory_used_bytes: usize,
    pub cache_memory_limit_bytes: usize,
    pub cache_hit_count: u64,
    pub cache_miss_count: u64,
    pub cache_hit_ratio: f64,
    pub cache_eviction_count: u64,
    pub cache_prefetch_count: u64,
    pub total_io_ops: u64,
    pub total_bytes_read: u64,
    pub total_bytes_written: u64,
    pub context_switch_savings: u64,
    pub avg_io_latency_micros: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChunkSectorInfo {
    pub chunk_x: i32,
    pub chunk_z: i32,
    pub sector_offset: u32,
    pub sector_count: u8,
    pub timestamp: u32,
    pub size_bytes: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RegionDetails {
    pub file_name: String,
    pub file_path: String,
    pub region_x: i32,
    pub region_z: i32,
    pub file_size_bytes: u64,
    pub allocated_sectors: u32,
    pub total_sectors: u32,
    pub active_chunks: usize,
    pub empty_chunks: usize,
    pub fragmentation_ratio: f64,
    pub largest_contiguous_free_sectors: u32,
    pub chunks: Vec<ChunkSectorInfo>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnvilBenchmarkReport {
    pub chunks_tested: usize,
    pub write_time_ms: u64,
    pub write_throughput_mb_sec: f64,
    pub read_time_ms: u64,
    pub read_throughput_mb_sec: f64,
    pub compression_ratio: f64,
    pub engine_used: String,
    pub context_switch_savings: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnvilRegistry {
    pub config: AnvilConfig,
}

impl Default for AnvilRegistry {
    fn default() -> Self {
        Self {
            config: AnvilConfig::default(),
        }
    }
}

impl AnvilRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if !paths.anvil_file.exists() {
            let registry = Self::default();
            registry.save(paths)?;
            return Ok(registry);
        }

        let content = fs::read_to_string(&paths.anvil_file)?;
        let registry: Self = toml::from_str(&content).map_err(|e| {
            CraftError::Config(format!("Failed to parse anvil configuration: {e}"))
        })?;
        Ok(registry)
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        if let Some(parent) = paths.anvil_file.parent() {
            fs::create_dir_all(parent)?;
        }
        if let Some(parent) = paths.anvil_lock.parent() {
            fs::create_dir_all(parent)?;
        }

        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&paths.anvil_lock)?;
        lock_file.lock_exclusive()?;

        let content = toml::to_string_pretty(self).map_err(|e| {
            CraftError::Config(format!("Failed to serialize anvil configuration: {e}"))
        })?;
        fs::write(&paths.anvil_file, content)?;
        let _ = lock_file.unlock();
        Ok(())
    }
}
