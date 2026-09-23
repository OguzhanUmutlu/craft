use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use fs2::FileExt;
use serde::{Deserialize, Serialize};
use std::fs::{self, OpenOptions};
use std::path::Path;

use crate::error::{CraftError, Result};
use crate::path::CraftPaths;

pub const DEFAULT_VCPU_HOURLY_COST: f64 = 0.04;
pub const DEFAULT_RAM_GIB_HOURLY_COST: f64 = 0.005;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ResourceTier {
    Idle,
    Low,
    Normal,
    High,
    Surge,
}

impl ResourceTier {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Idle => "Idle",
            Self::Low => "Low",
            Self::Normal => "Normal",
            Self::High => "High",
            Self::Surge => "Surge",
        }
    }
}

impl std::fmt::Display for ResourceTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HourlyWorkloadSample {
    pub timestamp: DateTime<Utc>,
    pub player_count: u32,
    pub avg_mspt: f64,
    pub max_mspt: f64,
    pub memory_rss_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ForecastPoint {
    pub timestamp: DateTime<Utc>,
    pub expected_players: f64,
    pub lower_bound_p10: f64,
    pub upper_bound_p90: f64,
    pub surge_risk: bool,
    pub recommended_tier: ResourceTier,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadForecast {
    pub server_name: String,
    pub generated_at: DateTime<Utc>,
    pub horizon_hours: u32,
    pub points: Vec<ForecastPoint>,
    pub peak_time: DateTime<Utc>,
    pub peak_players: f64,
    pub quiet_window_start: Option<u8>,
    pub quiet_window_end: Option<u8>,
    pub next_surge_predicted_in_mins: Option<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiurnalHourProfile {
    pub hour_multipliers: [f64; 24],
    pub hour_sample_counts: [u32; 24],
}

impl Default for DiurnalHourProfile {
    fn default() -> Self {
        // Standard default player diurnal curve (low early morning, high evening UTC)
        let mut multipliers = [1.0f64; 24];
        multipliers[0] = 0.65;
        multipliers[1] = 0.45;
        multipliers[2] = 0.30;
        multipliers[3] = 0.20;
        multipliers[4] = 0.15;
        multipliers[5] = 0.15;
        multipliers[6] = 0.20;
        multipliers[7] = 0.35;
        multipliers[8] = 0.50;
        multipliers[9] = 0.65;
        multipliers[10] = 0.80;
        multipliers[11] = 0.95;
        multipliers[12] = 1.10;
        multipliers[13] = 1.25;
        multipliers[14] = 1.40;
        multipliers[15] = 1.55;
        multipliers[16] = 1.70;
        multipliers[17] = 1.85;
        multipliers[18] = 2.00;
        multipliers[19] = 2.10;
        multipliers[20] = 1.95;
        multipliers[21] = 1.65;
        multipliers[22] = 1.30;
        multipliers[23] = 0.95;

        Self {
            hour_multipliers: multipliers,
            hour_sample_counts: [0; 24],
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DayOfWeekProfile {
    pub day_multipliers: [f64; 7],
}

impl Default for DayOfWeekProfile {
    fn default() -> Self {
        // Sunday=0, Monday=1, ..., Saturday=6
        // Weekends have ~30-40% higher baseline than weekdays
        Self {
            day_multipliers: [1.35, 0.85, 0.85, 0.90, 0.95, 1.30, 1.40],
        }
    }
}

pub struct SeasonalForecaster {
    pub diurnal: DiurnalHourProfile,
    pub weekly: DayOfWeekProfile,
    pub baseline_trend: f64,
    pub residual_std_dev: f64,
}

impl SeasonalForecaster {
    pub fn new() -> Self {
        Self {
            diurnal: DiurnalHourProfile::default(),
            weekly: DayOfWeekProfile::default(),
            baseline_trend: 10.0,
            residual_std_dev: 2.0,
        }
    }

    /// Fits the seasonal decomposition model using historical hourly samples.
    pub fn fit(samples: &[HourlyWorkloadSample]) -> Self {
        if samples.is_empty() {
            return Self::new();
        }

        let mut sum_players = 0.0;
        let mut hour_sums = [0.0f64; 24];
        let mut hour_counts = [0u32; 24];
        let mut day_sums = [0.0f64; 7];
        let mut day_counts = [0u32; 7];

        for s in samples {
            let p = s.player_count as f64;
            sum_players += p;
            let h = s.timestamp.hour() as usize;
            hour_sums[h] += p;
            hour_counts[h] += 1;

            let d = s.timestamp.weekday().num_days_from_sunday() as usize;
            day_sums[d] += p;
            day_counts[d] += 1;
        }

        let overall_avg = (sum_players / samples.len() as f64).max(0.5);

        let mut hour_multipliers = [1.0f64; 24];
        for h in 0..24 {
            if hour_counts[h] > 0 {
                let h_avg = hour_sums[h] / hour_counts[h] as f64;
                hour_multipliers[h] = (h_avg / overall_avg).max(0.05);
            } else {
                hour_multipliers[h] = DiurnalHourProfile::default().hour_multipliers[h];
            }
        }

        let mut day_multipliers = [1.0f64; 7];
        for d in 0..7 {
            if day_counts[d] > 0 {
                let d_avg = day_sums[d] / day_counts[d] as f64;
                day_multipliers[d] = (d_avg / overall_avg).max(0.1);
            } else {
                day_multipliers[d] = DayOfWeekProfile::default().day_multipliers[d];
            }
        }

        // Calculate residual standard deviation
        let mut variance_sum = 0.0;
        for s in samples {
            let h = s.timestamp.hour() as usize;
            let d = s.timestamp.weekday().num_days_from_sunday() as usize;
            let expected = overall_avg * hour_multipliers[h] * day_multipliers[d];
            let diff = s.player_count as f64 - expected;
            variance_sum += diff * diff;
        }
        let residual_std_dev = (variance_sum / samples.len().max(1) as f64).sqrt().max(1.0);

        Self {
            diurnal: DiurnalHourProfile {
                hour_multipliers,
                hour_sample_counts: hour_counts,
            },
            weekly: DayOfWeekProfile { day_multipliers },
            baseline_trend: overall_avg,
            residual_std_dev,
        }
    }

    /// Predicts future workload over a designated horizon in hours.
    pub fn forecast(
        &self,
        server_name: &str,
        start_time: DateTime<Utc>,
        horizon_hours: u32,
    ) -> WorkloadForecast {
        let mut points = Vec::with_capacity(horizon_hours as usize);
        let mut peak_time = start_time;
        let mut peak_players = 0.0;
        let mut next_surge_predicted_in_mins = None;

        for step in 1..=horizon_hours {
            let target_time = start_time + Duration::hours(step as i64);
            let h = target_time.hour() as usize;
            let d = target_time.weekday().num_days_from_sunday() as usize;

            let seasonal_factor = self.diurnal.hour_multipliers[h] * self.weekly.day_multipliers[d];
            let expected = (self.baseline_trend * seasonal_factor).max(0.0);

            // P10 is ~1.28 standard deviations below, P90 is ~1.28 standard deviations above
            let lower_p10 = (expected - 1.28 * self.residual_std_dev).max(0.0);
            let upper_p90 = expected + 1.28 * self.residual_std_dev;

            let surge_risk = upper_p90 >= 20.0 || (expected >= self.baseline_trend * 1.5 && expected >= 10.0);

            let recommended_tier = if expected < 1.0 && upper_p90 < 2.0 {
                ResourceTier::Idle
            } else if expected < 5.0 {
                ResourceTier::Low
            } else if expected < 25.0 {
                ResourceTier::Normal
            } else if expected < 60.0 {
                ResourceTier::High
            } else {
                ResourceTier::Surge
            };

            if expected > peak_players {
                peak_players = expected;
                peak_time = target_time;
            }

            if surge_risk && next_surge_predicted_in_mins.is_none() {
                let diff_mins = (target_time - start_time).num_minutes().max(0) as u32;
                next_surge_predicted_in_mins = Some(diff_mins);
            }

            points.push(ForecastPoint {
                timestamp: target_time,
                expected_players: (expected * 10.0).round() / 10.0,
                lower_bound_p10: (lower_p10 * 10.0).round() / 10.0,
                upper_bound_p90: (upper_p90 * 10.0).round() / 10.0,
                surge_risk,
                recommended_tier,
            });
        }

        // Identify quiet window (lowest contiguous 4-6 hour interval in diurnal profile)
        let mut min_window_sum = f64::MAX;
        let mut quiet_start = 2u8;
        for start_h in 0..24 {
            let mut window_sum = 0.0;
            for offset in 0..5 {
                let idx = (start_h + offset) % 24;
                window_sum += self.diurnal.hour_multipliers[idx];
            }
            if window_sum < min_window_sum {
                min_window_sum = window_sum;
                quiet_start = start_h as u8;
            }
        }
        let quiet_end = (quiet_start + 5) % 24;

        WorkloadForecast {
            server_name: server_name.to_string(),
            generated_at: start_time,
            horizon_hours,
            points,
            peak_time,
            peak_players: (peak_players * 10.0).round() / 10.0,
            quiet_window_start: Some(quiet_start),
            quiet_window_end: Some(quiet_end),
            next_surge_predicted_in_mins,
        }
    }
}

impl Default for SeasonalForecaster {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CostOptimizationReport {
    pub server_name: String,
    pub total_tracked_hours: f64,
    pub active_hours: f64,
    pub hibernated_hours: f64,
    pub vcpu_hours_saved: f64,
    pub ram_gib_hours_saved: f64,
    pub realized_savings_usd: f64,
    pub projected_monthly_savings_usd: f64,
    pub efficiency_score: f64,
}

pub struct CostOptimizationModel;

impl CostOptimizationModel {
    pub fn compute_savings(
        server_name: &str,
        tracked_hours: f64,
        hibernated_hours: f64,
        allocated_vcpus: f64,
        allocated_ram_gib: f64,
        hourly_vcpu_cost: f64,
        hourly_ram_gib_cost: f64,
    ) -> CostOptimizationReport {
        let active_hours = (tracked_hours - hibernated_hours).max(0.0);
        let vcpu_hours_saved = hibernated_hours * allocated_vcpus;
        let ram_gib_hours_saved = hibernated_hours * allocated_ram_gib;

        let realized_savings = (vcpu_hours_saved * hourly_vcpu_cost)
            + (ram_gib_hours_saved * hourly_ram_gib_cost);

        let savings_rate_per_hour = if tracked_hours > 0.0 {
            realized_savings / tracked_hours
        } else {
            0.0
        };

        let projected_monthly_savings = savings_rate_per_hour * 730.0;

        let efficiency_score = if tracked_hours > 0.0 {
            ((hibernated_hours / tracked_hours) * 100.0).clamp(0.0, 100.0)
        } else {
            0.0
        };

        CostOptimizationReport {
            server_name: server_name.to_string(),
            total_tracked_hours: (tracked_hours * 10.0).round() / 10.0,
            active_hours: (active_hours * 10.0).round() / 10.0,
            hibernated_hours: (hibernated_hours * 10.0).round() / 10.0,
            vcpu_hours_saved: (vcpu_hours_saved * 10.0).round() / 10.0,
            ram_gib_hours_saved: (ram_gib_hours_saved * 10.0).round() / 10.0,
            realized_savings_usd: (realized_savings * 100.0).round() / 100.0,
            projected_monthly_savings_usd: (projected_monthly_savings * 100.0).round() / 100.0,
            efficiency_score: (efficiency_score * 10.0).round() / 10.0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceThrottlingPlan {
    pub tier: ResourceTier,
    pub heap_min_mb: u32,
    pub heap_max_mb: u32,
    pub parallel_gc_threads: u32,
    pub conc_gc_threads: u32,
    pub cpu_quota_percent: u32,
    pub allow_hibernation: bool,
    pub jvm_flags: Vec<String>,
}

impl ResourceThrottlingPlan {
    pub fn for_tier(tier: ResourceTier, base_max_heap_mb: u32) -> Self {
        match tier {
            ResourceTier::Idle => Self {
                tier,
                heap_min_mb: (base_max_heap_mb / 4).max(512),
                heap_max_mb: (base_max_heap_mb / 2).max(1024),
                parallel_gc_threads: 1,
                conc_gc_threads: 1,
                cpu_quota_percent: 25,
                allow_hibernation: true,
                jvm_flags: vec![
                    format!("-Xms{}M", (base_max_heap_mb / 4).max(512)),
                    format!("-Xmx{}M", (base_max_heap_mb / 2).max(1024)),
                    "-XX:ParallelGCThreads=1".to_string(),
                    "-XX:ConcGCThreads=1".to_string(),
                    "-XX:+UseG1GC".to_string(),
                ],
            },
            ResourceTier::Low => Self {
                tier,
                heap_min_mb: (base_max_heap_mb / 2).max(1024),
                heap_max_mb: (base_max_heap_mb * 3 / 4).max(2048),
                parallel_gc_threads: 2,
                conc_gc_threads: 1,
                cpu_quota_percent: 50,
                allow_hibernation: false,
                jvm_flags: vec![
                    format!("-Xms{}M", (base_max_heap_mb / 2).max(1024)),
                    format!("-Xmx{}M", (base_max_heap_mb * 3 / 4).max(2048)),
                    "-XX:ParallelGCThreads=2".to_string(),
                    "-XX:ConcGCThreads=1".to_string(),
                    "-XX:+UseG1GC".to_string(),
                ],
            },
            ResourceTier::Normal => Self {
                tier,
                heap_min_mb: base_max_heap_mb,
                heap_max_mb: base_max_heap_mb,
                parallel_gc_threads: 4,
                conc_gc_threads: 2,
                cpu_quota_percent: 100,
                allow_hibernation: false,
                jvm_flags: vec![
                    format!("-Xms{}M", base_max_heap_mb),
                    format!("-Xmx{}M", base_max_heap_mb),
                    "-XX:ParallelGCThreads=4".to_string(),
                    "-XX:ConcGCThreads=2".to_string(),
                    "-XX:+UseG1GC".to_string(),
                ],
            },
            ResourceTier::High => Self {
                tier,
                heap_min_mb: (base_max_heap_mb * 5 / 4).max(base_max_heap_mb),
                heap_max_mb: (base_max_heap_mb * 5 / 4).max(base_max_heap_mb),
                parallel_gc_threads: 6,
                conc_gc_threads: 3,
                cpu_quota_percent: 100,
                allow_hibernation: false,
                jvm_flags: vec![
                    format!("-Xms{}M", (base_max_heap_mb * 5 / 4).max(base_max_heap_mb)),
                    format!("-Xmx{}M", (base_max_heap_mb * 5 / 4).max(base_max_heap_mb)),
                    "-XX:ParallelGCThreads=6".to_string(),
                    "-XX:ConcGCThreads=3".to_string(),
                    "-XX:+UseG1GC".to_string(),
                    "-XX:G1ReservePercent=15".to_string(),
                ],
            },
            ResourceTier::Surge => Self {
                tier,
                heap_min_mb: (base_max_heap_mb * 3 / 2).max(base_max_heap_mb),
                heap_max_mb: (base_max_heap_mb * 3 / 2).max(base_max_heap_mb),
                parallel_gc_threads: 8,
                conc_gc_threads: 4,
                cpu_quota_percent: 100,
                allow_hibernation: false,
                jvm_flags: vec![
                    format!("-Xms{}M", (base_max_heap_mb * 3 / 2).max(base_max_heap_mb)),
                    format!("-Xmx{}M", (base_max_heap_mb * 3 / 2).max(base_max_heap_mb)),
                    "-XX:ParallelGCThreads=8".to_string(),
                    "-XX:ConcGCThreads=4".to_string(),
                    "-XX:+UseG1GC".to_string(),
                    "-XX:G1ReservePercent=20".to_string(),
                ],
            },
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WorkloadPolicy {
    pub server_name: String,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_lead_time")]
    pub proactive_wake_lead_mins: u32,
    #[serde(default = "default_min_players")]
    pub proactive_wake_min_players: u32,
    #[serde(default = "default_true")]
    pub auto_throttling: bool,
    #[serde(default)]
    pub quiet_window_start_utc: Option<u8>,
    #[serde(default)]
    pub quiet_window_end_utc: Option<u8>,
    #[serde(default = "default_vcpu_cost")]
    pub hourly_vcpu_cost: f64,
    #[serde(default = "default_ram_cost")]
    pub hourly_ram_gib_cost: f64,
}

fn default_true() -> bool {
    true
}
fn default_lead_time() -> u32 {
    15
}
fn default_min_players() -> u32 {
    1
}
fn default_vcpu_cost() -> f64 {
    DEFAULT_VCPU_HOURLY_COST
}
fn default_ram_cost() -> f64 {
    DEFAULT_RAM_GIB_HOURLY_COST
}

impl WorkloadPolicy {
    pub fn new(server_name: impl Into<String>) -> Self {
        Self {
            server_name: server_name.into(),
            enabled: true,
            proactive_wake_lead_mins: 15,
            proactive_wake_min_players: 1,
            auto_throttling: true,
            quiet_window_start_utc: Some(2),
            quiet_window_end_utc: Some(7),
            hourly_vcpu_cost: DEFAULT_VCPU_HOURLY_COST,
            hourly_ram_gib_cost: DEFAULT_RAM_GIB_HOURLY_COST,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ForecastingRegistry {
    #[serde(default)]
    pub policies: Vec<WorkloadPolicy>,
}

impl ForecastingRegistry {
    pub fn load(paths: &CraftPaths) -> Result<Self> {
        if paths.forecasting_file.exists() {
            let content = fs::read_to_string(&paths.forecasting_file)?;
            let reg: ForecastingRegistry = toml::from_str(&content)
                .map_err(|e| CraftError::Config(format!("Failed to parse forecasting.toml: {}", e)))?;
            return Ok(reg);
        }
        Ok(Self::default())
    }

    pub fn save(&self, paths: &CraftPaths) -> Result<()> {
        let _ = fs::create_dir_all(&paths.locks_dir);
        let lock_file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .open(&paths.forecasting_lock)
            .map_err(|e| CraftError::Config(format!("Failed to open forecasting.lock: {}", e)))?;

        lock_file
            .lock_exclusive()
            .map_err(|e| CraftError::Config(format!("Failed to acquire forecasting.lock: {}", e)))?;

        let content = toml::to_string_pretty(self)
            .map_err(|e| CraftError::Config(format!("Failed to serialize forecasting.toml: {}", e)))?;
        let temp_path = paths.forecasting_file.with_extension("tmp");
        fs::write(&temp_path, content)?;
        fs::rename(&temp_path, &paths.forecasting_file)?;

        let _ = lock_file.unlock();
        Ok(())
    }

    pub fn find(&self, server_name: &str) -> Option<&WorkloadPolicy> {
        self.policies.iter().find(|p| p.server_name == server_name)
    }

    pub fn upsert(&mut self, policy: WorkloadPolicy) {
        if let Some(pos) = self.policies.iter().position(|p| p.server_name == policy.server_name) {
            self.policies[pos] = policy;
        } else {
            self.policies.push(policy);
        }
    }

    pub fn modify<F, R>(&mut self, paths: &CraftPaths, f: F) -> Result<R>
    where
        F: FnOnce(&mut Self) -> Result<R>,
    {
        *self = Self::load(paths)?;
        let result = f(self)?;
        self.save(paths)?;
        Ok(result)
    }
}

/// Generates an 8-level Unicode sparkline representation of forecast curve.
pub fn generate_forecast_sparkline(points: &[ForecastPoint]) -> String {
    if points.is_empty() {
        return String::new();
    }

    let spark_chars = [' ', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let values: Vec<f64> = points.iter().map(|p| p.expected_players).collect();
    let min_v = values.iter().copied().fold(f64::INFINITY, f64::min);
    let max_v = values.iter().copied().fold(f64::NEG_INFINITY, f64::max);

    let range = (max_v - min_v).max(0.001);

    values
        .into_iter()
        .map(|v| {
            let normalized = ((v - min_v) / range).clamp(0.0, 1.0);
            let idx = ((normalized * 7.0).round() as usize).min(7);
            spark_chars[idx]
        })
        .collect()
}

/// Saves raw workload samples to disk in json format.
pub fn save_workload_samples(file_path: &Path, samples: &[HourlyWorkloadSample]) -> Result<()> {
    if let Some(parent) = file_path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let serialized = serde_json::to_vec_pretty(samples)
        .map_err(|e| CraftError::Other(format!("Failed to serialize workload samples: {}", e)))?;
    fs::write(file_path, serialized)
        .map_err(|e| CraftError::Other(format!("Failed to write workload samples: {}", e)))?;
    Ok(())
}

/// Loads workload samples from disk.
pub fn load_workload_samples(file_path: &Path) -> Result<Vec<HourlyWorkloadSample>> {
    if !file_path.exists() {
        return Ok(Vec::new());
    }
    let content = fs::read(file_path)
        .map_err(|e| CraftError::Other(format!("Failed to read workload samples: {}", e)))?;
    let samples: Vec<HourlyWorkloadSample> = serde_json::from_slice(&content)
        .map_err(|e| CraftError::Other(format!("Failed to parse workload samples: {}", e)))?;
    Ok(samples)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seasonal_forecaster_fit_and_predict() {
        let now = Utc::now();
        let mut samples = Vec::new();

        // Generate synthetic diurnal curve across 7 days (168 hours)
        for h in 0..168 {
            let ts = now - Duration::hours(168 - h);
            let hour = ts.hour() as f64;
            // Evening peak around 19:00 UTC (hour 19), trough at 04:00 UTC (hour 4)
            let diurnal_component = ((hour - 4.0).abs() / 15.0) * 20.0;
            let player_count = (diurnal_component + 5.0) as u32;

            samples.push(HourlyWorkloadSample {
                timestamp: ts,
                player_count,
                avg_mspt: 25.0,
                max_mspt: 35.0,
                memory_rss_mb: 2048,
            });
        }

        let forecaster = SeasonalForecaster::fit(&samples);
        let forecast = forecaster.forecast("test-server", now, 24);

        assert_eq!(forecast.server_name, "test-server");
        assert_eq!(forecast.points.len(), 24);
        assert!(forecast.peak_players > 0.0);
        assert!(forecast.quiet_window_start.is_some());
    }

    #[test]
    fn test_cost_optimization_model() {
        let report = CostOptimizationModel::compute_savings(
            "survival-node",
            720.0, // 30 days
            360.0, // 12 hours/day hibernated
            4.0,   // 4 vCPUs
            8.0,   // 8 GiB RAM
            DEFAULT_VCPU_HOURLY_COST,
            DEFAULT_RAM_GIB_HOURLY_COST,
        );

        assert_eq!(report.server_name, "survival-node");
        assert_eq!(report.hibernated_hours, 360.0);
        assert!(report.vcpu_hours_saved > 1000.0);
        assert!(report.ram_gib_hours_saved > 2000.0);
        assert!(report.realized_savings_usd > 50.0);
        assert!(report.efficiency_score >= 49.0 && report.efficiency_score <= 51.0);
    }

    #[test]
    fn test_resource_throttling_plans() {
        let idle_plan = ResourceThrottlingPlan::for_tier(ResourceTier::Idle, 4096);
        assert!(idle_plan.allow_hibernation);
        assert_eq!(idle_plan.parallel_gc_threads, 1);

        let surge_plan = ResourceThrottlingPlan::for_tier(ResourceTier::Surge, 4096);
        assert!(!surge_plan.allow_hibernation);
        assert_eq!(surge_plan.parallel_gc_threads, 8);
        assert!(surge_plan.heap_max_mb >= 4096);
    }

    #[test]
    fn test_sparkline_generation() {
        let points = vec![
            ForecastPoint {
                timestamp: Utc::now(),
                expected_players: 0.0,
                lower_bound_p10: 0.0,
                upper_bound_p90: 0.0,
                surge_risk: false,
                recommended_tier: ResourceTier::Idle,
            },
            ForecastPoint {
                timestamp: Utc::now(),
                expected_players: 50.0,
                lower_bound_p10: 40.0,
                upper_bound_p90: 60.0,
                surge_risk: true,
                recommended_tier: ResourceTier::High,
            },
            ForecastPoint {
                timestamp: Utc::now(),
                expected_players: 100.0,
                lower_bound_p10: 80.0,
                upper_bound_p90: 120.0,
                surge_risk: true,
                recommended_tier: ResourceTier::Surge,
            },
        ];

        let spark = generate_forecast_sparkline(&points);
        assert_eq!(spark.chars().count(), 3);
    }
}
