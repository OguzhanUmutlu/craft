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
    pub ebpf_dir: PathBuf,
    pub ebpf_flamegraphs_dir: PathBuf,
    pub ebpf_probes_file: PathBuf,
    pub ebpf_lock: PathBuf,
    pub supply_chain_dir: PathBuf,
    pub supply_chain_trust_anchors_file: PathBuf,
    pub supply_chain_attestations_dir: PathBuf,
    pub supply_chain_policy_file: PathBuf,
    pub supply_chain_registry_file: PathBuf,
    pub supply_chain_lock: PathBuf,
    pub pqc_dir: PathBuf,
    pub pqc_keys_dir: PathBuf,
    pub pqc_certs_dir: PathBuf,
    pub pqc_policy_file: PathBuf,
    pub pqc_registry_file: PathBuf,
    pub pqc_lock: PathBuf,
    pub hsm_dir: PathBuf,
    pub hsm_tokens_dir: PathBuf,
    pub hsm_pcr_dir: PathBuf,
    pub hsm_zk_dir: PathBuf,
    pub hsm_registry_file: PathBuf,
    pub hsm_lock: PathBuf,
    pub compaction_dir: PathBuf,
    pub compaction_registry_file: PathBuf,
    pub compaction_lock: PathBuf,
    pub xdp_dir: PathBuf,
    pub xdp_rules_file: PathBuf,
    pub xdp_state_file: PathBuf,
    pub xdp_lock: PathBuf,
    pub pmu_dir: PathBuf,
    pub pmu_probes_file: PathBuf,
    pub pmu_state_file: PathBuf,
    pub pmu_lock: PathBuf,
    pub shm_dir: PathBuf,
    pub shm_registry_file: PathBuf,
    pub shm_state_file: PathBuf,
    pub shm_lock: PathBuf,
    pub patch_dir: PathBuf,
    pub patch_registry_file: PathBuf,
    pub patch_state_file: PathBuf,
    pub patch_lock: PathBuf,
    pub vm_dir: PathBuf,
    pub vm_registry_file: PathBuf,
    pub vm_state_file: PathBuf,
    pub vm_lock: PathBuf,
    pub crash_dir: PathBuf,
    pub crash_registry_file: PathBuf,
    pub crash_state_file: PathBuf,
    pub crash_reports_dir: PathBuf,
    pub crash_dumps_dir: PathBuf,
    pub crash_lock: PathBuf,
    pub rdma_dir: PathBuf,
    pub rdma_registry_file: PathBuf,
    pub rdma_state_file: PathBuf,
    pub rdma_lock: PathBuf,
    pub smartnic_dir: PathBuf,
    pub smartnic_registry_file: PathBuf,
    pub smartnic_state_file: PathBuf,
    pub smartnic_lock: PathBuf,
    pub memfabric_dir: PathBuf,
    pub memfabric_registry_file: PathBuf,
    pub memfabric_state_file: PathBuf,
    pub memfabric_lock: PathBuf,
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
        let ebpf_dir = home.join("ebpf");
        let ebpf_flamegraphs_dir = ebpf_dir.join("flamegraphs");
        let ebpf_probes_file = ebpf_dir.join("probes.toml");
        let ebpf_lock = locks_dir.join("ebpf.lock");
        let supply_chain_dir = home.join("supply_chain");
        let supply_chain_trust_anchors_file = supply_chain_dir.join("trust_anchors.json");
        let supply_chain_attestations_dir = supply_chain_dir.join("attestations");
        let supply_chain_policy_file = supply_chain_dir.join("policy.toml");
        let supply_chain_registry_file = supply_chain_dir.join("registry.toml");
        let supply_chain_lock = locks_dir.join("supply_chain.lock");
        let pqc_dir = home.join("pqc");
        let pqc_keys_dir = pqc_dir.join("keys");
        let pqc_certs_dir = pqc_dir.join("certs");
        let pqc_policy_file = pqc_dir.join("policy.json");
        let pqc_registry_file = pqc_dir.join("registry.json");
        let pqc_lock = locks_dir.join("pqc.lock");
        let hsm_dir = home.join("hsm");
        let hsm_tokens_dir = hsm_dir.join("tokens");
        let hsm_pcr_dir = hsm_dir.join("pcr");
        let hsm_zk_dir = hsm_dir.join("zk");
        let hsm_registry_file = hsm_dir.join("registry.json");
        let hsm_lock = locks_dir.join("hsm.lock");
        let compaction_dir = home.join("compaction");
        let compaction_registry_file = compaction_dir.join("registry.json");
        let compaction_lock = locks_dir.join("compaction.lock");
        let xdp_dir = home.join("xdp");
        let xdp_rules_file = xdp_dir.join("rules.json");
        let xdp_state_file = xdp_dir.join("state.json");
        let xdp_lock = locks_dir.join("xdp.lock");
        let pmu_dir = home.join("pmu");
        let pmu_probes_file = pmu_dir.join("probes.json");
        let pmu_state_file = pmu_dir.join("state.json");
        let pmu_lock = locks_dir.join("pmu.lock");
        let shm_dir = home.join("shm");
        let shm_registry_file = shm_dir.join("registry.json");
        let shm_state_file = shm_dir.join("state.json");
        let shm_lock = locks_dir.join("shm.lock");
        let patch_dir = home.join("patching");
        let patch_registry_file = patch_dir.join("registry.json");
        let patch_state_file = patch_dir.join("state.json");
        let patch_lock = locks_dir.join("patch.lock");
        let vm_dir = home.join("vms");
        let vm_registry_file = vm_dir.join("registry.json");
        let vm_state_file = vm_dir.join("state.json");
        let vm_lock = locks_dir.join("vm.lock");
        let crash_dir = home.join("crash");
        let crash_registry_file = crash_dir.join("registry.json");
        let crash_state_file = crash_dir.join("state.json");
        let crash_reports_dir = crash_dir.join("reports");
        let crash_dumps_dir = crash_dir.join("dumps");
        let crash_lock = locks_dir.join("crash.lock");
        let rdma_dir = home.join("rdma");
        let rdma_registry_file = rdma_dir.join("registry.json");
        let rdma_state_file = rdma_dir.join("state.json");
        let rdma_lock = locks_dir.join("rdma.lock");
        let smartnic_dir = home.join("smartnic");
        let smartnic_registry_file = smartnic_dir.join("registry.json");
        let smartnic_state_file = smartnic_dir.join("state.json");
        let smartnic_lock = locks_dir.join("smartnic.lock");
        let memfabric_dir = home.join("memfabric");
        let memfabric_registry_file = memfabric_dir.join("registry.json");
        let memfabric_state_file = memfabric_dir.join("state.json");
        let memfabric_lock = locks_dir.join("memfabric.lock");

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
            ebpf_dir,
            ebpf_flamegraphs_dir,
            ebpf_probes_file,
            ebpf_lock,
            supply_chain_dir,
            supply_chain_trust_anchors_file,
            supply_chain_attestations_dir,
            supply_chain_policy_file,
            supply_chain_registry_file,
            supply_chain_lock,
            pqc_dir,
            pqc_keys_dir,
            pqc_certs_dir,
            pqc_policy_file,
            pqc_registry_file,
            pqc_lock,
            hsm_dir,
            hsm_tokens_dir,
            hsm_pcr_dir,
            hsm_zk_dir,
            hsm_registry_file,
            hsm_lock,
            compaction_dir,
            compaction_registry_file,
            compaction_lock,
            xdp_dir,
            xdp_rules_file,
            xdp_state_file,
            xdp_lock,
            pmu_dir,
            pmu_probes_file,
            pmu_state_file,
            pmu_lock,
            shm_dir,
            shm_registry_file,
            shm_state_file,
            shm_lock,
            patch_dir,
            patch_registry_file,
            patch_state_file,
            patch_lock,
            vm_dir,
            vm_registry_file,
            vm_state_file,
            vm_lock,
            crash_dir,
            crash_registry_file,
            crash_state_file,
            crash_reports_dir,
            crash_dumps_dir,
            crash_lock,
            rdma_dir,
            rdma_registry_file,
            rdma_state_file,
            rdma_lock,
            smartnic_dir,
            smartnic_registry_file,
            smartnic_state_file,
            smartnic_lock,
            memfabric_dir,
            memfabric_registry_file,
            memfabric_state_file,
            memfabric_lock,
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
        let ebpf_dir = home.join("ebpf");
        let ebpf_flamegraphs_dir = ebpf_dir.join("flamegraphs");
        let supply_chain_dir = home.join("supply_chain");
        let supply_chain_attestations_dir = supply_chain_dir.join("attestations");
        let pqc_dir = home.join("pqc");
        let pqc_keys_dir = pqc_dir.join("keys");
        let pqc_certs_dir = pqc_dir.join("certs");
        let hsm_dir = home.join("hsm");
        let hsm_tokens_dir = hsm_dir.join("tokens");
        let hsm_pcr_dir = hsm_dir.join("pcr");
        let hsm_zk_dir = hsm_dir.join("zk");
        let xdp_dir = home.join("xdp");
        let pmu_dir = home.join("pmu");
        let shm_dir = home.join("shm");
        let patch_dir = home.join("patching");
        let vm_dir = home.join("vms");
        let crash_dir = home.join("crash");
        let crash_reports_dir = crash_dir.join("reports");
        let crash_dumps_dir = crash_dir.join("dumps");
        let rdma_dir = home.join("rdma");
        let smartnic_dir = home.join("smartnic");
        let memfabric_dir = home.join("memfabric");

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
            &ebpf_dir,
            &ebpf_flamegraphs_dir,
            &supply_chain_dir,
            &supply_chain_attestations_dir,
            &pqc_dir,
            &pqc_keys_dir,
            &pqc_certs_dir,
            &hsm_dir,
            &hsm_tokens_dir,
            &hsm_pcr_dir,
            &hsm_zk_dir,
            &xdp_dir,
            &pmu_dir,
            &shm_dir,
            &patch_dir,
            &vm_dir,
            &crash_dir,
            &crash_reports_dir,
            &crash_dumps_dir,
            &rdma_dir,
            &smartnic_dir,
            &memfabric_dir,
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
        let ebpf_probes_file = ebpf_dir.join("probes.toml");
        let ebpf_lock = locks_dir.join("ebpf.lock");
        let supply_chain_trust_anchors_file = supply_chain_dir.join("trust_anchors.json");
        let supply_chain_policy_file = supply_chain_dir.join("policy.toml");
        let supply_chain_registry_file = supply_chain_dir.join("registry.toml");
        let supply_chain_lock = locks_dir.join("supply_chain.lock");
        let pqc_policy_file = pqc_dir.join("policy.json");
        let pqc_registry_file = pqc_dir.join("registry.json");
        let pqc_lock = locks_dir.join("pqc.lock");
        let hsm_registry_file = hsm_dir.join("registry.json");
        let hsm_lock = locks_dir.join("hsm.lock");
        let compaction_dir = home.join("compaction");
        let compaction_registry_file = compaction_dir.join("registry.json");
        let compaction_lock = locks_dir.join("compaction.lock");
        let xdp_rules_file = xdp_dir.join("rules.json");
        let xdp_state_file = xdp_dir.join("state.json");
        let xdp_lock = locks_dir.join("xdp.lock");
        let pmu_probes_file = pmu_dir.join("probes.json");
        let pmu_state_file = pmu_dir.join("state.json");
        let pmu_lock = locks_dir.join("pmu.lock");
        let shm_registry_file = shm_dir.join("registry.json");
        let shm_state_file = shm_dir.join("state.json");
        let shm_lock = locks_dir.join("shm.lock");
        let patch_registry_file = patch_dir.join("registry.json");
        let patch_state_file = patch_dir.join("state.json");
        let patch_lock = locks_dir.join("patch.lock");
        let vm_registry_file = vm_dir.join("registry.json");
        let vm_state_file = vm_dir.join("state.json");
        let vm_lock = locks_dir.join("vm.lock");
        let crash_registry_file = crash_dir.join("registry.json");
        let crash_state_file = crash_dir.join("state.json");
        let crash_lock = locks_dir.join("crash.lock");
        let rdma_registry_file = rdma_dir.join("registry.json");
        let rdma_state_file = rdma_dir.join("state.json");
        let rdma_lock = locks_dir.join("rdma.lock");
        let smartnic_registry_file = smartnic_dir.join("registry.json");
        let smartnic_state_file = smartnic_dir.join("state.json");
        let smartnic_lock = locks_dir.join("smartnic.lock");
        let memfabric_registry_file = memfabric_dir.join("registry.json");
        let memfabric_state_file = memfabric_dir.join("state.json");
        let memfabric_lock = locks_dir.join("memfabric.lock");

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
            ebpf_dir,
            ebpf_flamegraphs_dir,
            ebpf_probes_file,
            ebpf_lock,
            supply_chain_dir,
            supply_chain_trust_anchors_file,
            supply_chain_attestations_dir,
            supply_chain_policy_file,
            supply_chain_registry_file,
            supply_chain_lock,
            pqc_dir,
            pqc_keys_dir,
            pqc_certs_dir,
            pqc_policy_file,
            pqc_registry_file,
            pqc_lock,
            hsm_dir,
            hsm_tokens_dir,
            hsm_pcr_dir,
            hsm_zk_dir,
            hsm_registry_file,
            hsm_lock,
            compaction_dir,
            compaction_registry_file,
            compaction_lock,
            xdp_dir,
            xdp_rules_file,
            xdp_state_file,
            xdp_lock,
            pmu_dir,
            pmu_probes_file,
            pmu_state_file,
            pmu_lock,
            shm_dir,
            shm_registry_file,
            shm_state_file,
            shm_lock,
            patch_dir,
            patch_registry_file,
            patch_state_file,
            patch_lock,
            vm_dir,
            vm_registry_file,
            vm_state_file,
            vm_lock,
            crash_dir,
            crash_registry_file,
            crash_state_file,
            crash_reports_dir,
            crash_dumps_dir,
            crash_lock,
            rdma_dir,
            rdma_registry_file,
            rdma_state_file,
            rdma_lock,
            smartnic_dir,
            smartnic_registry_file,
            smartnic_state_file,
            smartnic_lock,
            memfabric_dir,
            memfabric_registry_file,
            memfabric_state_file,
            memfabric_lock,
        })
    }

    /// Returns the page binary snapshot path for a specific page ID
    pub fn memfabric_page_path(&self, page_id: &str) -> PathBuf {
        self.memfabric_dir.join(format!("{}.page", page_id))
    }

    /// Returns the path to a specific P4 program file
    pub fn smartnic_p4_path(&self, name: &str) -> PathBuf {
        self.smartnic_dir.join(format!("{}.p4", name))
    }

    /// Returns the memory region file path for a specific MR ID
    pub fn rdma_mr_path(&self, mr_id: &str) -> PathBuf {
        self.rdma_dir.join(format!("{}.mr", mr_id))
    }

    /// Returns the crash report JSON file path for a specific report ID
    pub fn crash_report_path(&self, report_id: &str) -> PathBuf {
        self.crash_reports_dir.join(format!("{}.json", report_id))
    }

    /// Returns the patch backup/prologue storage path
    pub fn patch_backup_path(&self, server: &str, patch_name: &str) -> PathBuf {
        self.patch_dir.join(format!("{}_{}.prologue", server.replace('/', "_"), patch_name.replace('/', "_")))
    }

    /// Returns the SHM segment file path for a specific channel name
    pub fn shm_segment_path(&self, name: &str) -> PathBuf {
        let clean = name.trim_start_matches('/');
        self.shm_dir.join(format!("{}.shm", clean))
    }

    /// Returns the PMU probes file path
    pub fn pmu_probes_path(&self) -> PathBuf {
        self.pmu_probes_file.clone()
    }

    /// Returns the XDP rules file path
    pub fn xdp_rules_path(&self) -> PathBuf {
        self.xdp_rules_file.clone()
    }

    /// Returns the token file path for a specific HSM token ID
    pub fn hsm_token_path(&self, token_id: &str) -> PathBuf {
        self.hsm_tokens_dir.join(format!("{}.json", token_id))
    }

    /// Returns the PCR measurement file path for a specific PCR measurement ID
    pub fn hsm_pcr_path(&self, pcr_id: &str) -> PathBuf {
        self.hsm_pcr_dir.join(format!("{}.json", pcr_id))
    }

    /// Returns the ZKP membership commitment file path for a cluster ID
    pub fn hsm_zk_path(&self, cluster_id: &str) -> PathBuf {
        self.hsm_zk_dir.join(format!("{}.json", cluster_id))
    }

    /// Returns the compaction profile path for a specific server or node
    pub fn compaction_profile_path(&self, identifier: &str) -> PathBuf {
        self.compaction_dir.join(format!("{}.json", identifier))
    }

    /// Returns the public or private key path for a specific PQC key identifier
    pub fn pqc_key_path(&self, key_id: &str, is_secret: bool) -> PathBuf {
        let ext = if is_secret { "key" } else { "pub" };
        self.pqc_keys_dir.join(format!("{}.{}", key_id, ext))
    }

    /// Returns the attestation file path for a given artifact SHA-256 digest
    pub fn supply_chain_attestation_path(&self, sha256: &str) -> PathBuf {
        self.supply_chain_attestations_dir.join(format!("{}.json", sha256))
    }

    /// Returns the flamegraph SVG file path for a specific server or profiling session
    pub fn ebpf_flamegraph_path(&self, server: &str) -> PathBuf {
        self.ebpf_flamegraphs_dir.join(format!("{}.svg", server))
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
