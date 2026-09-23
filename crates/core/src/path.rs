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
        })
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
