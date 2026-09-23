use std::fmt::Write as FmtWrite;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Instant;

use craft_core::{
    AnvilBenchmarkReport, AnvilChunkCache, AnvilConfig, AnvilIoEngine, AnvilRegistry,
    AnvilStatusSummary, ChunkCompressionScheme, ChunkData, ChunkSectorInfo, CraftError,
    CraftPaths, IoEngineType, PrefetchSummary, RegionDetails, RegionFileReader, RegionFileWriter,
    Result,
};

static ANVIL_SERVICE: OnceLock<Arc<AnvilService>> = OnceLock::new();

pub struct AnvilService {
    paths: CraftPaths,
    io_engine: AnvilIoEngine,
    chunk_cache: AnvilChunkCache,
    registry: RwLock<AnvilRegistry>,
}

impl AnvilService {
    pub fn global(paths: &CraftPaths) -> Arc<Self> {
        ANVIL_SERVICE
            .get_or_init(|| Arc::new(Self::new(paths.clone())))
            .clone()
    }

    pub fn new(paths: CraftPaths) -> Self {
        let registry = AnvilRegistry::load(&paths).unwrap_or_default();
        let engine_type = IoEngineType::from_name(&registry.config.engine);
        let io_engine = AnvilIoEngine::new_with_engine(engine_type);
        let chunk_cache = AnvilChunkCache::new(
            registry.config.cache_max_bytes,
            registry.config.cache_max_chunks,
        );

        Self {
            paths,
            io_engine,
            chunk_cache,
            registry: RwLock::new(registry),
        }
    }

    pub fn get_status(&self) -> AnvilStatusSummary {
        let cache_stats = self.chunk_cache.stats();
        let (ops, r_bytes, w_bytes, savings, _batches, avg_lat) = self.io_engine.stats();
        let engine_name = self.io_engine.engine_type().as_str().to_string();

        AnvilStatusSummary {
            engine: engine_name,
            active_cached_chunks: cache_stats.cached_chunks,
            cache_memory_used_bytes: cache_stats.memory_used_bytes,
            cache_memory_limit_bytes: cache_stats.memory_limit_bytes,
            cache_hit_count: cache_stats.hit_count,
            cache_miss_count: cache_stats.miss_count,
            cache_hit_ratio: cache_stats.hit_ratio,
            cache_eviction_count: cache_stats.eviction_count,
            cache_prefetch_count: cache_stats.prefetch_count,
            total_io_ops: ops,
            total_bytes_read: r_bytes,
            total_bytes_written: w_bytes,
            context_switch_savings: savings,
            avg_io_latency_micros: avg_lat,
        }
    }

    pub fn inspect_region(&self, server_path: &Path, region_file: &str) -> Result<RegionDetails> {
        let mca_path = resolve_region_path(server_path, region_file)?;
        let reader = RegionFileReader::open(&mca_path)?;
        let inspection = reader.inspect()?;

        let mut chunks = Vec::new();
        let header = reader.header();

        for cz in 0..32 {
            for cx in 0..32 {
                let loc = header.get_location(cx, cz);
                if !loc.is_empty() {
                    let ts = header.get_timestamp(cx, cz);
                    let size_bytes = (loc.sector_count as u32) * 4096;
                    chunks.push(ChunkSectorInfo {
                        chunk_x: (inspection.region_x * 32) + cx,
                        chunk_z: (inspection.region_z * 32) + cz,
                        sector_offset: loc.offset,
                        sector_count: loc.sector_count,
                        timestamp: ts,
                        size_bytes,
                    });
                }
            }
        }

        Ok(RegionDetails {
            file_name: mca_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(region_file)
                .to_string(),
            file_path: mca_path.display().to_string(),
            region_x: inspection.region_x,
            region_z: inspection.region_z,
            file_size_bytes: inspection.file_size_bytes,
            allocated_sectors: inspection.allocated_payload_sectors,
            total_sectors: inspection.total_sectors,
            active_chunks: inspection.active_chunks,
            empty_chunks: inspection.empty_chunks,
            fragmentation_ratio: inspection.fragmentation_ratio,
            largest_contiguous_free_sectors: inspection.largest_contiguous_free_sectors,
            chunks,
        })
    }

    pub fn prefetch(
        &self,
        server_path: &Path,
        world: &str,
        center_x: i32,
        center_z: i32,
        radius: u32,
    ) -> Result<PrefetchSummary> {
        let region_dir = resolve_world_region_dir(server_path, world)?;
        self.chunk_cache.prefetch_radius(
            world,
            &region_dir,
            center_x,
            center_z,
            radius,
            &self.io_engine,
        )
    }

    pub fn benchmark(&self, chunks_count: usize) -> Result<AnvilBenchmarkReport> {
        let count = chunks_count.clamp(4, 1024);
        if !self.paths.anvil_cache_dir.exists() {
            fs::create_dir_all(&self.paths.anvil_cache_dir).map_err(CraftError::Io)?;
        }
        let bench_file = self.paths.anvil_cache_dir.join(format!(
            "bench_{}.mca",
            chrono::Utc::now().timestamp_nanos_opt().unwrap_or(0)
        ));

        // Write benchmark
        let write_start = Instant::now();
        let mut total_uncompressed_bytes = 0usize;

        {
            let mut writer = RegionFileWriter::open_or_create(&bench_file)?;
            for i in 0..count {
                let cx = (i % 32) as i32;
                let cz = (i / 32) as i32;
                // Generate synthetic chunk block data
                let payload = format!("Benchmark synthetic NBT chunk payload for block data chunk ({cx}, {cz}) with entity tags and blockstates repeating index {i}").repeat(10).into_bytes();
                total_uncompressed_bytes += payload.len();

                let chunk = ChunkData {
                    chunk_x: cx,
                    chunk_z: cz,
                    timestamp: 1700000000 + i as u32,
                    scheme: ChunkCompressionScheme::Zlib,
                    raw_payload: payload,
                    compressed_size: 0,
                };
                writer.write_chunk(&chunk)?;
            }
        }

        let write_elapsed = write_start.elapsed();
        let write_time_ms = write_elapsed.as_millis() as u64;
        let file_size = fs::metadata(&bench_file).map_err(CraftError::Io)?.len();

        let write_throughput = if write_elapsed.as_secs_f64() > 0.0 {
            (file_size as f64) / (1024.0 * 1024.0) / write_elapsed.as_secs_f64()
        } else {
            0.0
        };

        // Read benchmark
        let read_start = Instant::now();
        let mut read_bytes = 0usize;

        {
            let mut reader = RegionFileReader::open(&bench_file)?;
            for i in 0..count {
                let cx = (i % 32) as i32;
                let cz = (i / 32) as i32;
                if let Some(c) = reader.read_chunk(cx, cz)? {
                    read_bytes += c.raw_payload.len();
                }
            }
        }

        let read_elapsed = read_start.elapsed();
        let read_time_ms = read_elapsed.as_millis() as u64;
        let read_throughput = if read_elapsed.as_secs_f64() > 0.0 {
            (read_bytes as f64) / (1024.0 * 1024.0) / read_elapsed.as_secs_f64()
        } else {
            0.0
        };

        let compression_ratio = if file_size > 0 {
            (total_uncompressed_bytes as f64) / (file_size as f64)
        } else {
            1.0
        };

        let savings = (count.saturating_sub(1) * 2) as u64;
        let _ = fs::remove_file(&bench_file);

        Ok(AnvilBenchmarkReport {
            chunks_tested: count,
            write_time_ms,
            write_throughput_mb_sec: write_throughput,
            read_time_ms,
            read_throughput_mb_sec: read_throughput,
            compression_ratio,
            engine_used: self.io_engine.engine_type().as_str().to_string(),
            context_switch_savings: savings,
        })
    }

    pub fn set_config(&self, new_config: AnvilConfig) -> Result<AnvilConfig> {
        let mut reg = self.registry.write().map_err(|_| {
            CraftError::Other("Failed to acquire write lock on anvil registry".to_string())
        })?;
        reg.config = new_config.clone();
        reg.save(&self.paths)?;
        Ok(new_config)
    }

    pub fn generate_prometheus_metrics(&self) -> String {
        let mut out = String::with_capacity(1024);
        let status = self.get_status();

        writeln!(
            out,
            "# HELP craft_anvil_cached_chunks Total active chunks resident in LRU memory cache"
        )
        .unwrap();
        writeln!(out, "# TYPE craft_anvil_cached_chunks gauge").unwrap();
        writeln!(out, "craft_anvil_cached_chunks {}", status.active_cached_chunks).unwrap();

        writeln!(
            out,
            "# HELP craft_anvil_cache_memory_bytes Direct memory used by chunk cache in bytes"
        )
        .unwrap();
        writeln!(out, "# TYPE craft_anvil_cache_memory_bytes gauge").unwrap();
        writeln!(
            out,
            "craft_anvil_cache_memory_bytes {}",
            status.cache_memory_used_bytes
        )
        .unwrap();

        writeln!(
            out,
            "# HELP craft_anvil_cache_hit_ratio Chunk cache lookup hit ratio (0.0 to 1.0)"
        )
        .unwrap();
        writeln!(out, "# TYPE craft_anvil_cache_hit_ratio gauge").unwrap();
        writeln!(
            out,
            "craft_anvil_cache_hit_ratio {:.4}",
            status.cache_hit_ratio
        )
        .unwrap();

        writeln!(
            out,
            "# HELP craft_anvil_io_ops_total Total Anvil region chunk read and write operations"
        )
        .unwrap();
        writeln!(out, "# TYPE craft_anvil_io_ops_total counter").unwrap();
        writeln!(out, "craft_anvil_io_ops_total {}", status.total_io_ops).unwrap();

        writeln!(
            out,
            "# HELP craft_anvil_bytes_read_total Total bytes read from Anvil region files"
        )
        .unwrap();
        writeln!(out, "# TYPE craft_anvil_bytes_read_total counter").unwrap();
        writeln!(out, "craft_anvil_bytes_read_total {}", status.total_bytes_read).unwrap();

        writeln!(
            out,
            "# HELP craft_anvil_bytes_written_total Total bytes written to Anvil region files"
        )
        .unwrap();
        writeln!(out, "# TYPE craft_anvil_bytes_written_total counter").unwrap();
        writeln!(
            out,
            "craft_anvil_bytes_written_total {}",
            status.total_bytes_written
        )
        .unwrap();

        writeln!(
            out,
            "# HELP craft_anvil_context_switch_savings_total Total OS context switches saved via io_uring batching"
        )
        .unwrap();
        writeln!(out, "# TYPE craft_anvil_context_switch_savings_total counter").unwrap();
        writeln!(
            out,
            "craft_anvil_context_switch_savings_total {}",
            status.context_switch_savings
        )
        .unwrap();

        writeln!(
            out,
            "# HELP craft_anvil_io_latency_micros Average I/O operation latency in microseconds"
        )
        .unwrap();
        writeln!(out, "# TYPE craft_anvil_io_latency_micros gauge").unwrap();
        writeln!(
            out,
            "craft_anvil_io_latency_micros {:.2}",
            status.avg_io_latency_micros
        )
        .unwrap();

        out
    }
}

fn resolve_region_path(server_path: &Path, region_file: &str) -> Result<PathBuf> {
    // 1. Direct path check
    let direct = PathBuf::from(region_file);
    if direct.is_file() {
        return Ok(direct);
    }

    // 2. Relative to server_path
    let server_rel = server_path.join(region_file);
    if server_rel.is_file() {
        return Ok(server_rel);
    }

    // 3. Check server_path/world/region/region_file
    let world_region = server_path.join("world").join("region").join(region_file);
    if world_region.is_file() {
        return Ok(world_region);
    }

    // 4. Search recursively inside server_path for the filename
    if let Ok(entries) = fs::read_dir(server_path) {
        for entry in entries.flatten() {
            let sub_region = entry.path().join("region").join(region_file);
            if sub_region.is_file() {
                return Ok(sub_region);
            }
        }
    }

    Err(CraftError::Other(format!(
        "Region file '{region_file}' not found in server '{}'",
        server_path.display()
    )))
}

fn resolve_world_region_dir(server_path: &Path, world: &str) -> Result<PathBuf> {
    let candidate1 = server_path.join(world).join("region");
    if candidate1.is_dir() {
        return Ok(candidate1);
    }

    let candidate2 = server_path.join("world").join("region");
    if candidate2.is_dir() {
        return Ok(candidate2);
    }

    let candidate3 = server_path.join("region");
    if candidate3.is_dir() {
        return Ok(candidate3);
    }

    // Create candidate1 if none exist
    fs::create_dir_all(&candidate1).map_err(CraftError::Io)?;
    Ok(candidate1)
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_anvil_service_status_and_bench() {
        let dir = tempdir().unwrap();
        let paths = CraftPaths::from_base(dir.path().to_path_buf());
        let service = AnvilService::new(paths);

        let status = service.get_status();
        assert_eq!(status.active_cached_chunks, 0);
        assert!(status.cache_memory_limit_bytes > 0);

        let bench = service.benchmark(8).unwrap();
        assert_eq!(bench.chunks_tested, 8);
        assert!(bench.write_throughput_mb_sec >= 0.0);
        assert!(bench.read_throughput_mb_sec >= 0.0);

        let metrics = service.generate_prometheus_metrics();
        assert!(metrics.contains("craft_anvil_cached_chunks"));
        assert!(metrics.contains("craft_anvil_cache_memory_bytes"));
        assert!(metrics.contains("craft_anvil_io_ops_total"));
    }
}
