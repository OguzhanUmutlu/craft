use crate::error::{CraftError, Result};
use crate::path::CraftPaths;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DrPlan {
    pub server_name: String,
    pub primary_node: String,
    #[serde(default)]
    pub failover_nodes: Vec<String>,
    #[serde(default = "default_rto")]
    pub target_rto_seconds: u64,
    #[serde(default = "default_rpo")]
    pub target_rpo_hours: u32,
    #[serde(default)]
    pub mesh_targets: Vec<String>,
    #[serde(default)]
    pub reconstitution_steps: Vec<String>,
    pub created_at: DateTime<Utc>,
}

fn default_rto() -> u64 {
    30 // 30 seconds recovery time objective
}

fn default_rpo() -> u32 {
    1 // 1 hour recovery point objective
}

impl DrPlan {
    pub fn default_for_server(server_name: &str) -> Self {
        Self {
            server_name: server_name.to_string(),
            primary_node: "localhost".to_string(),
            failover_nodes: Vec::new(),
            target_rto_seconds: 30,
            target_rpo_hours: 1,
            mesh_targets: vec!["local-primary".to_string()],
            reconstitution_steps: vec![
                "1. Acquire remote host lock and verify environment".to_string(),
                "2. Fetch latest content-addressed manifest from storage mesh".to_string(),
                "3. Concurrently pull and decrypt missing deduplicated chunks".to_string(),
                "4. Reconstitute directory tree and verify SHA-256 Merkle root".to_string(),
                "5. Register server in destination node and configure firewall rules".to_string(),
                "6. Cold-start server instance and verify port binding".to_string(),
            ],
            created_at: Utc::now(),
        }
    }
}

pub type DrRecoveryResult = DrSimulationResult;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrSimulationResult {
    pub server_name: String,
    pub timestamp: DateTime<Utc>,
    pub rto_seconds: f64,
    pub total_files: usize,
    pub total_bytes: u64,
    pub divergent_bytes: u64,
    pub is_valid: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_message: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DrFailoverReport {
    pub server_name: String,
    pub source_node: String,
    pub target_node: String,
    pub timestamp: DateTime<Utc>,
    pub duration_seconds: f64,
    pub chunks_transferred: usize,
    pub bytes_transferred: u64,
    pub status: String,
}

pub struct DrRunbook;

impl DrRunbook {
    pub fn plan_path(paths: &CraftPaths, server_name: &str) -> PathBuf {
        paths.dr_dir.join(format!("{}.toml", server_name))
    }

    pub fn load_plan(paths: &CraftPaths, server_name: &str) -> Result<DrPlan> {
        let path = Self::plan_path(paths, server_name);
        if path.exists() {
            let content = fs::read_to_string(&path)?;
            let plan: DrPlan = toml::from_str(&content)
                .map_err(|e| CraftError::Config(format!("Failed to parse DR plan: {}", e)))?;
            return Ok(plan);
        }
        Ok(DrPlan::default_for_server(server_name))
    }

    pub fn save_plan(paths: &CraftPaths, plan: &DrPlan) -> Result<()> {
        let path = Self::plan_path(paths, &plan.server_name);
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(plan)
            .map_err(|e| CraftError::Config(format!("Failed to serialize DR plan: {}", e)))?;
        fs::write(&path, content)?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_dr_plan_roundtrip() {
        let temp = tempdir().unwrap();
        let paths = CraftPaths::from_base(temp.path().to_path_buf());

        let mut plan = DrRunbook::load_plan(&paths, "survival").unwrap();
        assert_eq!(plan.server_name, "survival");
        assert_eq!(plan.target_rto_seconds, 30);

        plan.failover_nodes.push("vps-backup-node".to_string());
        DrRunbook::save_plan(&paths, &plan).unwrap();

        let loaded = DrRunbook::load_plan(&paths, "survival").unwrap();
        assert_eq!(loaded.failover_nodes, vec!["vps-backup-node".to_string()]);
    }
}
