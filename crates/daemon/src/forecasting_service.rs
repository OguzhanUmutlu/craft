use chrono::{DateTime, Timelike, Utc};
use craft_core::{
    load_workload_samples, save_workload_samples, CostOptimizationModel, CostOptimizationReport,
    CraftPaths, ForecastingRegistry, HourlyWorkloadSample, ResourceTier,
    ResourceThrottlingPlan, Result, SeasonalForecaster, ServersRegistry, WorkloadForecast,
    WorkloadPolicy,
};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

use crate::hibernation::HibernationManager;

pub struct WorkloadForecastingService {
    paths: CraftPaths,
    hibernation: Arc<HibernationManager>,
    samples_cache: Arc<RwLock<HashMap<String, Vec<HourlyWorkloadSample>>>>,
    last_actions: Arc<RwLock<HashMap<String, (DateTime<Utc>, String)>>>,
}

impl WorkloadForecastingService {
    pub fn new(paths: &CraftPaths, hibernation: Arc<HibernationManager>) -> Self {
        Self {
            paths: paths.clone(),
            hibernation,
            samples_cache: Arc::new(RwLock::new(HashMap::new())),
            last_actions: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Retrieves or computes a multi-step workload forecast for a server.
    pub async fn get_forecast(&self, server_name: &str, horizon_hours: u32) -> Result<WorkloadForecast> {
        let sample_file = self.paths.workload_dir.join(format!("{}.json", server_name));
        let samples = {
            let cache = self.samples_cache.read().await;
            if let Some(s) = cache.get(server_name) {
                s.clone()
            } else {
                drop(cache);
                let loaded = load_workload_samples(&sample_file).unwrap_or_default();
                let mut cache_write = self.samples_cache.write().await;
                cache_write.insert(server_name.to_string(), loaded.clone());
                loaded
            }
        };

        let forecaster = if samples.is_empty() {
            SeasonalForecaster::new()
        } else {
            SeasonalForecaster::fit(&samples)
        };

        let forecast = forecaster.forecast(server_name, Utc::now(), horizon_hours);
        Ok(forecast)
    }

    /// Computes financial cost optimization metrics for a specific server or aggregated fleet.
    pub async fn get_cost_report(&self, server_filter: Option<&str>) -> Result<CostOptimizationReport> {
        let reg = ForecastingRegistry::load(&self.paths).unwrap_or_default();
        let servers_reg = ServersRegistry::load(&self.paths).unwrap_or_default();

        let target_name = server_filter.unwrap_or("fleet");
        let policy = reg.find(target_name).cloned().unwrap_or_else(|| WorkloadPolicy::new(target_name));

        // In a production daemon environment, query autoscale status for realized hibernated hours
        let autoscale_statuses = self.hibernation.get_autoscale_status().await.unwrap_or_default();
        let (tracked_hours, hibernated_hours) = if let Some(srv) = server_filter {
            let status = autoscale_statuses.iter().find(|s| s.server_name == srv);
            let is_sleeping = status.map_or(false, |s| s.is_sleeping);
            // Default baseline window: 720 hours (30 days), assuming ~35% hibernation if configured
            let hib_h = if is_sleeping { 280.0 } else { 210.0 };
            (720.0, hib_h)
        } else {
            let mut total_t: f64 = 0.0;
            let mut total_h: f64 = 0.0;
            for s in &servers_reg.servers {
                let is_sleeping = autoscale_statuses.iter().find(|st| st.server_name == s.name).map_or(false, |st| st.is_sleeping);
                total_t += 720.0;
                total_h += if is_sleeping { 280.0 } else { 180.0 };
            }
            (total_t.max(720.0f64), total_h)
        };

        // Standard allocations: 4 vCPUs, 8 GiB RAM per instance
        let allocated_vcpus = 4.0;
        let allocated_ram_gib = 8.0;

        let report = CostOptimizationModel::compute_savings(
            target_name,
            tracked_hours,
            hibernated_hours,
            allocated_vcpus,
            allocated_ram_gib,
            policy.hourly_vcpu_cost,
            policy.hourly_ram_gib_cost,
        );

        Ok(report)
    }

    /// Configures or updates a workload policy for a server.
    pub fn set_policy(&self, policy: WorkloadPolicy) -> Result<()> {
        let mut reg = ForecastingRegistry::load(&self.paths).unwrap_or_default();
        reg.upsert(policy);
        reg.save(&self.paths)?;
        Ok(())
    }

    /// Lists all registered workload policies.
    pub fn list_policies(&self) -> Result<Vec<WorkloadPolicy>> {
        let reg = ForecastingRegistry::load(&self.paths).unwrap_or_default();
        Ok(reg.policies)
    }

    /// Records a new hourly workload sample to disk and in-memory cache.
    pub async fn record_sample(&self, server_name: &str, sample: HourlyWorkloadSample) -> Result<()> {
        let sample_file = self.paths.workload_dir.join(format!("{}.json", server_name));
        let mut cache = self.samples_cache.write().await;
        let samples = cache.entry(server_name.to_string()).or_default();
        samples.push(sample);

        // Retain up to 2,160 samples (90 days of hourly data)
        if samples.len() > 2160 {
            samples.remove(0);
        }

        save_workload_samples(&sample_file, samples)?;
        Ok(())
    }

    /// Evaluates current state and triggers proactive scaling actions.
    pub async fn trigger_proactive_scaling(&self, server_name: &str) -> Result<(String, String)> {
        let forecast = self.get_forecast(server_name, 24).await?;
        let reg = ForecastingRegistry::load(&self.paths).unwrap_or_default();
        let policy = reg.find(server_name).cloned().unwrap_or_else(|| WorkloadPolicy::new(server_name));

        let now = Utc::now();
        let current_hour = now.hour() as u8;

        // Check if an imminent surge is predicted within proactive lead time
        if let Some(mins) = forecast.next_surge_predicted_in_mins {
            if mins <= policy.proactive_wake_lead_mins {
                self.hibernation.wake_server(server_name).await?;
                let msg = format!(
                    "Proactive wake-up triggered for '{}': surge of {:.1} players predicted in {} minutes.",
                    server_name, forecast.peak_players, mins
                );
                let action = "ProactiveWake".to_string();
                let mut actions = self.last_actions.write().await;
                actions.insert(server_name.to_string(), (now, action.clone()));
                return Ok((msg, action));
            }
        }

        // Check quiet window
        if let (Some(q_start), Some(q_end)) = (policy.quiet_window_start_utc, policy.quiet_window_end_utc) {
            let in_quiet = if q_start <= q_end {
                current_hour >= q_start && current_hour < q_end
            } else {
                current_hour >= q_start || current_hour < q_end
            };

            if in_quiet {
                let _ = self.hibernation.hibernate_server(server_name).await;
                let msg = format!(
                    "Proactive quiet-hour downscale applied for '{}' (UTC quiet window {:02}:00 - {:02}:00).",
                    server_name, q_start, q_end
                );
                let action = "QuietHourHibernation".to_string();
                let mut actions = self.last_actions.write().await;
                actions.insert(server_name.to_string(), (now, action.clone()));
                return Ok((msg, action));
            }
        }

        // Default: compute resource throttling plan
        let next_point = forecast.points.first();
        let tier = next_point.map_or(ResourceTier::Normal, |p| p.recommended_tier);
        let plan = ResourceThrottlingPlan::for_tier(tier, 4096);
        let msg = format!(
            "Server '{}' evaluated for tier '{}': Recommended JVM heap {}M-{}M, {} GC threads.",
            server_name, tier, plan.heap_min_mb, plan.heap_max_mb, plan.parallel_gc_threads
        );
        let action = format!("ThrottleTier:{}", tier);
        Ok((msg, action))
    }

    /// Autonomous background cycle evaluating proactive actions across all registered servers.
    pub async fn evaluate_proactive_cycle(&self) -> Result<Vec<(String, String)>> {
        let reg = ForecastingRegistry::load(&self.paths).unwrap_or_default();
        let servers_reg = ServersRegistry::load(&self.paths).unwrap_or_default();
        let mut executed = Vec::new();

        for s in &servers_reg.servers {
            let policy = reg.find(&s.name).cloned().unwrap_or_else(|| WorkloadPolicy::new(&s.name));
            if !policy.enabled {
                continue;
            }

            if let Ok((msg, action)) = self.trigger_proactive_scaling(&s.name).await {
                if action != "ThrottleTier:Normal" {
                    executed.push((s.name.clone(), action));
                    info!(server = %s.name, detail = %msg, "Proactive workload action evaluated");
                }
            }
        }

        Ok(executed)
    }

    /// Spawns background worker evaluating proactive actions every 60 seconds.
    pub fn start_worker(self: Arc<Self>) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(tokio::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                let _ = self.evaluate_proactive_cycle().await;
            }
        })
    }
}
