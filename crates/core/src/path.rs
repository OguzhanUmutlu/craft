use crate::error::{CraftError, Result};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct CraftPaths {
    pub home: PathBuf,
    pub servers_dir: PathBuf,
    pub softwares_dir: PathBuf,
    pub cache_dir: PathBuf,
    pub backups_dir: PathBuf,
    pub run_dir: PathBuf,
    pub locks_dir: PathBuf,
    pub logs_dir: PathBuf,
    pub trash_dir: PathBuf,
    pub servers_file: PathBuf,
    pub remotes_file: PathBuf,
    pub clusters_file: PathBuf,
    pub webhooks_file: PathBuf,
    pub autoscale_file: PathBuf,
    pub rbac_file: PathBuf,
    pub audit_file: PathBuf,
    pub mesh_file: PathBuf,
    pub chunks_dir: PathBuf,
    pub dr_dir: PathBuf,
    pub intelligence_file: PathBuf,
    pub diagnostics_dir: PathBuf,
    pub intelligence_lock: PathBuf,
    pub edge_file: PathBuf,
    pub edge_lock: PathBuf,
    pub edge_dir: PathBuf,
    pub config_file: PathBuf,
    pub rollouts_file: PathBuf,
    pub rollouts_lock: PathBuf,
    pub socket_file: PathBuf,
    pub pid_file: PathBuf,
    pub indices_dir: PathBuf,
    pub forensics_dir: PathBuf,
    pub forecasting_file: PathBuf,
    pub forecasting_lock: PathBuf,
    pub workload_dir: PathBuf,
    pub modpack_ci_dir: PathBuf,
    pub delta_cache_dir: PathBuf,
    pub modpack_registry_file: PathBuf,
    pub modpack_lock: PathBuf,
    pub sdn_dir: PathBuf,
    pub sdn_mesh_file: PathBuf,
    pub sdn_certs_dir: PathBuf,
    pub wireguard_dir: PathBuf,
    pub sdn_lock: PathBuf,
    pub raft_dir: PathBuf,
    pub raft_state_file: PathBuf,
    pub raft_wal_dir: PathBuf,
    pub raft_snapshots_dir: PathBuf,
    pub raft_lock: PathBuf,
    pub raft_groups_dir: PathBuf,
    pub multiraft_file: PathBuf,
    pub multiraft_lock: PathBuf,
    pub quotas_dir: PathBuf,
    pub quotas_file: PathBuf,
    pub quotas_lock: PathBuf,
    pub cgroups_dir: PathBuf,
    pub tracing_dir: PathBuf,
    pub tracing_file: PathBuf,
    pub tracing_lock: PathBuf,
    pub traces_spans_dir: PathBuf,
    pub anvil_dir: PathBuf,
    pub anvil_file: PathBuf,
    pub anvil_lock: PathBuf,
    pub anvil_cache_dir: PathBuf,
    pub numa_dir: PathBuf,
    pub numa_file: PathBuf,
    pub numa_lock: PathBuf,
    pub migrations_dir: PathBuf,
    pub migration_snapshots_dir: PathBuf,
    pub migrations_file: PathBuf,
    pub migrations_lock: PathBuf,
}

impl CraftPaths {
    pub fn from_base(home: PathBuf) -> Self {
        let servers_dir = home.join("servers");
        let softwares_dir = home.join("softwares");
        let cache_dir = home.join("cache");
        let backups_dir = home.join("backups");
        let run_dir = home.join("run");
        let locks_dir = run_dir.join("locks");
        let logs_dir = home.join("logs");
        let trash_dir = home.join("trash");
        let chunks_dir = cache_dir.join("chunks");
        let dr_dir = home.join("dr");
        let diagnostics_dir = home.join("diagnostics");
        let edge_dir = home.join("edge");
        let indices_dir = home.join("indices");
        let forensics_dir = home.join("forensics");

        let servers_file = home.join("servers.toml");
        let remotes_file = home.join("remotes.toml");
        let clusters_file = home.join("clusters.toml");
        let webhooks_file = home.join("webhooks.toml");
        let autoscale_file = home.join("autoscale.toml");
        let rbac_file = home.join("rbac.toml");
        let audit_file = home.join("audit.log");
        let mesh_file = home.join("mesh.toml");
        let intelligence_file = home.join("intelligence.toml");
        let intelligence_lock = locks_dir.join("intelligence.lock");
        let edge_file = home.join("edge.toml");
        let edge_lock = locks_dir.join("edge.lock");
        let config_file = home.join("config.toml");
        let rollouts_file = home.join("rollouts.toml");
        let rollouts_lock = locks_dir.join("rollouts.lock");
        let socket_file = run_dir.join("daemon.sock");
        let pid_file = run_dir.join("daemon.pid");
        let forecasting_file = home.join("forecasting.toml");
        let forecasting_lock = locks_dir.join("forecasting.lock");
        let workload_dir = diagnostics_dir.join("workload");
        let modpack_ci_dir = home.join("modpacks").join("ci");
        let delta_cache_dir = cache_dir.join("deltas");
        let modpack_registry_file = home.join("modpacks.toml");
        let modpack_lock = locks_dir.join("modpack.lock");
        let sdn_dir = home.join("sdn");
        let sdn_mesh_file = sdn_dir.join("mesh.toml");
        let sdn_certs_dir = sdn_dir.join("certs");
        let wireguard_dir = sdn_dir.join("wireguard");
        let sdn_lock = locks_dir.join("sdn.lock");
        let raft_dir = home.join("raft");
        let raft_state_file = raft_dir.join("state.toml");
        let raft_wal_dir = raft_dir.join("wal");
        let raft_snapshots_dir = raft_dir.join("snapshots");
        let raft_lock = locks_dir.join("raft.lock");
        let raft_groups_dir = raft_dir.join("groups");
        let multiraft_file = raft_dir.join("multiraft.toml");
        let multiraft_lock = locks_dir.join("multiraft.lock");
        let quotas_dir = home.join("quotas");
        let quotas_file = quotas_dir.join("quotas.toml");
        let quotas_lock = locks_dir.join("quotas.lock");
        let cgroups_dir = home.join("cgroups");
        let tracing_dir = home.join("tracing");
        let tracing_file = tracing_dir.join("tracing.toml");
        let tracing_lock = locks_dir.join("tracing.lock");
        let traces_spans_dir = tracing_dir.join("spans");
        let anvil_dir = home.join("anvil");
        let anvil_file = anvil_dir.join("anvil.toml");
        let anvil_lock = locks_dir.join("anvil.lock");
        let anvil_cache_dir = cache_dir.join("anvil");
        let numa_dir = home.join("numa");
        let numa_file = numa_dir.join("numa.toml");
        let numa_lock = locks_dir.join("numa.lock");
        let migrations_dir = home.join("migrations");
        let migration_snapshots_dir = migrations_dir.join("snapshots");
        let migrations_file = migrations_dir.join("migrations.toml");
        let migrations_lock = locks_dir.join("migrations.lock");

        Self {
            home,
            servers_dir,
            softwares_dir,
            cache_dir,
            backups_dir,
            run_dir,
            locks_dir,
            logs_dir,
            trash_dir,
            servers_file,
            remotes_file,
            clusters_file,
            webhooks_file,
            autoscale_file,
            rbac_file,
            audit_file,
            mesh_file,
            chunks_dir,
            dr_dir,
            intelligence_file,
            diagnostics_dir,
            intelligence_lock,
            edge_file,
            edge_lock,
            edge_dir,
            config_file,
            rollouts_file,
            rollouts_lock,
            socket_file,
            pid_file,
            indices_dir,
            forensics_dir,
            forecasting_file,
            forecasting_lock,
            workload_dir,
            modpack_ci_dir,
            delta_cache_dir,
            modpack_registry_file,
            modpack_lock,
            sdn_dir,
            sdn_mesh_file,
            sdn_certs_dir,
            wireguard_dir,
            sdn_lock,
            raft_dir,
            raft_state_file,
            raft_wal_dir,
            raft_snapshots_dir,
            raft_lock,
            raft_groups_dir,
            multiraft_file,
            multiraft_lock,
            quotas_dir,
            quotas_file,
            quotas_lock,
            cgroups_dir,
            tracing_dir,
            tracing_file,
            tracing_lock,
            traces_spans_dir,
            anvil_dir,
            anvil_file,
            anvil_lock,
            anvil_cache_dir,
            numa_dir,
            numa_file,
            numa_lock,
            migrations_dir,
            migration_snapshots_dir,
            migrations_file,
            migrations_lock,
        }
    }

    pub fn new() -> Result<Self> {
        let home = if let Ok(val) = env::var("CRAFT_HOME") {
            PathBuf::from(val)
        } else {
            let user_home = directories::UserDirs::new()
                .ok_or_else(|| {
                    CraftError::Config("Unable to locate user home directory".to_string())
                })?
                .home_dir()
                .to_path_buf();

            user_home.join(".craft")
        };

        let servers_dir = home.join("servers");
        let softwares_dir = home.join("softwares");
        let cache_dir = home.join("cache");
        let backups_dir = home.join("backups");
        let run_dir = home.join("run");
        let locks_dir = run_dir.join("locks");
        let logs_dir = home.join("logs");
        let trash_dir = home.join("trash");
        let chunks_dir = cache_dir.join("chunks");
        let dr_dir = home.join("dr");
        let diagnostics_dir = home.join("diagnostics");
        let edge_dir = home.join("edge");
        let indices_dir = home.join("indices");
        let forensics_dir = home.join("forensics");
        let workload_dir = diagnostics_dir.join("workload");
        let modpack_ci_dir = home.join("modpacks").join("ci");
        let delta_cache_dir = cache_dir.join("deltas");
        let sdn_dir = home.join("sdn");
        let sdn_certs_dir = sdn_dir.join("certs");
        let wireguard_dir = sdn_dir.join("wireguard");
        let raft_dir = home.join("raft");
        let raft_wal_dir = raft_dir.join("wal");
        let raft_snapshots_dir = raft_dir.join("snapshots");
        let raft_groups_dir = raft_dir.join("groups");
        let quotas_dir = home.join("quotas");
        let cgroups_dir = home.join("cgroups");
        let tracing_dir = home.join("tracing");
        let traces_spans_dir = tracing_dir.join("spans");
        let anvil_dir = home.join("anvil");
        let anvil_cache_dir = cache_dir.join("anvil");
        let numa_dir = home.join("numa");
        let migrations_dir = home.join("migrations");
        let migration_snapshots_dir = migrations_dir.join("snapshots");

        // Ensure all primary directories exist
        for dir in [
            &home,
            &servers_dir,
            &softwares_dir,
            &cache_dir,
            &chunks_dir,
            &backups_dir,
            &trash_dir,
            &run_dir,
            &locks_dir,
            &logs_dir,
            &dr_dir,
            &diagnostics_dir,
            &edge_dir,
            &indices_dir,
            &forensics_dir,
            &workload_dir,
            &modpack_ci_dir,
            &delta_cache_dir,
            &sdn_dir,
            &sdn_certs_dir,
            &wireguard_dir,
            &raft_dir,
            &raft_wal_dir,
            &raft_snapshots_dir,
            &raft_groups_dir,
            &quotas_dir,
            &cgroups_dir,
            &tracing_dir,
            &traces_spans_dir,
            &anvil_dir,
            &anvil_cache_dir,
            &numa_dir,
            &migrations_dir,
            &migration_snapshots_dir,
        ] {
            if !dir.exists() {
                fs::create_dir_all(dir)?;
            }
        }

        let servers_file = home.join("servers.toml");
        let remotes_file = home.join("remotes.toml");
        let clusters_file = home.join("clusters.toml");
        let webhooks_file = home.join("webhooks.toml");
        let autoscale_file = home.join("autoscale.toml");
        let rbac_file = home.join("rbac.toml");
        let audit_file = home.join("audit.log");
        let mesh_file = home.join("mesh.toml");
        let intelligence_file = home.join("intelligence.toml");
        let intelligence_lock = locks_dir.join("intelligence.lock");
        let edge_file = home.join("edge.toml");
        let edge_lock = locks_dir.join("edge.lock");
        let config_file = home.join("config.toml");
        let rollouts_file = home.join("rollouts.toml");
        let rollouts_lock = locks_dir.join("rollouts.lock");
        let socket_file = run_dir.join("daemon.sock");
        let pid_file = run_dir.join("daemon.pid");
        let forecasting_file = home.join("forecasting.toml");
        let forecasting_lock = locks_dir.join("forecasting.lock");
        let modpack_registry_file = home.join("modpacks.toml");
        let modpack_lock = locks_dir.join("modpack.lock");
        let sdn_mesh_file = sdn_dir.join("mesh.toml");
        let sdn_lock = locks_dir.join("sdn.lock");
        let raft_state_file = raft_dir.join("state.toml");
        let raft_lock = locks_dir.join("raft.lock");
        let multiraft_file = raft_dir.join("multiraft.toml");
        let multiraft_lock = locks_dir.join("multiraft.lock");
        let quotas_file = quotas_dir.join("quotas.toml");
        let quotas_lock = locks_dir.join("quotas.lock");
        let tracing_file = tracing_dir.join("tracing.toml");
        let tracing_lock = locks_dir.join("tracing.lock");
        let anvil_file = anvil_dir.join("anvil.toml");
        let anvil_lock = locks_dir.join("anvil.lock");
        let numa_file = numa_dir.join("numa.toml");
        let numa_lock = locks_dir.join("numa.lock");
        let migrations_file = migrations_dir.join("migrations.toml");
        let migrations_lock = locks_dir.join("migrations.lock");

        Ok(Self {
            home,
            servers_dir,
            softwares_dir,
            cache_dir,
            backups_dir,
            run_dir,
            locks_dir,
            logs_dir,
            trash_dir,
            servers_file,
            remotes_file,
            clusters_file,
            webhooks_file,
            autoscale_file,
            rbac_file,
            audit_file,
            mesh_file,
            chunks_dir,
            dr_dir,
            intelligence_file,
            diagnostics_dir,
            intelligence_lock,
            edge_file,
            edge_lock,
            edge_dir,
            config_file,
            rollouts_file,
            rollouts_lock,
            socket_file,
            pid_file,
            indices_dir,
            forensics_dir,
            forecasting_file,
            forecasting_lock,
            workload_dir,
            modpack_ci_dir,
            delta_cache_dir,
            modpack_registry_file,
            modpack_lock,
            sdn_dir,
            sdn_mesh_file,
            sdn_certs_dir,
            wireguard_dir,
            sdn_lock,
            raft_dir,
            raft_state_file,
            raft_wal_dir,
            raft_snapshots_dir,
            raft_lock,
            raft_groups_dir,
            multiraft_file,
            multiraft_lock,
            quotas_dir,
            quotas_file,
            quotas_lock,
            cgroups_dir,
            tracing_dir,
            tracing_file,
            tracing_lock,
            traces_spans_dir,
            anvil_dir,
            anvil_file,
            anvil_lock,
            anvil_cache_dir,
            numa_dir,
            numa_file,
            numa_lock,
            migrations_dir,
            migration_snapshots_dir,
            migrations_file,
            migrations_lock,
        })
    }

    /// Returns the staging directory path for a specific server's live migration
    pub fn migration_server_dir(&self, server: &str) -> PathBuf {
        self.migrations_dir.join(server)
    }

    /// Returns the checkpoint directory path for a specific migration instance
    pub fn migration_checkpoint_dir(&self, migration_id: &str) -> PathBuf {
        self.migration_snapshots_dir.join(migration_id)
    }

    /// Returns the directory path for a specific Raft group's state and WAL
    pub fn raft_group_dir(&self, group_id: u64) -> PathBuf {
        self.raft_groups_dir.join(format!("group_{}", group_id))
    }

    /// Returns the WAL directory path for a specific Raft group
    pub fn raft_group_wal(&self, group_id: u64) -> PathBuf {
        self.raft_group_dir(group_id).join("wal")
    }

    /// Returns the snapshots directory path for a specific Raft group
    pub fn raft_group_snapshots(&self, group_id: u64) -> PathBuf {
        self.raft_group_dir(group_id).join("snapshots")
    }

    /// Resolves a server path either from a provided path, a name, or partial name match
    pub fn resolve_server_path(
        &self,
        explicit_path: Option<&Path>,
        name: Option<&str>,
        allow_partial: bool,
    ) -> Result<PathBuf> {
        if let Some(p) = explicit_path {
            return Ok(p.to_path_buf());
        }

        let name = name.ok_or_else(|| {
            CraftError::Config(
                "Please specify a server name with --name, --path or as an argument.".to_string(),
            )
        })?;

        // 1. Check if the name matches any registered server in servers.toml
        if let Ok(registry) = crate::config::ServersRegistry::load(self) {
            if let Some(server) = registry.find_by_name(name) {
                return Ok(server.path.clone());
            }
            // Also check if any registered server's path ends with the name
            for server in &registry.servers {
                if let Some(fname) = server.path.file_name() {
                    if fname.to_string_lossy().eq_ignore_ascii_case(name) {
                        return Ok(server.path.clone());
                    }
                }
            }
        }

        let candidate = self.servers_dir.join(name);
        if candidate.exists() {
            return Ok(candidate);
        }

        if allow_partial {
            let lower_name = name.to_lowercase();
            let mut matches = Vec::new();

            if let Ok(entries) = fs::read_dir(&self.servers_dir) {
                for entry in entries.flatten() {
                    let file_name = entry.file_name().to_string_lossy().to_string();
                    if file_name.to_lowercase().starts_with(&lower_name) {
                        matches.push(entry.path());
                    }
                }
            }

            if matches.len() == 1 {
                return Ok(matches.remove(0));
            } else if matches.len() > 1 {
                return Err(CraftError::Config(format!(
                    "Ambiguous server name '{}'. Matches: {}",
                    name,
                    matches
                        .iter()
                        .filter_map(|p| p.file_name().map(|f| f.to_string_lossy().to_string()))
                        .collect::<Vec<_>>()
                        .join(", ")
                )));
            }
        }

        Ok(candidate)
    }
}
