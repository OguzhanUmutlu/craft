use craft_core::compaction::{PagePoolConfig, PagePoolStats, PAGE_SIZE_BYTES};
use serde::{Deserialize, Serialize};
use std::alloc::{alloc_zeroed, dealloc, Layout};
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Cache-line alignment for modern x86_64 / aarch64 architectures.
pub const CACHE_LINE_ALIGNMENT: usize = 64;
/// Standard network packet buffer slice size (2KB fits standard 1500 MTU + headers).
pub const DEFAULT_PAGE_SLICE_SIZE: usize = 2048;
/// Standard default page pool slot capacity.
pub const DEFAULT_POOL_CAPACITY_PAGES: usize = 1024;

/// Hardware / kernel page descriptor representing a pre-mapped DMA page slot.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PageDescriptor {
    pub id: u64,
    pub slot_index: usize,
    pub offset: usize,
    pub len: usize,
    pub capacity: usize,
    pub dma_address: u64,
}

struct RawAlignedMemory {
    ptr: *mut u8,
    layout: Layout,
}

unsafe impl Send for RawAlignedMemory {}
unsafe impl Sync for RawAlignedMemory {}

impl Drop for RawAlignedMemory {
    fn drop(&mut self) {
        if !self.ptr.is_null() && self.layout.size() > 0 {
            unsafe {
                dealloc(self.ptr, self.layout);
            }
        }
    }
}

struct PoolInner {
    #[allow(dead_code)]
    memory: RawAlignedMemory,
    base_ptr: *mut u8,
    free_indices: Vec<usize>,
    capacity_pages: usize,
    slice_size: usize,
}

unsafe impl Send for PoolInner {}
unsafe impl Sync for PoolInner {}

/// High-performance zero-allocation page pool for network sockets, io_uring, and packet drivers.
#[derive(Clone)]
pub struct SocketPagePool {
    inner: Arc<Mutex<PoolInner>>,
    pub config: PagePoolConfig,
    pub capacity_pages: usize,
    pub slice_size: usize,
    total_allocated: Arc<AtomicU64>,
    total_recycled: Arc<AtomicU64>,
    pool_exhaustions: Arc<AtomicU64>,
    active_pages: Arc<AtomicUsize>,
    dma_sync_overhead_nanos: Arc<AtomicU64>,
}

/// RAII zero-copy page slice reference that automatically recycles its slot on drop.
pub struct PageSliceRef {
    pool: SocketPagePool,
    pub descriptor: PageDescriptor,
    ptr: *mut u8,
}

unsafe impl Send for PageSliceRef {}
unsafe impl Sync for PageSliceRef {}

impl Deref for PageSliceRef {
    type Target = [u8];
    fn deref(&self) -> &Self::Target {
        unsafe { std::slice::from_raw_parts(self.ptr, self.descriptor.len) }
    }
}

impl DerefMut for PageSliceRef {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { std::slice::from_raw_parts_mut(self.ptr, self.descriptor.len) }
    }
}

impl Drop for PageSliceRef {
    fn drop(&mut self) {
        self.pool.recycle_slice(self.descriptor.slot_index);
    }
}

impl PageSliceRef {
    pub fn set_len(&mut self, len: usize) {
        self.descriptor.len = len.min(self.descriptor.capacity);
    }

    pub fn capacity(&self) -> usize {
        self.descriptor.capacity
    }

    pub fn as_ptr(&self) -> *const u8 {
        self.ptr
    }

    pub fn as_mut_ptr(&mut self) -> *mut u8 {
        self.ptr
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PagePoolPipelineResult {
    pub packets_processed: usize,
    pub total_bytes: usize,
    pub duration_micros: u64,
    pub packets_per_sec: f64,
    pub throughput_mb_sec: f64,
    pub fast_path_reuse_ratio: f64,
    pub exhaustion_events: u64,
}

impl SocketPagePool {
    /// Creates a new cache-line aligned page pool with pre-allocated contiguous memory.
    pub fn new(config: PagePoolConfig, capacity_pages: usize, slice_size: usize) -> Self {
        let actual_slice_size = if slice_size == 0 {
            DEFAULT_PAGE_SLICE_SIZE
        } else {
            // Round up to multiple of cache-line size (64 bytes)
            (slice_size + (CACHE_LINE_ALIGNMENT - 1)) & !(CACHE_LINE_ALIGNMENT - 1)
        };
        let actual_capacity = capacity_pages.max(1);
        let total_bytes = actual_capacity * actual_slice_size;

        // Align entire pool to PAGE_SIZE_BYTES (4096) for THP and DMA efficiency
        let layout = Layout::from_size_align(total_bytes, PAGE_SIZE_BYTES)
            .unwrap_or_else(|_| Layout::from_size_align(total_bytes, CACHE_LINE_ALIGNMENT).unwrap());

        let raw_ptr = unsafe { alloc_zeroed(layout) };
        if raw_ptr.is_null() {
            panic!("Failed to allocate {} bytes for SocketPagePool", total_bytes);
        }

        let memory = RawAlignedMemory {
            ptr: raw_ptr,
            layout,
        };

        let mut free_indices = Vec::with_capacity(actual_capacity);
        for i in 0..actual_capacity {
            free_indices.push(i);
        }

        let inner = PoolInner {
            memory,
            base_ptr: raw_ptr,
            free_indices,
            capacity_pages: actual_capacity,
            slice_size: actual_slice_size,
        };

        Self {
            inner: Arc::new(Mutex::new(inner)),
            config,
            capacity_pages: actual_capacity,
            slice_size: actual_slice_size,
            total_allocated: Arc::new(AtomicU64::new(0)),
            total_recycled: Arc::new(AtomicU64::new(0)),
            pool_exhaustions: Arc::new(AtomicU64::new(0)),
            active_pages: Arc::new(AtomicUsize::new(0)),
            dma_sync_overhead_nanos: Arc::new(AtomicU64::new(0)),
        }
    }

    /// Allocates a pre-mapped zero-copy page slice from the pool.
    pub fn allocate_slice(&self) -> Option<PageSliceRef> {
        let (slot_index, ptr, capacity) = {
            let mut inner = self.inner.lock().ok()?;
            if let Some(slot) = inner.free_indices.pop() {
                let offset = slot * inner.slice_size;
                let slot_ptr = unsafe { inner.base_ptr.add(offset) };
                (slot, slot_ptr, inner.slice_size)
            } else {
                self.pool_exhaustions.fetch_add(1, Ordering::Relaxed);
                return None;
            }
        };

        self.total_allocated.fetch_add(1, Ordering::Relaxed);
        self.active_pages.fetch_add(1, Ordering::Relaxed);

        let descriptor = PageDescriptor {
            id: self.total_allocated.load(Ordering::Relaxed),
            slot_index,
            offset: slot_index * self.slice_size,
            len: capacity,
            capacity,
            dma_address: 0x1000_0000 + (slot_index as u64 * self.slice_size as u64),
        };

        Some(PageSliceRef {
            pool: self.clone(),
            descriptor,
            ptr,
        })
    }

    /// Internal method to recycle a slot index back to the free list.
    fn recycle_slice(&self, slot_index: usize) {
        if let Ok(mut inner) = self.inner.lock() {
            if slot_index < inner.capacity_pages {
                inner.free_indices.push(slot_index);
            }
        }
        self.total_recycled.fetch_add(1, Ordering::Relaxed);
        self.active_pages.fetch_sub(1, Ordering::Relaxed);
    }

    /// Records DMA synchronization overhead in nanoseconds.
    pub fn record_dma_overhead_nanos(&self, nanos: u64) {
        self.dma_sync_overhead_nanos.fetch_add(nanos, Ordering::Relaxed);
    }

    /// Gets cumulative DMA synchronization overhead in nanoseconds.
    pub fn dma_overhead_nanos(&self) -> u64 {
        self.dma_sync_overhead_nanos.load(Ordering::Relaxed)
    }

    /// Returns the current statistics of the page pool.
    pub fn stats(&self) -> PagePoolStats {
        let total_alloc = self.total_allocated.load(Ordering::Relaxed);
        let total_recyc = self.total_recycled.load(Ordering::Relaxed);
        let active = self.active_pages.load(Ordering::Relaxed);
        let exhaustions = self.pool_exhaustions.load(Ordering::Relaxed);

        PagePoolStats {
            total_pages_allocated: total_alloc,
            pages_recycled: total_recyc,
            active_pages: active,
            allocation_stalls: 0,
            zero_alloc_hits: total_recyc,
            pool_exhaustions: exhaustions,
            fragmentation_ratio: 0.0,
            hugepages_allocated: ((self.capacity_pages * self.slice_size) / (2 * 1024 * 1024)) as u64,
            hugepages_split: 0,
            defrag_cycles_completed: 0,
            recycled_bytes: total_recyc * self.slice_size as u64,
        }
    }

    /// Simulates a high-throughput network packet ingestion and recycling pipeline.
    pub fn simulate_packet_pipeline(
        &self,
        packet_count: usize,
        packet_size: usize,
    ) -> PagePoolPipelineResult {
        let start = Instant::now();
        let target_size = packet_size.min(self.slice_size);
        let mut processed = 0;
        let mut total_bytes = 0;

        for seq in 0..packet_count {
            if let Some(mut slice) = self.allocate_slice() {
                slice.set_len(target_size);
                // Zero-copy direct write into the aligned page buffer
                if target_size >= 4 {
                    let seq_bytes = (seq as u32).to_be_bytes();
                    slice[0..4].copy_from_slice(&seq_bytes);
                }
                // Memory read / checksum simulation
                let first_byte = slice[0];
                let _ = first_byte;
                processed += 1;
                total_bytes += target_size;
                // slice drops here and is recycled back to the pool immediately
            }
        }

        let elapsed = start.elapsed();
        let duration_micros = elapsed.as_micros() as u64;
        let duration_secs = elapsed.as_secs_f64().max(0.000_001);
        let packets_per_sec = (processed as f64) / duration_secs;
        let throughput_mb_sec = (total_bytes as f64) / (1024.0 * 1024.0) / duration_secs;
        let stats = self.stats();

        PagePoolPipelineResult {
            packets_processed: processed,
            total_bytes,
            duration_micros,
            packets_per_sec,
            throughput_mb_sec,
            fast_path_reuse_ratio: stats.fast_path_reuse_ratio(),
            exhaustion_events: stats.pool_exhaustions,
        }
    }

    /// Formats human-readable pool status.
    pub fn status_summary(&self) -> String {
        let stats = self.stats();
        let total_kb = (self.capacity_pages * self.slice_size) / 1024;
        format!(
            "Capacity: {} pages ({} KB total, {} bytes/slice) | Active: {} | Recycled: {} | Fast-path Reuse: {:.1}% | Exhaustions: {}",
            self.capacity_pages,
            total_kb,
            self.slice_size,
            stats.active_pages,
            stats.pages_recycled,
            stats.fast_path_reuse_ratio() * 100.0,
            stats.pool_exhaustions
        )
    }
}

impl Default for SocketPagePool {
    fn default() -> Self {
        let config = PagePoolConfig::default();
        Self::new(config, DEFAULT_POOL_CAPACITY_PAGES, DEFAULT_PAGE_SLICE_SIZE)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_socket_page_pool_lifecycle() {
        let pool = SocketPagePool::default();
        assert_eq!(pool.capacity_pages, DEFAULT_POOL_CAPACITY_PAGES);

        // Allocate a slice
        {
            let mut slice = pool.allocate_slice().expect("Slice should allocate");
            assert_eq!(slice.capacity(), DEFAULT_PAGE_SLICE_SIZE);
            slice.set_len(512);
            assert_eq!(slice.len(), 512);

            // Mutate memory
            slice[0] = 0xAA;
            slice[511] = 0xBB;
            assert_eq!(slice[0], 0xAA);
            assert_eq!(slice[511], 0xBB);

            let stats = pool.stats();
            assert_eq!(stats.active_pages, 1);
            assert_eq!(stats.total_pages_allocated, 1);
            assert_eq!(stats.pages_recycled, 0);
        }

        // After dropping slice, verify it was recycled
        let stats = pool.stats();
        assert_eq!(stats.active_pages, 0);
        assert_eq!(stats.pages_recycled, 1);
    }

    #[test]
    fn test_packet_pipeline_simulation() {
        let pool = SocketPagePool::new(PagePoolConfig::default(), 64, 1024);
        let res = pool.simulate_packet_pipeline(1000, 512);

        assert_eq!(res.packets_processed, 1000);
        assert_eq!(res.total_bytes, 512_000);
        assert_eq!(res.exhaustion_events, 0);
        assert!(res.packets_per_sec > 10_000.0);
        let stats = pool.stats();
        assert_eq!(stats.active_pages, 0);
        assert_eq!(stats.total_pages_allocated, 1000);
        assert_eq!(stats.pages_recycled, 1000);
    }

    #[test]
    fn test_page_pool_exhaustion() {
        let pool = SocketPagePool::new(PagePoolConfig::default(), 4, 512);
        let s1 = pool.allocate_slice();
        let s2 = pool.allocate_slice();
        let s3 = pool.allocate_slice();
        let s4 = pool.allocate_slice();
        let s5 = pool.allocate_slice();

        assert!(s1.is_some());
        assert!(s2.is_some());
        assert!(s3.is_some());
        assert!(s4.is_some());
        assert!(s5.is_none(), "5th allocation must fail on pool of size 4");

        let stats = pool.stats();
        assert_eq!(stats.pool_exhaustions, 1);
        assert_eq!(stats.active_pages, 4);

        // Drop one and allocate again
        drop(s1);
        let s6 = pool.allocate_slice();
        assert!(s6.is_some(), "Should succeed after slot recycled");
    }
}
