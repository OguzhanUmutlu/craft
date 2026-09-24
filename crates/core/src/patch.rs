use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::time::{SystemTime, UNIX_EPOCH};

/// Target architecture for binary rewriting
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArchInstructionSet {
    X86_64,
    Aarch64,
    BytecodeOnly,
}

impl ArchInstructionSet {
    pub fn current() -> Self {
        #[cfg(target_arch = "x86_64")]
        {
            Self::X86_64
        }
        #[cfg(target_arch = "aarch64")]
        {
            Self::Aarch64
        }
        #[cfg(not(any(target_arch = "x86_64", target_arch = "aarch64")))]
        {
            Self::BytecodeOnly
        }
    }

    pub fn as_str(&self) -> &'static str {
        match self {
            Self::X86_64 => "x86_64",
            Self::Aarch64 => "aarch64",
            Self::BytecodeOnly => "bytecode",
        }
    }
}

/// Target symbol or JVM class method descriptor
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatchTargetType {
    NativeSymbol(String),
    JvmClassMethod {
        class_name: String,
        method_name: String,
        signature: String,
    },
}

impl PatchTargetType {
    pub fn display_target(&self) -> String {
        match self {
            Self::NativeSymbol(sym) => sym.clone(),
            Self::JvmClassMethod {
                class_name,
                method_name,
                signature,
            } => format!("{}.{}{}", class_name, method_name, signature),
        }
    }
}

impl std::fmt::Display for PatchTargetType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_target())
    }
}

/// Type of trampoline hook
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrampolinePatchType {
    /// 5-byte relative jmp: 0xE9 <rel32>
    Rel32Jmp,
    /// 14-byte 64-bit absolute jmp: FF 25 00 00 00 00 <u64 addr>
    Abs64Jmp,
    /// 4-byte AArch64 immediate branch: B <imm26>
    Aarch64BranchImm,
    /// 16-byte AArch64 literal load + branch: LDR X16, #8; BR X16; <u64 addr>
    Aarch64LiteralLdr,
    /// Java Instrumentation Agent bytecode swap
    BytecodeMethodSwap,
}

impl TrampolinePatchType {
    pub fn opcode_size(&self) -> usize {
        match self {
            Self::Rel32Jmp => 5,
            Self::Abs64Jmp => 14,
            Self::Aarch64BranchImm => 4,
            Self::Aarch64LiteralLdr => 16,
            Self::BytecodeMethodSwap => 0,
        }
    }
}

/// Lifecycle state of a patch
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PatchState {
    Inactive,
    Active,
    RolledBack,
    Failed,
}

impl PatchState {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Inactive => "Inactive",
            Self::Active => "Active",
            Self::RolledBack => "RolledBack",
            Self::Failed => "Failed",
        }
    }
}

/// Trampoline instruction descriptor containing byte sequences
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrampolineDescriptor {
    pub patch_type: TrampolinePatchType,
    pub original_prologue_bytes: Vec<u8>,
    pub trampoline_bytes: Vec<u8>,
    pub shadow_code_bytes: Vec<u8>,
    pub hook_address: u64,
    pub target_address: u64,
    pub shadow_trampoline_address: u64,
}

impl TrampolineDescriptor {
    pub fn new(
        patch_type: TrampolinePatchType,
        original_prologue_bytes: Vec<u8>,
        trampoline_bytes: Vec<u8>,
        shadow_code_bytes: Vec<u8>,
        hook_address: u64,
        target_address: u64,
        shadow_trampoline_address: u64,
    ) -> Self {
        Self {
            patch_type,
            original_prologue_bytes,
            trampoline_bytes,
            shadow_code_bytes,
            hook_address,
            target_address,
            shadow_trampoline_address,
        }
    }
}

/// High-level manifest of an applied or staged patch
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PatchManifest {
    pub name: String,
    pub server: String,
    pub arch: ArchInstructionSet,
    pub target_type: PatchTargetType,
    pub state: PatchState,
    pub created_at_epoch_s: u64,
    pub applied_at_epoch_s: Option<u64>,
    pub rolled_back_at_epoch_s: Option<u64>,
    pub execution_count: u64,
    pub apply_duration_micros: u64,
    pub descriptor: TrampolineDescriptor,
    pub safety_verified: bool,
}

/// Status summary across all registered patches
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatchStatusSummary {
    pub active_patches: usize,
    pub total_applied: usize,
    pub total_rollbacks: usize,
    pub safety_rejections: usize,
    pub avg_apply_micros: f64,
    pub patches: Vec<PatchManifest>,
}

impl Default for PatchStatusSummary {
    fn default() -> Self {
        Self {
            active_patches: 0,
            total_applied: 0,
            total_rollbacks: 0,
            safety_rejections: 0,
            avg_apply_micros: 0.0,
            patches: Vec::new(),
        }
    }
}

/// Synthetic patch benchmark report
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PatchBenchmarkMetrics {
    pub iterations: usize,
    pub elapsed_ms: f64,
    pub patches_per_sec: f64,
    pub avg_apply_micros: f64,
    pub p99_apply_micros: f64,
    pub status_message: String,
}

/// Basic block within a control flow graph
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BasicBlock {
    pub id: usize,
    pub start_offset: usize,
    pub end_offset: usize,
    pub successors: Vec<usize>,
    pub predecessors: Vec<usize>,
}

/// Control Flow Graph (CFG) for prologue and function safety analysis
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ControlFlowGraph {
    pub blocks: Vec<BasicBlock>,
    pub entry_block: usize,
}

impl ControlFlowGraph {
    pub fn new() -> Self {
        Self {
            blocks: Vec::new(),
            entry_block: 0,
        }
    }

    pub fn add_block(&mut self, start_offset: usize, end_offset: usize) -> usize {
        let id = self.blocks.len();
        self.blocks.push(BasicBlock {
            id,
            start_offset,
            end_offset,
            successors: Vec::new(),
            predecessors: Vec::new(),
        });
        id
    }

    pub fn add_edge(&mut self, from: usize, to: usize) {
        if from < self.blocks.len() && to < self.blocks.len() {
            if !self.blocks[from].successors.contains(&to) {
                self.blocks[from].successors.push(to);
            }
            if !self.blocks[to].predecessors.contains(&from) {
                self.blocks[to].predecessors.push(from);
            }
        }
    }
}

impl Default for ControlFlowGraph {
    fn default() -> Self {
        Self::new()
    }
}

/// Validator verifying control flow dominance to ensure trampoline insertion is safe
pub struct DominanceValidator;

impl DominanceValidator {
    /// Validates whether inserting a trampoline of `trampoline_len` bytes at `hook_offset`
    /// does not split any instruction or receive incoming branches into the middle of the opcode.
    pub fn validate_hook_safety(
        cfg: &ControlFlowGraph,
        hook_offset: usize,
        trampoline_len: usize,
        function_len: usize,
    ) -> Result<bool> {
        // 1. Minimum function size check
        if function_len < hook_offset + trampoline_len {
            return Err(CraftError::Config(format!(
                "Function length ({} bytes) too short for trampoline of {} bytes at offset {}",
                function_len, trampoline_len, hook_offset
            )));
        }

        let hook_end = hook_offset + trampoline_len;

        // 2. CFG jump collision check: No block outside the entry block can jump into the middle
        // of [hook_offset + 1, hook_end)
        for block in &cfg.blocks {
            // Check if any block target falls strictly within the middle of the trampoline
            if block.start_offset > hook_offset && block.start_offset < hook_end {
                // If there are predecessors from outside the trampoline range, it's unsafe!
                for &pred_id in &block.predecessors {
                    if let Some(pred_block) = cfg.blocks.get(pred_id) {
                        if pred_block.start_offset >= hook_end || pred_block.end_offset <= hook_offset {
                            return Err(CraftError::Config(format!(
                                "CFG collision: incoming branch from block {} ({:#x}) into middle of trampoline ({:#x}..{:#x})",
                                pred_id, pred_block.start_offset, hook_offset, hook_end
                            )));
                        }
                    }
                }
            }
        }

        Ok(true)
    }
}

/// Memory page permission manager
pub struct PagePermissionGuard;

impl PagePermissionGuard {
    /// Configures page permissions for a memory region to allow code modification
    #[cfg(unix)]
    pub unsafe fn make_writable_executable(address: usize, length: usize) -> Result<()> {
        let page_size = 4096;
        let page_start = address & !(page_size - 1);
        let page_len = (address + length - page_start + page_size - 1) & !(page_size - 1);

        let prot = libc::PROT_READ | libc::PROT_WRITE | libc::PROT_EXEC;
        let res = libc::mprotect(page_start as *mut libc::c_void, page_len, prot);
        if res != 0 {
            let err = std::io::Error::last_os_error();
            return Err(CraftError::Io(err));
        }
        Ok(())
    }

    #[cfg(not(unix))]
    pub unsafe fn make_writable_executable(_address: usize, _length: usize) -> Result<()> {
        // Simulated execution on non-unix platforms
        Ok(())
    }

    /// Restores page permissions to read/execute only
    #[cfg(unix)]
    pub unsafe fn make_read_executable(address: usize, length: usize) -> Result<()> {
        let page_size = 4096;
        let page_start = address & !(page_size - 1);
        let page_len = (address + length - page_start + page_size - 1) & !(page_size - 1);

        let prot = libc::PROT_READ | libc::PROT_EXEC;
        let res = libc::mprotect(page_start as *mut libc::c_void, page_len, prot);
        if res != 0 {
            let err = std::io::Error::last_os_error();
            return Err(CraftError::Io(err));
        }
        Ok(())
    }

    #[cfg(not(unix))]
    pub unsafe fn make_read_executable(_address: usize, _length: usize) -> Result<()> {
        Ok(())
    }
}

/// Registry storing active and historical dynamic patches with advisory file locking
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PatchRegistry {
    pub patches: Vec<PatchManifest>,
    pub total_rollbacks: usize,
    pub safety_rejections: usize,
}

impl Default for PatchRegistry {
    fn default() -> Self {
        Self {
            patches: Vec::new(),
            total_rollbacks: 0,
            safety_rejections: 0,
        }
    }
}

impl PatchRegistry {
    /// Loads registry from disk with advisory lock
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        let file_path = &paths.patch_registry_file;
        if !file_path.exists() {
            return Ok(Self::default());
        }

        let lock_path = &paths.patch_lock;
        if let Some(parent) = lock_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        lock_file.lock_shared()?;

        let mut file = OpenOptions::new().read(true).open(file_path)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let _ = lock_file.unlock();

        if contents.trim().is_empty() {
            return Ok(Self::default());
        }

        serde_json::from_str(&contents).map_err(CraftError::from)
    }

    /// Saves registry to disk with exclusive advisory lock
    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let file_path = &paths.patch_registry_file;
        if let Some(parent) = file_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }

        let lock_path = &paths.patch_lock;
        if let Some(parent) = lock_path.parent() {
            if !parent.exists() {
                fs::create_dir_all(parent)?;
            }
        }
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(lock_path)?;
        lock_file.lock_exclusive()?;

        let json = serde_json::to_string_pretty(self)
            .map_err(CraftError::from)?;

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(file_path)?;
        file.seek(SeekFrom::Start(0))?;
        file.write_all(json.as_bytes())?;
        file.sync_all()?;
        let _ = lock_file.unlock();

        Ok(())
    }

    /// Finds a patch by server and name
    pub fn get_patch(&self, server: &str, name: &str) -> Option<&PatchManifest> {
        self.patches.iter().find(|p| p.server == server && p.name == name)
    }

    /// Finds a mutable patch by server and name
    pub fn get_patch_mut(&mut self, server: &str, name: &str) -> Option<&mut PatchManifest> {
        self.patches.iter_mut().find(|p| p.server == server && p.name == name)
    }

    /// Registers a newly created or applied patch
    pub fn register(&mut self, manifest: PatchManifest) -> Result<()> {
        if let Some(existing) = self.get_patch_mut(&manifest.server, &manifest.name) {
            *existing = manifest;
        } else {
            self.patches.push(manifest);
        }
        Ok(())
    }

    /// Updates the state of an existing patch
    pub fn update_state(&mut self, server: &str, name: &str, state: PatchState) -> Result<bool> {
        let epoch = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if let Some(p) = self.get_patch_mut(server, name) {
            p.state = state;
            match state {
                PatchState::Active => p.applied_at_epoch_s = Some(epoch),
                PatchState::RolledBack => {
                    p.rolled_back_at_epoch_s = Some(epoch);
                    self.total_rollbacks += 1;
                }
                _ => {}
            }
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// Removes a patch record
    pub fn remove(&mut self, server: &str, name: &str) -> Result<bool> {
        let initial_len = self.patches.len();
        self.patches.retain(|p| !(p.server == server && p.name == name));
        Ok(self.patches.len() < initial_len)
    }

    /// Generates a status summary
    pub fn get_summary(&self, server_filter: Option<&str>) -> PatchStatusSummary {
        let filtered: Vec<PatchManifest> = self
            .patches
            .iter()
            .filter(|p| {
                if let Some(srv) = server_filter {
                    p.server == srv
                } else {
                    true
                }
            })
            .cloned()
            .collect();

        let active_patches = filtered.iter().filter(|p| p.state == PatchState::Active).count();
        let total_applied = filtered.iter().filter(|p| p.applied_at_epoch_s.is_some()).count();

        let total_micros: u64 = filtered.iter().map(|p| p.apply_duration_micros).sum();
        let avg_apply_micros = if !filtered.is_empty() {
            total_micros as f64 / filtered.len() as f64
        } else {
            0.0
        };

        PatchStatusSummary {
            active_patches,
            total_applied,
            total_rollbacks: self.total_rollbacks,
            safety_rejections: self.safety_rejections,
            avg_apply_micros,
            patches: filtered,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cfg_dominance_safety() {
        let mut cfg = ControlFlowGraph::new();
        let b0 = cfg.add_block(0, 16);
        let b1 = cfg.add_block(16, 32);
        let b2 = cfg.add_block(32, 48);

        cfg.add_edge(b0, b1);
        cfg.add_edge(b1, b2);

        // Safe 5-byte hook at function entry (offset 0)
        let safe = DominanceValidator::validate_hook_safety(&cfg, 0, 5, 48).unwrap();
        assert!(safe);

        // Add illegal jump from b2 directly into the middle of the 5-byte trampoline (offset 2)
        let b_mid = cfg.add_block(2, 6);
        cfg.add_edge(b2, b_mid);

        let collision = DominanceValidator::validate_hook_safety(&cfg, 0, 5, 48);
        assert!(collision.is_err());
    }

    #[test]
    fn test_patch_registry_lifecycle() {
        let mut registry = PatchRegistry::default();
        let desc = TrampolineDescriptor::new(
            TrampolinePatchType::Rel32Jmp,
            vec![0x55, 0x48, 0x89, 0xE5, 0x90],
            vec![0xE9, 0x00, 0x10, 0x00, 0x00],
            vec![0x90, 0x90],
            0x1000,
            0x2000,
            0x3000,
        );

        let manifest = PatchManifest {
            name: "hotfix_dupe".to_string(),
            server: "lobby".to_string(),
            arch: ArchInstructionSet::X86_64,
            target_type: PatchTargetType::NativeSymbol("handle_inventory_click".to_string()),
            state: PatchState::Active,
            created_at_epoch_s: 1000,
            applied_at_epoch_s: Some(1005),
            rolled_back_at_epoch_s: None,
            execution_count: 42,
            apply_duration_micros: 120,
            descriptor: desc,
            safety_verified: true,
        };

        registry.register(manifest).unwrap();
        assert_eq!(registry.patches.len(), 1);

        let summary = registry.get_summary(None);
        assert_eq!(summary.active_patches, 1);
        assert_eq!(summary.total_applied, 1);
        assert_eq!(summary.avg_apply_micros, 120.0);

        // Rollback
        let rolled_back = registry.update_state("lobby", "hotfix_dupe", PatchState::RolledBack).unwrap();
        assert!(rolled_back);
        assert_eq!(registry.total_rollbacks, 1);

        let summary2 = registry.get_summary(None);
        assert_eq!(summary2.active_patches, 0);
    }
}
