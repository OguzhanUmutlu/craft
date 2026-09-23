use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::fs::{self, OpenOptions};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

static ROLLOUT_COUNTER: AtomicU64 = AtomicU64::new(1);

fn generate_rollout_id(cluster_name: &str) -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let count = ROLLOUT_COUNTER.fetch_add(1, Ordering::SeqCst);
    let mut hasher = Sha256::new();
    hasher.update(cluster_name.as_bytes());
    hasher.update(&now.to_be_bytes());
    hasher.update(&count.to_be_bytes());
    let digest = format!("{:x}", hasher.finalize());
    format!("rollout-{}-{}", cluster_name, &digest[..8])
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", rename_all = "snake_case")]
pub enum RolloutStrategy {
    Canary {
        percentage: u8,
        bake_seconds: u64,
    },
    BlueGreen,
    Rolling {
        max_parallel: usize,
    },
}

impl Default for RolloutStrategy {
    fn default() -> Self {
        RolloutStrategy::Canary {
            percentage: 25,
            bake_seconds: 60,
        }
    }
}

impl std::fmt::Display for RolloutStrategy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RolloutStrategy::Canary { percentage, bake_seconds } => {
                write!(f, "Canary ({}%, {}s bake)", percentage, bake_seconds)
            }
            RolloutStrategy::BlueGreen => write!(f, "BlueGreen"),
            RolloutStrategy::Rolling { max_parallel } => {
                write!(f, "Rolling (parallel: {})", max_parallel)
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "stage", rename_all = "snake_case")]
pub enum RolloutStage {
    Pending,
    CanaryBaking {
        node_id: String,
        started_at: u64,
        elapsed_seconds: u64,
    },
    CanaryPromoting {
        canary_node: String,
    },
    RollingOut {
        completed_nodes: Vec<String>,
        remaining_nodes: Vec<String>,
        in_flight: Vec<String>,
    },
    BlueGreenSwitching {
        active_color: String,
        staging_color: String,
    },
    Succeeded {
        completed_at: u64,
    },
    RolledBack {
        reason: String,
        rolled_back_at: u64,
    },
    Failed {
        error: String,
        failed_at: u64,
    },
}

impl RolloutStage {
    pub fn is_active(&self) -> bool {
        matches!(
            self,
            RolloutStage::Pending
                | RolloutStage::CanaryBaking { .. }
                | RolloutStage::CanaryPromoting { .. }
                | RolloutStage::RollingOut { .. }
                | RolloutStage::BlueGreenSwitching { .. }
        )
    }

    pub fn name(&self) -> &'static str {
        match self {
            RolloutStage::Pending => "Pending",
            RolloutStage::CanaryBaking { .. } => "CanaryBaking",
            RolloutStage::CanaryPromoting { .. } => "CanaryPromoting",
            RolloutStage::RollingOut { .. } => "RollingOut",
            RolloutStage::BlueGreenSwitching { .. } => "BlueGreenSwitching",
            RolloutStage::Succeeded { .. } => "Succeeded",
            RolloutStage::RolledBack { .. } => "RolledBack",
            RolloutStage::Failed { .. } => "Failed",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct CanaryHealthCriteria {
    pub min_tps: f64,
    pub max_mspt: f64,
    pub max_jitter_ms: f64,
    pub max_crash_count: u32,
    pub bake_seconds: u64,
}

impl Default for CanaryHealthCriteria {
    fn default() -> Self {
        Self {
            min_tps: 19.0,
            max_mspt: 45.0,
            max_jitter_ms: 15.0,
            max_crash_count: 0,
            bake_seconds: 60,
        }
    }
}

impl CanaryHealthCriteria {
    pub fn evaluate(
        &self,
        tps: f64,
        mspt: f64,
        jitter_ms: f64,
        crashes: u32,
    ) -> std::result::Result<(), String> {
        if crashes > self.max_crash_count {
            return Err(format!(
                "Node crashed {} times exceeding threshold {}",
                crashes, self.max_crash_count
            ));
        }
        if tps < self.min_tps && tps > 0.0 {
            return Err(format!(
                "Node TPS {:.1} is below minimum required {:.1}",
                tps, self.min_tps
            ));
        }
        if mspt > self.max_mspt {
            return Err(format!(
                "Node MSPT {:.1}ms exceeds maximum allowed {:.1}ms",
                mspt, self.max_mspt
            ));
        }
        if jitter_ms > self.max_jitter_ms {
            return Err(format!(
                "Node tick jitter {:.1}ms exceeds maximum allowed {:.1}ms",
                jitter_ms, self.max_jitter_ms
            ));
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RolloutPlan {
    pub id: String,
    pub cluster_name: String,
    pub target_version: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub target_software: Option<String>,
    pub strategy: RolloutStrategy,
    pub criteria: CanaryHealthCriteria,
    pub nodes: Vec<String>,
    #[serde(default)]
    pub pre_rollout_snapshots: HashMap<String, String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub active_proxy_group: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub blue_green_active_color: Option<String>,
    pub created_at: u64,
}

impl RolloutPlan {
    pub fn new(
        cluster_name: impl Into<String>,
        target_version: impl Into<String>,
        target_software: Option<String>,
        strategy: RolloutStrategy,
        criteria: CanaryHealthCriteria,
        nodes: Vec<String>,
    ) -> Self {
        let cluster = cluster_name.into();
        let id = generate_rollout_id(&cluster);
        let created_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            id,
            cluster_name: cluster,
            target_version: target_version.into(),
            target_software,
            strategy,
            criteria,
            nodes,
            pre_rollout_snapshots: HashMap::new(),
            active_proxy_group: None,
            blue_green_active_color: None,
            created_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct RolloutRecord {
    pub plan: RolloutPlan,
    pub stage: RolloutStage,
    pub started_at: u64,
    pub updated_at: u64,
    #[serde(default)]
    pub logs: Vec<String>,
}

impl RolloutRecord {
    pub fn new(plan: RolloutPlan) -> Self {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        Self {
            plan,
            stage: RolloutStage::Pending,
            started_at: now,
            updated_at: now,
            logs: Vec::new(),
        }
    }

    pub fn is_active(&self) -> bool {
        self.stage.is_active()
    }

    pub fn can_abort(&self) -> bool {
        self.stage.is_active()
    }

    pub fn append_log(&mut self, msg: impl Into<String>) {
        let formatted = format!(
            "[{}] {}",
            chrono::Utc::now().format("%Y-%m-%d %H:%M:%S"),
            msg.into()
        );
        self.logs.push(formatted);
        self.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct NodeHealth {
    pub node_id: String,
    pub status: String,
    pub current_tps: f64,
    pub current_mspt: f64,
    pub crash_count: u32,
    pub active_version: String,
    pub is_canary: bool,
    pub is_drained: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct FleetHealthStatus {
    pub cluster_name: String,
    pub node_statuses: HashMap<String, NodeHealth>,
    pub overall_healthy: bool,
    pub evaluated_at: u64,
}

impl FleetHealthStatus {
    pub fn new(cluster_name: impl Into<String>) -> Self {
        Self {
            cluster_name: cluster_name.into(),
            node_statuses: HashMap::new(),
            overall_healthy: true,
            evaluated_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "action", rename_all = "snake_case")]
pub enum FleetHealingAction {
    RestartNode {
        node_id: String,
        reason: String,
    },
    RollbackNode {
        node_id: String,
        snapshot: String,
        reason: String,
    },
    DrainNode {
        node_id: String,
        reason: String,
    },
    PromoteCanary {
        node_id: String,
    },
    MarkDegraded {
        node_id: String,
        reason: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct RolloutRegistry {
    #[serde(default)]
    pub rollouts: HashMap<String, RolloutRecord>,
    #[serde(default)]
    pub active_rollouts: HashMap<String, String>,
    #[serde(default)]
    pub history: Vec<String>,
}

impl RolloutRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if !paths.rollouts_file.exists() {
            let default_reg = Self::default();
            default_reg.save(paths)?;
            return Ok(default_reg);
        }

        let content = fs::read_to_string(&paths.rollouts_file).map_err(|e| {
            CraftError::Config(format!(
                "Failed to read rollouts config file {:?}: {}",
                paths.rollouts_file, e
            ))
        })?;

        toml::from_str(&content).map_err(|e| {
            CraftError::Config(format!(
                "Failed to parse rollouts config file {:?}: {}",
                paths.rollouts_file, e
            ))
        })
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        if let Some(parent) = paths.rollouts_file.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self).map_err(|e| {
            CraftError::Config(format!("Failed to serialize rollouts config: {}", e))
        })?;
        let temp_file = paths.rollouts_file.with_extension("tmp");
        fs::write(&temp_file, content)?;
        fs::rename(temp_file, &paths.rollouts_file)?;
        Ok(())
    }

    pub fn modify<F, R>(paths: &CraftPaths, f: F) -> Result<R>
    where
        F: FnOnce(&mut Self) -> Result<R>,
    {
        if let Some(parent) = paths.rollouts_lock.parent() {
            fs::create_dir_all(parent)?;
        }
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&paths.rollouts_lock)?;

        lock_file.lock_exclusive()?;

        let res = (|| {
            let mut registry = Self::load(paths)?;
            let ret = f(&mut registry)?;
            registry.save(paths)?;
            Ok(ret)
        })();

        let _ = lock_file.unlock();
        res
    }

    pub fn get_rollout(&self, rollout_id: &str) -> Option<&RolloutRecord> {
        self.rollouts.get(rollout_id)
    }

    pub fn get_rollout_mut(&mut self, rollout_id: &str) -> Option<&mut RolloutRecord> {
        self.rollouts.get_mut(rollout_id)
    }

    pub fn get_active_rollout(&self, cluster: &str) -> Option<&RolloutRecord> {
        self.active_rollouts
            .get(cluster)
            .and_then(|id| self.rollouts.get(id))
    }

    pub fn start_rollout(&mut self, plan: RolloutPlan) -> Result<String> {
        if let Some(active_id) = self.active_rollouts.get(&plan.cluster_name) {
            return Err(CraftError::Other(format!(
                "Cluster '{}' already has an active rollout '{}'",
                plan.cluster_name, active_id
            )));
        }

        let rollout_id = plan.id.clone();
        let cluster_name = plan.cluster_name.clone();
        let mut record = RolloutRecord::new(plan);
        record.append_log(format!(
            "Initiating rollout '{}' for cluster '{}'",
            rollout_id, cluster_name
        ));

        self.rollouts.insert(rollout_id.clone(), record);
        self.active_rollouts.insert(cluster_name, rollout_id.clone());
        self.history.retain(|id| id != &rollout_id);
        self.history.push(rollout_id.clone());

        Ok(rollout_id)
    }

    pub fn update_stage(
        &mut self,
        rollout_id: &str,
        stage: RolloutStage,
        log_message: Option<&str>,
    ) -> Result<()> {
        let record = self.rollouts.get_mut(rollout_id).ok_or_else(|| {
            CraftError::Other(format!("Rollout '{}' not found", rollout_id))
        })?;

        if let Some(msg) = log_message {
            record.append_log(msg);
        }

        let is_terminal = !stage.is_active();
        record.stage = stage;
        record.updated_at = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        if is_terminal {
            self.active_rollouts.remove(&record.plan.cluster_name);
        }

        Ok(())
    }

    pub fn abort_rollout(&mut self, rollout_id: &str, reason: &str) -> Result<()> {
        let record = self.rollouts.get_mut(rollout_id).ok_or_else(|| {
            CraftError::Other(format!("Rollout '{}' not found", rollout_id))
        })?;

        if !record.can_abort() {
            return Err(CraftError::Other(format!(
                "Rollout '{}' is already in terminal state '{}' and cannot be aborted",
                rollout_id,
                record.stage.name()
            )));
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs();

        record.append_log(format!("Rollout aborted: {}", reason));
        record.stage = RolloutStage::RolledBack {
            reason: reason.to_string(),
            rolled_back_at: now,
        };
        record.updated_at = now;

        self.active_rollouts.remove(&record.plan.cluster_name);
        Ok(())
    }

    pub fn list_active(&self) -> Vec<&RolloutRecord> {
        self.active_rollouts
            .values()
            .filter_map(|id| self.rollouts.get(id))
            .collect()
    }

    pub fn list_history(&self) -> Vec<&RolloutRecord> {
        self.history
            .iter()
            .rev()
            .filter_map(|id| self.rollouts.get(id))
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_canary_health_criteria_evaluation() {
        let criteria = CanaryHealthCriteria::default();
        // Healthy check
        assert!(criteria.evaluate(20.0, 35.0, 5.0, 0).is_ok());

        // Crash threshold exceeded
        assert!(criteria.evaluate(20.0, 35.0, 5.0, 1).is_err());

        // Low TPS
        assert!(criteria.evaluate(15.0, 35.0, 5.0, 0).is_err());

        // High MSPT
        assert!(criteria.evaluate(20.0, 55.0, 5.0, 0).is_err());

        // High Jitter
        assert!(criteria.evaluate(20.0, 35.0, 25.0, 0).is_err());
    }

    #[test]
    fn test_rollout_registry_lifecycle() {
        let temp_dir = tempfile::tempdir().unwrap();
        let paths = CraftPaths::from_base(temp_dir.path().to_path_buf());

        let plan = RolloutPlan::new(
            "survival-cluster",
            "1.21.1",
            Some("paper".to_string()),
            RolloutStrategy::Canary {
                percentage: 25,
                bake_seconds: 30,
            },
            CanaryHealthCriteria::default(),
            vec!["node-1".to_string(), "node-2".to_string()],
        );

        let rollout_id = plan.id.clone();

        // Start rollout
        let started_id = RolloutRegistry::modify(&paths, |reg| reg.start_rollout(plan)).unwrap();
        assert_eq!(started_id, rollout_id);

        // Registry check
        let reg = RolloutRegistry::load(&paths).unwrap();
        let record = reg.get_active_rollout("survival-cluster").unwrap();
        assert!(record.is_active());
        assert_eq!(record.plan.target_version, "1.21.1");

        // Duplicate start should fail
        let duplicate_plan = RolloutPlan::new(
            "survival-cluster",
            "1.21.2",
            None,
            RolloutStrategy::BlueGreen,
            CanaryHealthCriteria::default(),
            vec!["node-1".to_string()],
        );
        assert!(RolloutRegistry::modify(&paths, |reg| reg.start_rollout(duplicate_plan)).is_err());

        // Transition stage to Baking
        RolloutRegistry::modify(&paths, |reg| {
            reg.update_stage(
                &rollout_id,
                RolloutStage::CanaryBaking {
                    node_id: "node-1".to_string(),
                    started_at: 100,
                    elapsed_seconds: 15,
                },
                Some("Node-1 entered canary bake"),
            )
        })
        .unwrap();

        // Complete rollout
        RolloutRegistry::modify(&paths, |reg| {
            reg.update_stage(
                &rollout_id,
                RolloutStage::Succeeded { completed_at: 200 },
                Some("Rollout succeeded for all nodes"),
            )
        })
        .unwrap();

        let reg = RolloutRegistry::load(&paths).unwrap();
        assert!(reg.get_active_rollout("survival-cluster").is_none());
        let record = reg.get_rollout(&rollout_id).unwrap();
        assert_eq!(record.stage.name(), "Succeeded");
        assert_eq!(record.logs.len(), 3);
    }
}
