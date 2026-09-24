use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fmt;
use std::fs::{self, OpenOptions};
use std::str::FromStr;
use std::time::{SystemTime, UNIX_EPOCH};

pub const PAGE_SIZE_BYTES: usize = 4096;
pub const HUGEPAGE_SIZE_BYTES: usize = 2 * 1024 * 1024; // 2MB
pub const MAX_BUDDY_ORDER: usize = 10; // Orders 0 (4KB) to 10 (4MB)

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThpMode {
    Always,
    Madvise,
    Never,
}

impl fmt::Display for ThpMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThpMode::Always => write!(f, "always"),
            ThpMode::Madvise => write!(f, "madvise"),
            ThpMode::Never => write!(f, "never"),
        }
    }
}

impl FromStr for ThpMode {
    type Err = CraftError;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().trim() {
            "always" => Ok(ThpMode::Always),
            "madvise" => Ok(ThpMode::Madvise),
            "never" => Ok(ThpMode::Never),
            other => Err(CraftError::Config(format!(
                "Invalid THP mode '{}'. Supported: always, madvise, never",
                other
            ))),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ThpDefragMode {
    Always,
    Defer,
    DeferMadvise,
    Madvise,
    Never,
}

impl fmt::Display for ThpDefragMode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ThpDefragMode::Always => write!(f, "always"),
            ThpDefragMode::Defer => write!(f, "defer"),
            ThpDefragMode::DeferMadvise => write!(f, "defer+madvise"),
            ThpDefragMode::Madvise => write!(f, "madvise"),
            ThpDefragMode::Never => write!(f, "never"),
        }
    }
}

impl FromStr for ThpDefragMode {
    type Err = CraftError;
    fn from_str(s: &str) -> std::result::Result<Self, Self::Err> {
        match s.to_lowercase().trim() {
            "always" => Ok(ThpDefragMode::Always),
            "defer" => Ok(ThpDefragMode::Defer),
            "defer+madvise" | "defer_madvise" => Ok(ThpDefragMode::DeferMadvise),
            "madvise" => Ok(ThpDefragMode::Madvise),
            "never" => Ok(ThpDefragMode::Never),
            other => Err(CraftError::Config(format!(
                "Invalid THP defrag mode '{}'. Supported: always, defer, defer+madvise, madvise, never",
                other
            ))),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ThpStatus {
    pub enabled: ThpMode,
    pub defrag: ThpDefragMode,
    pub khugepaged_scan_sleep_ms: u32,
    pub khugepaged_pages_to_scan: u32,
    pub khugepaged_max_ptes_none: u32,
    pub alloc_stalls_total: u64,
    pub compact_fail_total: u64,
    pub hugepages_allocated: u64,
    pub hugepages_split: u64,
}

impl Default for ThpStatus {
    fn default() -> Self {
        Self {
            enabled: ThpMode::Madvise,
            defrag: ThpDefragMode::Madvise,
            khugepaged_scan_sleep_ms: 10000,
            khugepaged_pages_to_scan: 4096,
            khugepaged_max_ptes_none: 511,
            alloc_stalls_total: 0,
            compact_fail_total: 0,
            hugepages_allocated: 128,
            hugepages_split: 4,
        }
    }
}

impl ThpStatus {
    pub fn read_system_or_default() -> Self {
        #[cfg(target_os = "linux")]
        {
            let mut status = Self::default();
            if let Ok(content) = fs::read_to_string("/sys/kernel/mm/transparent_hugepage/enabled") {
                if content.contains("[always]") {
                    status.enabled = ThpMode::Always;
                } else if content.contains("[madvise]") {
                    status.enabled = ThpMode::Madvise;
                } else if content.contains("[never]") {
                    status.enabled = ThpMode::Never;
                }
            }
            if let Ok(content) = fs::read_to_string("/sys/kernel/mm/transparent_hugepage/defrag") {
                if content.contains("[always]") {
                    status.defrag = ThpDefragMode::Always;
                } else if content.contains("[defer+madvise]") {
                    status.defrag = ThpDefragMode::DeferMadvise;
                } else if content.contains("[defer]") {
                    status.defrag = ThpDefragMode::Defer;
                } else if content.contains("[madvise]") {
                    status.defrag = ThpDefragMode::Madvise;
                } else if content.contains("[never]") {
                    status.defrag = ThpDefragMode::Never;
                }
            }
            status
        }
        #[cfg(not(target_os = "linux"))]
        {
            Self::default()
        }
    }
}

/// Buddy allocator representation tracking free page distributions across orders 0..10
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BuddyAllocatorState {
    pub free_pages_per_order: [u64; 11],
}

impl Default for BuddyAllocatorState {
    fn default() -> Self {
        // Realistic distribution with some fragmentation
        Self {
            free_pages_per_order: [
                1024, // Order 0: 4KB (4MB)
                512,  // Order 1: 8KB (4MB)
                256,  // Order 2: 16KB (4MB)
                128,  // Order 3: 32KB (4MB)
                64,   // Order 4: 64KB (4MB)
                32,   // Order 5: 128KB (4MB)
                16,   // Order 6: 256KB (4MB)
                8,    // Order 7: 512KB (4MB)
                4,    // Order 8: 1MB (4MB)
                16,   // Order 9: 2MB Hugepages (32MB)
                8,    // Order 10: 4MB (32MB)
            ],
        }
    }
}

impl BuddyAllocatorState {
    /// Calculates the memory fragmentation index F in [0.0, 1.0] for a target order.
    /// Higher values indicate higher fragmentation (inability to satisfy contiguous blocks).
    pub fn fragmentation_index(&self, target_order: usize) -> f64 {
        let order = target_order.min(MAX_BUDDY_ORDER);
        let mut total_free_pages = 0u64;
        let mut target_and_above_pages = 0u64;

        for (i, count) in self.free_pages_per_order.iter().enumerate() {
            let pages = count * (1 << i);
            total_free_pages += pages;
            if i >= order {
                target_and_above_pages += pages;
            }
        }

        if total_free_pages == 0 {
            return 1.0;
        }

        let ratio = (target_and_above_pages as f64) / (total_free_pages as f64);
        (1.0 - ratio).clamp(0.0, 1.0)
    }

    /// Simulates defragmentation / compaction by migrating small scattered order pages
    /// into coalesced higher-order buddy blocks.
    pub fn compact(&mut self) -> (u64, u64) {
        let mut migrated = 0u64;
        let mut hugepages_formed = 0u64;

        // Coalesce lower orders 0..8 towards higher orders, forming order 9 (2MB hugepages)
        for order in 0..9 {
            let count = self.free_pages_per_order[order];
            if count >= 2 {
                let to_merge = count / 2;
                self.free_pages_per_order[order] -= to_merge * 2;
                self.free_pages_per_order[order + 1] += to_merge;
                migrated += to_merge * (1 << order);
                if order + 1 == 9 {
                    hugepages_formed += to_merge;
                }
            }
        }

        (migrated, hugepages_formed)
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PagePoolConfig {
    pub page_size: usize,
    pub hugepage_mode: ThpMode,
    pub compaction_threshold_percent: f64,
    pub proactive_defrag_interval_secs: u64,
    pub min_free_kbytes: u64,
}

impl Default for PagePoolConfig {
    fn default() -> Self {
        Self {
            page_size: PAGE_SIZE_BYTES,
            hugepage_mode: ThpMode::Madvise,
            compaction_threshold_percent: 65.0,
            proactive_defrag_interval_secs: 60,
            min_free_kbytes: 65536,
        }
    }
}

#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct PagePoolStats {
    pub total_pages_allocated: u64,
    pub pages_recycled: u64,
    pub active_pages: usize,
    pub allocation_stalls: u64,
    pub zero_alloc_hits: u64,
    pub pool_exhaustions: u64,
    pub fragmentation_ratio: f64,
    pub hugepages_allocated: u64,
    pub hugepages_split: u64,
    pub defrag_cycles_completed: u64,
    pub recycled_bytes: u64,
}

impl PagePoolStats {
    pub fn recycle_efficiency(&self) -> f64 {
        if self.total_pages_allocated == 0 {
            100.0
        } else {
            ((self.pages_recycled as f64) / (self.total_pages_allocated as f64) * 100.0)
                .clamp(0.0, 100.0)
        }
    }

    pub fn fast_path_reuse_ratio(&self) -> f64 {
        if self.total_pages_allocated == 0 {
            1.0
        } else {
            ((self.pages_recycled as f64) / (self.total_pages_allocated as f64)).clamp(0.0, 1.0)
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CompactionStatus {
    Success,
    Partial,
    NoOp,
}

impl fmt::Display for CompactionStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompactionStatus::Success => write!(f, "success"),
            CompactionStatus::Partial => write!(f, "partial"),
            CompactionStatus::NoOp => write!(f, "no_op"),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompactionCycleResult {
    pub cycle_id: String,
    pub timestamp_epoch: u64,
    pub initial_fragmentation: f64,
    pub final_fragmentation: f64,
    pub pages_migrated: u64,
    pub pages_freed: u64,
    pub hugepages_formed: u64,
    pub duration_ms: u64,
    pub status: CompactionStatus,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompactionStatusSummary {
    pub thp_status: ThpStatus,
    pub fragmentation_index: f64,
    pub fragmentation_threshold: f64,
    pub compaction_needed: bool,
    pub total_pages_allocated: u64,
    pub pages_recycled: u64,
    pub recycle_efficiency_percent: f64,
    pub zero_alloc_hits: u64,
    pub allocation_stalls: u64,
    pub total_cycles_completed: u64,
    pub last_cycle: Option<CompactionCycleResult>,
}

pub struct CompactionOrchestrator {
    pub config: PagePoolConfig,
    pub buddy_state: BuddyAllocatorState,
    pub stats: PagePoolStats,
    pub thp_status: ThpStatus,
}

impl Default for CompactionOrchestrator {
    fn default() -> Self {
        Self::new(PagePoolConfig::default())
    }
}

impl CompactionOrchestrator {
    pub fn new(config: PagePoolConfig) -> Self {
        let thp_status = ThpStatus::read_system_or_default();
        let buddy_state = BuddyAllocatorState::default();
        let frag = buddy_state.fragmentation_index(9); // 2MB hugepage target
        let mut stats = PagePoolStats::default();
        stats.fragmentation_ratio = frag;
        stats.hugepages_allocated = thp_status.hugepages_allocated;
        stats.hugepages_split = thp_status.hugepages_split;

        Self {
            config,
            buddy_state,
            stats,
            thp_status,
        }
    }

    pub fn evaluate_fragmentation(&self) -> f64 {
        self.buddy_state.fragmentation_index(9)
    }

    pub fn trigger_compaction(&mut self) -> CompactionCycleResult {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        let initial_frag = self.evaluate_fragmentation();
        let cycle_id = format!("compact-{:x}", now);

        let (migrated, hugepages_formed) = self.buddy_state.compact();
        let final_frag = self.evaluate_fragmentation();

        self.stats.defrag_cycles_completed += 1;
        self.stats.fragmentation_ratio = final_frag;
        self.stats.hugepages_allocated += hugepages_formed;

        let status = if final_frag < initial_frag {
            CompactionStatus::Success
        } else if migrated > 0 {
            CompactionStatus::Partial
        } else {
            CompactionStatus::NoOp
        };

        CompactionCycleResult {
            cycle_id,
            timestamp_epoch: now,
            initial_fragmentation: initial_frag,
            final_fragmentation: final_frag,
            pages_migrated: migrated,
            pages_freed: migrated / 2,
            hugepages_formed,
            duration_ms: 12,
            status,
        }
    }

    pub fn tune_thp(&mut self, mode: ThpMode, defrag: ThpDefragMode) -> Result<()> {
        self.thp_status.enabled = mode;
        self.thp_status.defrag = defrag;
        self.config.hugepage_mode = mode;
        Ok(())
    }

    pub fn record_allocation(&mut self, pages: u64, recycled: bool) {
        self.stats.total_pages_allocated += pages;
        if recycled {
            self.stats.pages_recycled += pages;
            self.stats.zero_alloc_hits += pages;
            self.stats.recycled_bytes += pages * (PAGE_SIZE_BYTES as u64);
        }
    }

    pub fn record_stall(&mut self) {
        self.stats.allocation_stalls += 1;
    }

    pub fn get_status_summary(&self, last_cycle: Option<CompactionCycleResult>) -> CompactionStatusSummary {
        let frag = self.evaluate_fragmentation();
        let needed = (frag * 100.0) >= self.config.compaction_threshold_percent;

        CompactionStatusSummary {
            thp_status: self.thp_status.clone(),
            fragmentation_index: frag,
            fragmentation_threshold: self.config.compaction_threshold_percent / 100.0,
            compaction_needed: needed,
            total_pages_allocated: self.stats.total_pages_allocated,
            pages_recycled: self.stats.pages_recycled,
            recycle_efficiency_percent: self.stats.recycle_efficiency(),
            zero_alloc_hits: self.stats.zero_alloc_hits,
            allocation_stalls: self.stats.allocation_stalls,
            total_cycles_completed: self.stats.defrag_cycles_completed,
            last_cycle,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CompactionRegistry {
    pub version: u32,
    pub config: PagePoolConfig,
    pub buddy_state: BuddyAllocatorState,
    pub history: Vec<CompactionCycleResult>,
    pub last_compaction_epoch: u64,
    pub updated_at_epoch: u64,
}

impl Default for CompactionRegistry {
    fn default() -> Self {
        Self {
            version: 1,
            config: PagePoolConfig::default(),
            buddy_state: BuddyAllocatorState::default(),
            history: Vec::new(),
            last_compaction_epoch: 0,
            updated_at_epoch: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}

impl CompactionRegistry {
    pub fn record_cycle(&mut self, cycle: CompactionCycleResult) {
        self.last_compaction_epoch = cycle.timestamp_epoch;
        self.updated_at_epoch = cycle.timestamp_epoch;
        if self.history.len() >= 50 {
            self.history.remove(0);
        }
        self.history.push(cycle);
    }

    pub fn load(paths: &CraftPaths) -> Result<Self> {
        let file = &paths.compaction_registry_file;
        if !file.exists() {
            return Ok(Self::default());
        }

        let lock_path = &paths.compaction_lock;
        if let Some(parent) = lock_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .map_err(|e| CraftError::Io(e))?;
        lock_file
            .lock_shared()
            .map_err(|e| CraftError::Other(format!("Failed to lock compaction registry: {}", e)))?;

        let res = (|| {
            let content = fs::read_to_string(file).map_err(|e| CraftError::Io(e))?;
            serde_json::from_str::<Self>(&content).map_err(|e| {
                CraftError::Config(format!("Failed to parse compaction registry JSON: {}", e))
            })
        })();

        let _ = lock_file.unlock();
        res
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let file = &paths.compaction_registry_file;
        if let Some(parent) = file.parent() {
            fs::create_dir_all(parent).map_err(|e| CraftError::Io(e))?;
        }

        let lock_path = &paths.compaction_lock;
        if let Some(parent) = lock_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)
            .map_err(|e| CraftError::Io(e))?;
        lock_file
            .lock_exclusive()
            .map_err(|e| CraftError::Other(format!("Failed to lock compaction registry: {}", e)))?;

        let res = (|| {
            let json = serde_json::to_string_pretty(self).map_err(|e| {
                CraftError::Config(format!("Failed to serialize compaction registry: {}", e))
            })?;
            fs::write(file, json).map_err(|e| CraftError::Io(e))
        })();

        let _ = lock_file.unlock();
        res
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buddy_fragmentation_index() {
        let mut state = BuddyAllocatorState::default();
        let frag_init = state.fragmentation_index(9);
        assert!(frag_init > 0.0 && frag_init < 1.0);

        let (migrated, hugepages) = state.compact();
        assert!(migrated > 0);
        assert!(hugepages > 0);

        let frag_after = state.fragmentation_index(9);
        assert!(frag_after < frag_init, "Compaction must reduce fragmentation index");
    }

    #[test]
    fn test_compaction_orchestrator_cycle() {
        let mut orch = CompactionOrchestrator::default();
        let cycle = orch.trigger_compaction();
        assert_eq!(cycle.status, CompactionStatus::Success);
        assert!(cycle.pages_migrated > 0);
        assert!(cycle.final_fragmentation <= cycle.initial_fragmentation);
    }

    #[test]
    fn test_page_pool_stats_and_recycling() {
        let mut orch = CompactionOrchestrator::default();
        orch.record_allocation(100, true);
        orch.record_allocation(100, false);
        assert_eq!(orch.stats.total_pages_allocated, 200);
        assert_eq!(orch.stats.pages_recycled, 100);
        assert_eq!(orch.stats.recycle_efficiency(), 50.0);
    }

    #[test]
    fn test_thp_mode_parsing() {
        assert_eq!(ThpMode::from_str("always").unwrap(), ThpMode::Always);
        assert_eq!(ThpMode::from_str("madvise").unwrap(), ThpMode::Madvise);
        assert_eq!(ThpMode::from_str("never").unwrap(), ThpMode::Never);
        assert!(ThpMode::from_str("invalid").is_err());
    }

    #[test]
    fn test_compaction_registry_persistence() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = CraftPaths::from_base(tmp.path().to_path_buf());

        let mut reg = CompactionRegistry::default();
        reg.last_compaction_epoch = 12345;
        reg.save(&paths).unwrap();

        let loaded = CompactionRegistry::load(&paths).unwrap();
        assert_eq!(loaded.last_compaction_epoch, 12345);
    }
}
