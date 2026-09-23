use craft_core::{
    CgroupV2Driver, CraftError, CraftPaths, QuotaRegistry, QuotaUsageSummary, Result,
    ServerPriority, ServerResourceLimit, TenantQuota,
};
use std::collections::HashSet;
use std::fs;
use tracing::{info, warn};

/// Background and in-process coordinator service for cgroups v2 resource isolation and fair-share scheduling
pub struct QuotaService;

impl QuotaService {
    /// Notification hook when a server process is started
    pub fn on_server_started(paths: &CraftPaths, server_name: &str, pid: u32) -> Result<()> {
        let driver = CgroupV2Driver::new(paths);
        let registry = QuotaRegistry::load(paths).unwrap_or_default();

        let limit = registry
            .get_server_limit(server_name)
            .cloned()
            .unwrap_or_else(|| ServerResourceLimit::new(server_name));

        // Create cgroup hierarchy and apply limits
        driver.apply_limits(server_name, &limit)?;

        // Attach PID to cgroup.procs
        driver.attach_pid(server_name, pid)?;

        info!(
            "[OK] Provisioned cgroup v2 for server '{}' (PID: {}, Priority: {})",
            server_name,
            pid,
            limit.priority.name()
        );

        Ok(())
    }

    /// Notification hook when a server process is stopped
    pub fn on_server_stopped(paths: &CraftPaths, server_name: &str) -> Result<()> {
        let driver = CgroupV2Driver::new(paths);
        driver.destroy_cgroup(server_name)?;
        info!("[OK] Torn down cgroup v2 for server '{}'", server_name);
        Ok(())
    }

    /// Inspects resource limits and cgroups v2 live statistics for a server
    pub fn get_server_quota(paths: &CraftPaths, server_name: &str) -> Result<QuotaUsageSummary> {
        let driver = CgroupV2Driver::new(paths);
        let registry = QuotaRegistry::load(paths).unwrap_or_default();

        let limits = registry
            .get_server_limit(server_name)
            .cloned()
            .unwrap_or_else(|| ServerResourceLimit::new(server_name));

        let stats = driver.read_stats(server_name).unwrap_or_default();

        let procs_file = driver.server_cgroup_dir(server_name).join("cgroup.procs");
        let (active, pid) = if procs_file.exists() {
            let pid = fs::read_to_string(&procs_file)
                .ok()
                .and_then(|s| s.lines().next().and_then(|l| l.trim().parse::<u32>().ok()));
            (pid.is_some(), pid)
        } else {
            (false, None)
        };

        let throttled = stats.cpu_throttle_ratio > 0.15 || stats.memory_high_events > 0;

        let health_indicator = if stats.memory_oom_events > 0 || stats.memory_oom_kill_events > 0 {
            "[OOM_RISK]".to_string()
        } else if throttled {
            "[THROTTLED]".to_string()
        } else if stats.memory_current_bytes > 0 || stats.cpu_usage_usec > 0 {
            "[NORMAL]".to_string()
        } else {
            "[PRISTINE]".to_string()
        };

        Ok(QuotaUsageSummary {
            server_name: server_name.to_string(),
            tenant_id: limits.tenant_id.clone(),
            active,
            pid,
            limits,
            stats,
            throttled,
            health_indicator,
        })
    }

    /// Sets or updates resource limits for a server, hot-applying to cgroup v2
    pub fn set_server_quota(
        paths: &CraftPaths,
        limits: ServerResourceLimit,
    ) -> Result<QuotaUsageSummary> {
        let mut registry = QuotaRegistry::load(paths)?;
        registry.set_server_limit(limits.clone())?;
        registry.save(paths)?;

        let driver = CgroupV2Driver::new(paths);
        // Hot-apply if cgroup exists
        if driver.server_cgroup_dir(&limits.server_name).exists() {
            if let Err(e) = driver.apply_limits(&limits.server_name, &limits) {
                warn!(
                    "Failed to hot-apply cgroup limits to '{}': {}",
                    limits.server_name, e
                );
            }
        }

        Self::get_server_quota(paths, &limits.server_name)
    }

    /// Queries quota allocation and aggregate usage for a tenant
    pub fn get_tenant_quota(
        paths: &CraftPaths,
        tenant_id: &str,
    ) -> Result<(TenantQuota, u64, u32, usize)> {
        let registry = QuotaRegistry::load(paths)?;
        let quota = registry.get_tenant_quota(tenant_id).cloned().ok_or_else(|| {
            CraftError::Config(format!("Tenant '{}' not found in QuotaRegistry", tenant_id))
        })?;

        let (count, mem_bytes, cpu_percent) = registry.calculate_tenant_usage(tenant_id);
        let mem_mb = mem_bytes / (1024 * 1024);

        Ok((quota, mem_mb, cpu_percent, count))
    }

    /// Sets or updates a tenant quota budget
    pub fn set_tenant_quota(paths: &CraftPaths, quota: TenantQuota) -> Result<()> {
        let mut registry = QuotaRegistry::load(paths)?;
        registry.set_tenant_quota(quota);
        registry.save(paths)
    }

    /// Lists quota usage and statistics across all servers
    pub fn list_quota_usage(
        paths: &CraftPaths,
        tenant_filter: Option<&str>,
    ) -> Result<Vec<QuotaUsageSummary>> {
        let registry = QuotaRegistry::load(paths).unwrap_or_default();
        let driver = CgroupV2Driver::new(paths);

        let mut server_names = HashSet::new();
        for name in registry.server_limits.keys() {
            server_names.insert(name.clone());
        }
        for active in driver.list_active_cgroups() {
            server_names.insert(active);
        }

        let mut list = Vec::new();
        for server in server_names {
            if let Ok(summary) = Self::get_server_quota(paths, &server) {
                if let Some(filter) = tenant_filter {
                    if summary.tenant_id.as_deref().unwrap_or("default") != filter {
                        continue;
                    }
                }
                list.push(summary);
            }
        }

        list.sort_by(|a, b| a.server_name.cmp(&b.server_name));
        Ok(list)
    }

    /// Enforces fair-share scheduling arbitration across all active server cgroups
    pub fn enforce_fair_share(paths: &CraftPaths) -> Result<(usize, String)> {
        let driver = CgroupV2Driver::new(paths);
        let registry = QuotaRegistry::load(paths).unwrap_or_default();
        let active_cgroups = driver.list_active_cgroups();

        let mut rebalanced_count = 0;
        for server in &active_cgroups {
            let mut limit = registry
                .get_server_limit(server)
                .cloned()
                .unwrap_or_else(|| ServerResourceLimit::new(server));

            // Guarantee baseline weights according to priority class
            let target_cpu_weight = match limit.priority {
                ServerPriority::GatewayProxy => limit.cpu_weight.max(500),
                ServerPriority::StandardWorld => limit.cpu_weight.clamp(100, 200),
                ServerPriority::BackgroundWorker => limit.cpu_weight.clamp(50, 100),
                ServerPriority::BatchTask => limit.cpu_weight.clamp(20, 50),
            };

            let target_io_weight = match limit.priority {
                ServerPriority::GatewayProxy => limit.io_weight.max(500),
                ServerPriority::StandardWorld => limit.io_weight.clamp(100, 200),
                ServerPriority::BackgroundWorker => limit.io_weight.clamp(50, 100),
                ServerPriority::BatchTask => limit.io_weight.clamp(20, 50),
            };

            limit.cpu_weight = target_cpu_weight;
            limit.io_weight = target_io_weight;

            if driver.apply_limits(server, &limit).is_ok() {
                rebalanced_count += 1;
            }
        }

        let msg = format!(
            "[OK] Rebalanced cgroups v2 fair-share weights for {} active servers",
            rebalanced_count
        );
        info!("{}", msg);
        Ok((rebalanced_count, msg))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_quota_service_lifecycle_and_summary() {
        let dir = tempdir().unwrap();
        let paths = CraftPaths::from_base(dir.path().to_path_buf());

        let srv = "alpha-lobby";
        let pid = 4242;

        let limit = ServerResourceLimit::new(srv)
            .with_memory_mb(1024, Some(768))
            .with_cpu_percent(100)
            .with_priority(ServerPriority::GatewayProxy);

        let set_res = QuotaService::set_server_quota(&paths, limit.clone()).unwrap();
        assert_eq!(set_res.server_name, srv);
        assert_eq!(set_res.limits.priority, ServerPriority::GatewayProxy);

        QuotaService::on_server_started(&paths, srv, pid).unwrap();

        let summary = QuotaService::get_server_quota(&paths, srv).unwrap();
        assert_eq!(summary.server_name, srv);
        assert_eq!(summary.pid, Some(pid));
        assert!(summary.active);

        let (rebalanced, _) = QuotaService::enforce_fair_share(&paths).unwrap();
        assert_eq!(rebalanced, 1);

        QuotaService::on_server_stopped(&paths, srv).unwrap();
        let after_stop = QuotaService::get_server_quota(&paths, srv).unwrap();
        assert!(!after_stop.active);
    }
}
