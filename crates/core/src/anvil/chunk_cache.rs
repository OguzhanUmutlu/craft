use std::collections::HashMap;
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::RwLock;
use std::time::Instant;
use chrono::Utc;
use serde::{Deserialize, Serialize};

use super::io_uring_driver::{AnvilIoEngine, IoBatchRead};
use super::region::{
    decompress_payload, region_coords_from_chunk, ChunkCompressionScheme, RegionFileReader,
    SECTOR_BYTES,
};
use crate::error::Result;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkKey {
    pub world: String,
    pub chunk_x: i32,
    pub chunk_z: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CachedChunk {
    pub key: ChunkKey,
    pub payload: Vec<u8>,
    pub compressed_size: usize,
    pub scheme: ChunkCompressionScheme,
    pub timestamp: u32,
    pub last_accessed_micros: u64,
    pub hit_count: u32,
}

impl CachedChunk {
    pub fn memory_size(&self) -> usize {
        std::mem::size_of::<Self>() + self.payload.len() + self.key.world.len()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnvilCacheStats {
    pub cached_chunks: usize,
    pub memory_used_bytes: usize,
    pub memory_limit_bytes: usize,
    pub hit_count: u64,
    pub miss_count: u64,
    pub hit_ratio: f64,
    pub eviction_count: u64,
    pub prefetch_count: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrefetchSummary {
    pub center_x: i32,
    pub center_z: i32,
    pub radius: u32,
    pub total_candidates: usize,
    pub already_cached: usize,
    pub chunks_loaded: usize,
    pub load_failures: usize,
    pub bytes_loaded: usize,
    pub elapsed_millis: u64,
}

struct CacheInner {
    map: HashMap<ChunkKey, CachedChunk>,
    access_counter: u64,
    current_bytes: usize,
}

pub struct AnvilChunkCache {
    inner: RwLock<CacheInner>,
    max_bytes: usize,
    max_chunks: usize,
    hit_count: AtomicU64,
    miss_count: AtomicU64,
    eviction_count: AtomicU64,
    prefetch_count: AtomicU64,
}

impl AnvilChunkCache {
    pub fn new(max_bytes: usize, max_chunks: usize) -> Self {
        Self {
            inner: RwLock::new(CacheInner {
                map: HashMap::new(),
                access_counter: 0,
                current_bytes: 0,
            }),
            max_bytes: max_bytes.max(1024 * 1024), // Minimum 1MB
            max_chunks: max_chunks.max(64),
            hit_count: AtomicU64::new(0),
            miss_count: AtomicU64::new(0),
            eviction_count: AtomicU64::new(0),
            prefetch_count: AtomicU64::new(0),
        }
    }

    pub fn get(&self, world: &str, chunk_x: i32, chunk_z: i32) -> Option<CachedChunk> {
        let key = ChunkKey {
            world: world.to_string(),
            chunk_x,
            chunk_z,
        };

        let mut inner = self.inner.write().ok()?;
        inner.access_counter += 1;
        let counter = inner.access_counter;

        if let Some(chunk) = inner.map.get_mut(&key) {
            chunk.last_accessed_micros = counter;
            chunk.hit_count = chunk.hit_count.saturating_add(1);
            let clone = chunk.clone();
            drop(inner);
            self.hit_count.fetch_add(1, Ordering::Relaxed);
            Some(clone)
        } else {
            drop(inner);
            self.miss_count.fetch_add(1, Ordering::Relaxed);
            None
        }
    }

    pub fn insert(&self, mut chunk: CachedChunk) {
        let chunk_mem = chunk.memory_size();
        let key = chunk.key.clone();

        let mut inner = match self.inner.write() {
            Ok(lock) => lock,
            Err(e) => e.into_inner(),
        };

        inner.access_counter += 1;
        chunk.last_accessed_micros = inner.access_counter;

        // If replacing existing entry, deduct its size first
        if let Some(old) = inner.map.remove(&key) {
            inner.current_bytes = inner.current_bytes.saturating_sub(old.memory_size());
        }

        // Evict LRU entries until we fit under memory and chunk count limits
        while !inner.map.is_empty()
            && (inner.current_bytes + chunk_mem > self.max_bytes
                || inner.map.len() >= self.max_chunks)
        {
            // Find key with minimum last_accessed_micros
            let lru_key = inner
                .map
                .iter()
                .min_by_key(|(_, val)| val.last_accessed_micros)
                .map(|(k, _)| k.clone());

            if let Some(k) = lru_key {
                if let Some(removed) = inner.map.remove(&k) {
                    inner.current_bytes = inner.current_bytes.saturating_sub(removed.memory_size());
                    self.eviction_count.fetch_add(1, Ordering::Relaxed);
                }
            } else {
                break;
            }
        }

        inner.current_bytes += chunk_mem;
        inner.map.insert(key, chunk);
    }

    pub fn remove(&self, world: &str, chunk_x: i32, chunk_z: i32) -> Option<CachedChunk> {
        let key = ChunkKey {
            world: world.to_string(),
            chunk_x,
            chunk_z,
        };
        let mut inner = self.inner.write().ok()?;
        let removed = inner.map.remove(&key)?;
        inner.current_bytes = inner.current_bytes.saturating_sub(removed.memory_size());
        Some(removed)
    }

    pub fn clear(&self) {
        if let Ok(mut inner) = self.inner.write() {
            inner.map.clear();
            inner.current_bytes = 0;
        }
    }

    pub fn stats(&self) -> AnvilCacheStats {
        let hits = self.hit_count.load(Ordering::Relaxed);
        let misses = self.miss_count.load(Ordering::Relaxed);
        let evictions = self.eviction_count.load(Ordering::Relaxed);
        let prefetch = self.prefetch_count.load(Ordering::Relaxed);

        let total_lookups = hits + misses;
        let hit_ratio = if total_lookups > 0 {
            (hits as f64) / (total_lookups as f64)
        } else {
            0.0
        };

        let (cached_chunks, memory_used_bytes) = if let Ok(inner) = self.inner.read() {
            (inner.map.len(), inner.current_bytes)
        } else {
            (0, 0)
        };

        AnvilCacheStats {
            cached_chunks,
            memory_used_bytes,
            memory_limit_bytes: self.max_bytes,
            hit_count: hits,
            miss_count: misses,
            hit_ratio,
            eviction_count: evictions,
            prefetch_count: prefetch,
        }
    }

    pub fn prefetch_radius<P: AsRef<Path>>(
        &self,
        world: &str,
        world_region_dir: P,
        center_x: i32,
        center_z: i32,
        radius: u32,
        io_engine: &AnvilIoEngine,
    ) -> Result<PrefetchSummary> {
        let start = Instant::now();
        let rad = radius as i32;

        let mut candidate_coords = Vec::new();
        for dz in -rad..=rad {
            for dx in -rad..=rad {
                candidate_coords.push((center_x + dx, center_z + dz));
            }
        }
        let total_candidates = candidate_coords.len();

        // Filter out what's already cached
        let mut already_cached = 0;
        let mut missing_coords = Vec::new();

        {
            let inner = match self.inner.read() {
                Ok(l) => l,
                Err(e) => e.into_inner(),
            };
            for (cx, cz) in candidate_coords {
                let key = ChunkKey {
                    world: world.to_string(),
                    chunk_x: cx,
                    chunk_z: cz,
                };
                if inner.map.contains_key(&key) {
                    already_cached += 1;
                } else {
                    missing_coords.push((cx, cz));
                }
            }
        }

        if missing_coords.is_empty() {
            return Ok(PrefetchSummary {
                center_x,
                center_z,
                radius,
                total_candidates,
                already_cached,
                chunks_loaded: 0,
                load_failures: 0,
                bytes_loaded: 0,
                elapsed_millis: start.elapsed().as_millis() as u64,
            });
        }

        // Group missing coordinates by region filename
        let region_dir = world_region_dir.as_ref();
        let mut chunks_by_region: HashMap<(i32, i32), Vec<(i32, i32)>> = HashMap::new();
        for (cx, cz) in missing_coords {
            let reg = region_coords_from_chunk(cx, cz);
            chunks_by_region.entry(reg).or_default().push((cx, cz));
        }

        let mut chunks_loaded = 0;
        let mut load_failures = 0;
        let mut bytes_loaded = 0;

        for ((rx, rz), chunk_list) in chunks_by_region {
            let mca_path = region_dir.join(format!("r.{rx}.{rz}.mca"));
            if !mca_path.exists() {
                // Region file does not exist on disk yet
                continue;
            }

            // Open region file reader to read header locations
            let reader = match RegionFileReader::open(&mca_path) {
                Ok(r) => r,
                Err(_) => {
                    load_failures += chunk_list.len();
                    continue;
                }
            };

            // Build IO batch requests for all chunks in this region
            let mut io_requests = Vec::new();
            for &(cx, cz) in &chunk_list {
                let loc = reader.header().get_location(cx, cz);
                if !loc.is_empty() {
                    let offset = (loc.offset as u64) * (SECTOR_BYTES as u64);
                    let length = (loc.sector_count as usize) * SECTOR_BYTES;
                    io_requests.push(IoBatchRead {
                        file_path: mca_path.clone(),
                        offset,
                        length,
                        chunk_x: cx,
                        chunk_z: cz,
                    });
                }
            }

            // Submit batch read to io_uring / threaded fallback engine
            let results = io_engine.read_batch(&io_requests);

            for res in results {
                if !res.success || res.data.len() < 5 {
                    load_failures += 1;
                    continue;
                }

                let payload_len = u32::from_be_bytes([
                    res.data[0],
                    res.data[1],
                    res.data[2],
                    res.data[3],
                ]) as usize;

                if payload_len == 0 || payload_len > res.data.len() - 4 {
                    load_failures += 1;
                    continue;
                }

                let scheme_byte = res.data[4];
                let scheme = match ChunkCompressionScheme::from_byte(scheme_byte) {
                    Ok(s) => s,
                    Err(_) => {
                        load_failures += 1;
                        continue;
                    }
                };

                let compressed_size = payload_len.saturating_sub(1);
                let compressed_slice = &res.data[5..5 + compressed_size];

                match decompress_payload(scheme, compressed_slice) {
                    Ok(payload) => {
                        let bytes_len = payload.len();
                        let cached = CachedChunk {
                            key: ChunkKey {
                                world: world.to_string(),
                                chunk_x: res.chunk_x,
                                chunk_z: res.chunk_z,
                            },
                            payload,
                            compressed_size,
                            scheme,
                            timestamp: reader.header().get_timestamp(res.chunk_x, res.chunk_z),
                            last_accessed_micros: Utc::now().timestamp_micros() as u64,
                            hit_count: 0,
                        };
                        self.insert(cached);
                        chunks_loaded += 1;
                        bytes_loaded += bytes_len;
                    }
                    Err(_) => {
                        load_failures += 1;
                    }
                }
            }
        }

        self.prefetch_count
            .fetch_add(chunks_loaded as u64, Ordering::Relaxed);

        Ok(PrefetchSummary {
            center_x,
            center_z,
            radius,
            total_candidates,
            already_cached,
            chunks_loaded,
            load_failures,
            bytes_loaded,
            elapsed_millis: start.elapsed().as_millis() as u64,
        })
    }
}
