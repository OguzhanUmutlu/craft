// crates/daemon/src/bft_service.rs
//
// Autonomous Geo-Distributed Byzantine Fault-Tolerant Consensus, Zero-Knowledge State Attestation & BFT Cluster Quorum Service.
// Strictly zero emojis.

use craft_core::bft::{
    BftBenchmarkMetrics, BftRegistry, BftStatusSummary, BftTransaction, BftTxType,
    BftValidator, ZkStateProof,
};
use craft_core::error::{CraftError, Result};
use craft_core::path::CraftPaths;
use craft_net::bft::{benchmark_bft_consensus, HotStuffBftEngine};
use std::str::FromStr;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

static INSTANCE: OnceLock<Arc<BftConsensusService>> = OnceLock::new();

pub struct BftConsensusService {
    paths: CraftPaths,
    engine: Mutex<HotStuffBftEngine>,
    current_view: Arc<AtomicU64>,
    committed_blocks_total: Arc<AtomicU64>,
    slashed_validators_total: Arc<AtomicU64>,
    zk_proofs_verified_total: Arc<AtomicU64>,
}

impl BftConsensusService {
    pub fn new(paths: CraftPaths) -> Self {
        let reg = BftRegistry::load(&paths).unwrap_or_default();
        let engine = HotStuffBftEngine::new("node-dal-01", reg.clone());

        let current_view = Arc::new(AtomicU64::new(reg.current_view));
        let committed_blocks_total = Arc::new(AtomicU64::new(reg.committed_blocks_count));
        let slashed_count = reg.validators.iter().filter(|v| v.slashed).count() as u64;
        let slashed_validators_total = Arc::new(AtomicU64::new(slashed_count));
        let zk_proofs_verified_total = Arc::new(AtomicU64::new(0));

        Self {
            paths,
            engine: Mutex::new(engine),
            current_view,
            committed_blocks_total,
            slashed_validators_total,
            zk_proofs_verified_total,
        }
    }

    pub fn global(paths: &CraftPaths) -> Arc<Self> {
        INSTANCE
            .get_or_init(|| Arc::new(Self::new(paths.clone())))
            .clone()
    }

    pub fn get_status(&self, _server: Option<&str>) -> Result<BftStatusSummary> {
        let reg = BftRegistry::load(&self.paths).unwrap_or_default();
        let engine = self.engine.lock().map_err(|e| CraftError::Other(e.to_string()))?;

        let mut summary = reg.get_status_summary();
        summary.current_view = engine.current_view.max(summary.current_view);
        summary.block_height = engine.registry.block_height.max(summary.block_height);
        summary.committed_blocks = self.committed_blocks_total.load(Ordering::Relaxed).max(summary.committed_blocks);
        summary.slashed_validators = self.slashed_validators_total.load(Ordering::Relaxed) as usize;

        Ok(summary)
    }

    pub fn submit_transaction(
        &self,
        tx_type_str: &str,
        payload: &str,
        sender_opt: Option<&str>,
    ) -> Result<(String, u64)> {
        let tx_type = BftTxType::from_str(tx_type_str)?;
        let sender = sender_opt.unwrap_or("client-node-01");
        let mut engine = self.engine.lock().map_err(|e| CraftError::Other(e.to_string()))?;

        let nonce = engine.registry.committed_blocks_count + 1;
        let tx = BftTransaction::new(sender, tx_type, payload, nonce);
        let tx_id = tx.tx_id.clone();

        let block = engine.propose_block(vec![tx])?;
        self.committed_blocks_total.fetch_add(1, Ordering::Relaxed);

        // Update persistent registry
        let mut reg = BftRegistry::load(&self.paths).unwrap_or_default();
        reg.block_height = block.height;
        reg.last_committed_block_hash = block.block_hash;
        reg.committed_blocks_count += 1;
        let _ = reg.save(&self.paths);

        Ok((tx_id, block.view))
    }

    pub fn list_validators(&self, _server: Option<&str>) -> Result<Vec<BftValidator>> {
        let reg = BftRegistry::load(&self.paths).unwrap_or_default();
        Ok(reg.validators)
    }

    pub fn add_validator(
        &self,
        node_id: &str,
        address: &str,
        public_key: &str,
        stake: u64,
    ) -> Result<()> {
        let mut reg = BftRegistry::load(&self.paths).unwrap_or_default();
        reg.add_validator(node_id, address, public_key, stake)?;
        reg.save(&self.paths)?;

        let mut engine = self.engine.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        engine.registry = reg;

        Ok(())
    }

    pub fn remove_validator(&self, node_id: &str) -> Result<()> {
        let mut reg = BftRegistry::load(&self.paths).unwrap_or_default();
        reg.remove_validator(node_id)?;
        reg.save(&self.paths)?;

        let mut engine = self.engine.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        engine.registry = reg;

        Ok(())
    }

    pub fn trigger_view_change(&self, reason: &str) -> Result<(u64, u64, String)> {
        let mut engine = self.engine.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        let payload = engine.trigger_view_change(reason);
        self.current_view.store(payload.to_view, Ordering::Relaxed);

        let mut reg = BftRegistry::load(&self.paths).unwrap_or_default();
        reg.current_view = payload.to_view;
        let _ = reg.save(&self.paths);

        let new_proposer = reg
            .get_proposer_for_view(payload.to_view)
            .map(|v| v.node_id.clone())
            .unwrap_or_else(|| "none".to_string());

        Ok((payload.from_view, payload.to_view, new_proposer))
    }

    pub fn verify_zk_proof(&self, proof_json: &str) -> Result<bool> {
        let proof: ZkStateProof = serde_json::from_str(proof_json)
            .map_err(|e| CraftError::Config(format!("Failed to parse ZK state proof JSON: {}", e)))?;

        let mut engine = self.engine.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        let valid = engine.verify_zk_state_proof(&proof)?;
        if valid {
            self.zk_proofs_verified_total.fetch_add(1, Ordering::Relaxed);
        }

        Ok(valid)
    }

    pub fn run_bench(&self, iterations: u32, validators: u32) -> Result<BftBenchmarkMetrics> {
        let metrics = benchmark_bft_consensus(iterations, validators)?;
        self.committed_blocks_total.fetch_add(iterations as u64, Ordering::Relaxed);
        if metrics.equivocations_detected > 0 {
            self.slashed_validators_total.fetch_add(metrics.equivocations_detected as u64, Ordering::Relaxed);
        }
        Ok(metrics)
    }

    pub fn reset_metrics(&self) -> Result<()> {
        let mut engine = self.engine.lock().map_err(|e| CraftError::Other(e.to_string()))?;
        engine.reset_metrics();
        self.committed_blocks_total.store(0, Ordering::Relaxed);
        self.slashed_validators_total.store(0, Ordering::Relaxed);
        self.zk_proofs_verified_total.store(0, Ordering::Relaxed);

        let mut reg = BftRegistry::load(&self.paths).unwrap_or_default();
        reg.committed_blocks_count = 0;
        reg.slashing_records.clear();
        for v in &mut reg.validators {
            v.slashed = false;
            v.active = true;
            v.total_votes = 0;
        }
        let _ = reg.save(&self.paths);

        Ok(())
    }

    pub fn generate_prometheus_metrics(&self) -> String {
        let summary = self.get_status(None).unwrap_or_else(|_| BftRegistry::default().get_status_summary());
        let mut out = String::new();
        out.push_str("# HELP craft_bft_current_view Current consensus view sequence number\n");
        out.push_str("# TYPE craft_bft_current_view gauge\n");
        out.push_str(&format!("craft_bft_current_view {}\n", summary.current_view));

        out.push_str("# HELP craft_bft_committed_blocks_total Cumulative count of committed BFT blocks\n");
        out.push_str("# TYPE craft_bft_committed_blocks_total counter\n");
        out.push_str(&format!("craft_bft_committed_blocks_total {}\n", summary.committed_blocks));

        out.push_str("# HELP craft_bft_validators_total Active voting validator nodes in quorum\n");
        out.push_str("# TYPE craft_bft_validators_total gauge\n");
        out.push_str(&format!("craft_bft_validators_total {}\n", summary.active_validators));

        out.push_str("# HELP craft_bft_slashed_validators_total Total Byzantine nodes slashed for equivocation\n");
        out.push_str("# TYPE craft_bft_slashed_validators_total counter\n");
        out.push_str(&format!("craft_bft_slashed_validators_total {}\n", summary.slashed_validators));

        out.push_str("# HELP craft_bft_commit_latency_micros Average block commit finality latency in microseconds\n");
        out.push_str("# TYPE craft_bft_commit_latency_micros gauge\n");
        out.push_str(&format!("craft_bft_commit_latency_micros {}\n", (summary.avg_commit_latency_ms * 1000.0) as u64));

        out.push_str("# HELP craft_bft_zk_proofs_verified_total Cumulative zero-knowledge state proofs verified\n");
        out.push_str("# TYPE craft_bft_zk_proofs_verified_total counter\n");
        out.push_str(&format!("craft_bft_zk_proofs_verified_total {}\n", self.zk_proofs_verified_total.load(Ordering::Relaxed)));

        out
    }
}
