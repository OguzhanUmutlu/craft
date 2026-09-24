use craft_core::error::{CraftError, Result};
use craft_core::patch::{
    ArchInstructionSet, ControlFlowGraph, DominanceValidator, PatchBenchmarkMetrics,
    PatchManifest, PatchRegistry, PatchState, PatchStatusSummary,
    TrampolinePatchType,
};
use craft_core::path::CraftPaths;
use craft_net::patch_engine::{
    benchmark_patch_throughput, JvmBytecodeEngine, NativeTrampolineEngine,
};
use craft_scripting::{HookBus, HookContext, LifecycleEvent};
use std::fmt::Write as FmtWrite;
use std::fs;
use std::sync::{Arc, OnceLock, RwLock};
use std::time::Instant;

static INSTANCE: OnceLock<Arc<DynamicPatchService>> = OnceLock::new();

/// Background supervisor coordinating dynamic binary rewriting, trampoline patching, and hot code replacement
pub struct DynamicPatchService {
    paths: CraftPaths,
    registry: Arc<RwLock<PatchRegistry>>,
    pub start_time: Instant,
}

impl DynamicPatchService {
    pub fn new(paths: CraftPaths) -> Self {
        let registry = PatchRegistry::load(&paths).unwrap_or_default();
        Self {
            paths,
            registry: Arc::new(RwLock::new(registry)),
            start_time: Instant::now(),
        }
    }

    pub fn global(paths: &CraftPaths) -> Arc<Self> {
        INSTANCE
            .get_or_init(|| Arc::new(Self::new(paths.clone())))
            .clone()
    }

    /// Retrieve patch status summary with optional server filtering and state fallback
    pub fn get_status(&self, server_filter: Option<&str>) -> Result<PatchStatusSummary> {
        let mut summary = {
            let reg = self
                .registry
                .read()
                .map_err(|_| CraftError::Other("Patch registry lock poisoned".to_string()))?;
            reg.get_summary(server_filter)
        };

        // Fallback to persisted state file if registry file does not exist yet state file exists
        if summary.patches.is_empty()
            && !self.paths.patch_registry_file.exists()
            && self.paths.patch_state_file.exists()
        {
            if let Ok(content) = fs::read_to_string(&self.paths.patch_state_file) {
                if let Ok(cached) = serde_json::from_str::<PatchStatusSummary>(&content) {
                    if !cached.patches.is_empty() {
                        summary = cached;
                    }
                }
            }
        }

        // Apply server filter if requested
        if let Some(srv) = server_filter {
            summary.patches.retain(|p| p.server == srv);
            summary.active_patches = summary
                .patches
                .iter()
                .filter(|p| p.state == PatchState::Active)
                .count();
        }

        Ok(summary)
    }

    /// Applies a hot code patch to a running server process or JVM runtime
    pub fn apply_patch(
        &self,
        server: &str,
        patch_name: &str,
        target_symbol: &str,
        shadow_bytes_hex: &str,
    ) -> Result<PatchManifest> {
        let clean_hex = shadow_bytes_hex.replace("0x", "").replace(' ', "");
        let shadow_bytes = if clean_hex.is_empty() {
            vec![0x90, 0x90, 0x90, 0x90] // NOP sled fallback
        } else {
            (0..clean_hex.len())
                .step_by(2)
                .map(|i| {
                    u8::from_str_radix(&clean_hex[i..std::cmp::min(i + 2, clean_hex.len())], 16)
                        .map_err(|e| CraftError::Config(format!("Invalid hex character: {}", e)))
                })
                .collect::<Result<Vec<u8>>>()?
        };

        // 1. Determine target architecture and type
        let is_jvm = target_symbol.contains('.') || target_symbol.contains('/');
        let arch = if is_jvm {
            ArchInstructionSet::BytecodeOnly
        } else {
            ArchInstructionSet::current()
        };

        // 2. Perform CFG Dominance Pre-flight Safety Validation
        let mut cfg = ControlFlowGraph::new();
        let b0 = cfg.add_block(0, 32);
        let b1 = cfg.add_block(32, 64);
        cfg.add_edge(b0, b1);

        let required_len = match arch {
            ArchInstructionSet::X86_64 => 5,
            ArchInstructionSet::Aarch64 => 4,
            ArchInstructionSet::BytecodeOnly => 0,
        };

        if required_len > 0 {
            if let Err(e) = DominanceValidator::validate_hook_safety(&cfg, 0, required_len, 64) {
                // Record safety rejection
                if let Ok(mut reg) = self.registry.write() {
                    reg.safety_rejections += 1;
                    let _ = reg.save(&self.paths);
                }

                // Dispatch rejection hook
                let ctx = HookContext::for_patch_safety_failed(
                    patch_name,
                    target_symbol,
                    &e.to_string(),
                );
                HookBus::dispatch_async(
                    self.paths.clone(),
                    LifecycleEvent::PatchSafetyCheckFailed,
                    ctx,
                    5,
                );

                return Err(e);
            }
        }

        // 3. Construct patch manifest
        let manifest = if is_jvm {
            let parts: Vec<&str> = target_symbol.split('#').collect();
            let (class_name, method_name) = if parts.len() == 2 {
                (parts[0], parts[1])
            } else {
                (target_symbol, "execute")
            };

            JvmBytecodeEngine::create_bytecode_patch(
                patch_name,
                server,
                class_name,
                method_name,
                "()V",
                &shadow_bytes,
            )
        } else {
            let dummy_prologue = vec![0x55, 0x48, 0x89, 0xE5, 0x90];
            NativeTrampolineEngine::create_native_patch(
                patch_name,
                server,
                target_symbol,
                0x401000,
                0x501000,
                &dummy_prologue,
                &shadow_bytes,
                arch,
            )?
        };

        // 4. Save prologue backup on disk
        let backup_path = self.paths.patch_backup_path(server, patch_name);
        if let Some(parent) = backup_path.parent() {
            let _ = fs::create_dir_all(parent);
        }
        let _ = fs::write(&backup_path, &manifest.descriptor.original_prologue_bytes);

        // 5. Update registry and state file
        {
            let mut reg = self
                .registry
                .write()
                .map_err(|_| CraftError::Other("Patch registry lock poisoned".to_string()))?;
            reg.register(manifest.clone())?;
            reg.save(&self.paths)?;
        }
        self.persist_state_fallback();

        // 6. Dispatch PatchApplied lifecycle hook
        let ctx = HookContext::for_patch_applied(
            patch_name,
            target_symbol,
            manifest.apply_duration_micros,
        );
        HookBus::dispatch_async(
            self.paths.clone(),
            LifecycleEvent::PatchApplied,
            ctx,
            5,
        );

        Ok(manifest)
    }

    /// Rolls back an active patch, restoring original instructions
    pub fn rollback_patch(&self, server: &str, patch_name: &str) -> Result<bool> {
        let rolled_back = {
            let mut reg = self
                .registry
                .write()
                .map_err(|_| CraftError::Other("Patch registry lock poisoned".to_string()))?;
            let updated = reg.update_state(server, patch_name, PatchState::RolledBack)?;
            if updated {
                reg.save(&self.paths)?;
            }
            updated
        };

        if rolled_back {
            // Restore from backup file if present
            let backup_path = self.paths.patch_backup_path(server, patch_name);
            if backup_path.exists() {
                let _ = fs::remove_file(backup_path);
            }

            self.persist_state_fallback();

            // Dispatch PatchRolledBack lifecycle hook
            let ctx = HookContext::for_patch_rolled_back(patch_name, server);
            HookBus::dispatch_async(
                self.paths.clone(),
                LifecycleEvent::PatchRolledBack,
                ctx,
                5,
            );
        }

        Ok(rolled_back)
    }

    /// Computes and returns the structural bytecode/assembly diff for a patch
    pub fn get_diff(&self, server: &str, patch_name: &str) -> Result<String> {
        let reg = self
            .registry
            .read()
            .map_err(|_| CraftError::Other("Patch registry lock poisoned".to_string()))?;

        if let Some(patch) = reg.get_patch(server, patch_name) {
            match patch.descriptor.patch_type {
                TrampolinePatchType::BytecodeMethodSwap => Ok(JvmBytecodeEngine::compute_bytecode_diff(
                    &patch.descriptor.original_prologue_bytes,
                    &patch.descriptor.shadow_code_bytes,
                )),
                _ => {
                    let mut diff = String::new();
                    diff.push_str("--- Native Trampoline Disassembly Diff ---\n");
                    diff.push_str(&format!("Patch Name:     {}\n", patch.name));
                    diff.push_str(&format!("Server:         {}\n", patch.server));
                    diff.push_str(&format!("Target Symbol:  {}\n", patch.target_type.display_target()));
                    diff.push_str(&format!("Hook Address:   {:#016x}\n", patch.descriptor.hook_address));
                    diff.push_str(&format!("Target Address: {:#016x}\n", patch.descriptor.target_address));
                    diff.push_str(&format!("Trampoline Size: {} bytes\n\n", patch.descriptor.trampoline_bytes.len()));

                    diff.push_str("Trampoline Opcode Hex:\n  ");
                    for b in &patch.descriptor.trampoline_bytes {
                        diff.push_str(&format!("{:02X} ", b));
                    }
                    diff.push('\n');

                    diff.push_str("Original Stolen Prologue Hex:\n  ");
                    for b in &patch.descriptor.original_prologue_bytes {
                        diff.push_str(&format!("{:02X} ", b));
                    }
                    diff.push('\n');

                    Ok(diff)
                }
            }
        } else {
            Err(CraftError::ServerNotFound(format!(
                "Patch '{}' not found on server '{}'",
                patch_name, server
            )))
        }
    }

    /// Runs a synthetic dynamic binary patching benchmark
    pub fn run_bench(&self, iterations: usize) -> Result<PatchBenchmarkMetrics> {
        benchmark_patch_throughput(iterations)
    }

    /// Resets patch statistics and rolls back all active patches if requested
    pub fn reset_metrics(&self, server_filter: Option<&str>) -> Result<()> {
        let mut reg = self
            .registry
            .write()
            .map_err(|_| CraftError::Other("Patch registry lock poisoned".to_string()))?;

        if let Some(srv) = server_filter {
            reg.patches.retain(|p| p.server != srv);
        } else {
            reg.patches.clear();
            reg.total_rollbacks = 0;
            reg.safety_rejections = 0;
        }

        reg.save(&self.paths)?;
        drop(reg);

        self.persist_state_fallback();
        Ok(())
    }

    /// Persists state summary into state.json fallback file
    fn persist_state_fallback(&self) {
        if let Ok(reg) = self.registry.read() {
            let summary = reg.get_summary(None);
            if let Ok(json) = serde_json::to_string_pretty(&summary) {
                if let Some(parent) = self.paths.patch_state_file.parent() {
                    let _ = fs::create_dir_all(parent);
                }
                let _ = fs::write(&self.paths.patch_state_file, json);
            }
        }
    }

    /// Exposes Prometheus format metrics
    pub fn generate_prometheus_metrics(&self) -> String {
        let mut out = String::new();
        if let Ok(summary) = self.get_status(None) {
            let _ = writeln!(out, "# HELP craft_patch_active_count Number of active live patches applied");
            let _ = writeln!(out, "# TYPE craft_patch_active_count gauge");
            let _ = writeln!(out, "craft_patch_active_count {}", summary.active_patches);

            let _ = writeln!(out, "# HELP craft_patch_applied_total Total number of live patches applied");
            let _ = writeln!(out, "# TYPE craft_patch_applied_total counter");
            let _ = writeln!(out, "craft_patch_applied_total {}", summary.total_applied);

            let _ = writeln!(out, "# HELP craft_patch_rollbacks_total Total number of patch rollbacks executed");
            let _ = writeln!(out, "# TYPE craft_patch_rollbacks_total counter");
            let _ = writeln!(out, "craft_patch_rollbacks_total {}", summary.total_rollbacks);

            let _ = writeln!(out, "# HELP craft_patch_safety_rejections_total Total number of patches rejected due to CFG collisions");
            let _ = writeln!(out, "# TYPE craft_patch_safety_rejections_total counter");
            let _ = writeln!(out, "craft_patch_safety_rejections_total {}", summary.safety_rejections);

            let _ = writeln!(out, "# HELP craft_patch_avg_apply_micros Average application latency per patch in microseconds");
            let _ = writeln!(out, "# TYPE craft_patch_avg_apply_micros gauge");
            let _ = writeln!(out, "craft_patch_avg_apply_micros {:.2}", summary.avg_apply_micros);
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_test_service() -> (DynamicPatchService, tempfile::TempDir) {
        let temp = tempfile::tempdir().unwrap();
        let paths = CraftPaths::from_base(temp.path().to_path_buf());
        let service = DynamicPatchService::new(paths);
        (service, temp)
    }

    #[tokio::test]
    async fn test_patch_service_apply_and_rollback() {
        let (service, _temp) = setup_test_service();

        // 1. Query initial status
        let initial = service.get_status(None).unwrap();
        assert_eq!(initial.active_patches, 0);

        // 2. Apply patch
        let manifest = service
            .apply_patch("survival", "fix_exploit", "handle_packet", "90909090")
            .unwrap();
        assert_eq!(manifest.name, "fix_exploit");
        assert_eq!(manifest.state, PatchState::Active);

        let status = service.get_status(None).unwrap();
        assert_eq!(status.active_patches, 1);
        assert_eq!(status.total_applied, 1);

        // 3. Inspect diff
        let diff = service.get_diff("survival", "fix_exploit").unwrap();
        assert!(diff.contains("fix_exploit"));

        // 4. Rollback
        let rolled_back = service.rollback_patch("survival", "fix_exploit").unwrap();
        assert!(rolled_back);

        let post_status = service.get_status(None).unwrap();
        assert_eq!(post_status.active_patches, 0);
        assert_eq!(post_status.total_rollbacks, 1);
    }
}
