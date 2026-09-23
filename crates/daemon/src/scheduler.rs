use chrono::{DateTime, Datelike, Timelike, Utc};
use craft_backup::{enforce_retention, BackupEngine, BackupFormat};
use craft_core::{CraftPaths, GlobalBackupRegistry, ServersRegistry};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use tracing::{error, info, warn};

use crate::supervisor::Supervisor;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CronField {
    Any,
    Exact(u32),
    Step(u32),
    Range(u32, u32),
    List(Vec<u32>),
}

impl CronField {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s == "*" {
            return Some(CronField::Any);
        }
        if let Some(step_str) = s.strip_prefix("*/") {
            if let Ok(step) = step_str.parse::<u32>() {
                if step > 0 {
                    return Some(CronField::Step(step));
                }
            }
            return None;
        }
        if s.contains(',') {
            let mut list = Vec::new();
            for part in s.split(',') {
                if let Ok(val) = part.trim().parse::<u32>() {
                    list.push(val);
                } else {
                    return None;
                }
            }
            if !list.is_empty() {
                return Some(CronField::List(list));
            }
            return None;
        }
        if s.contains('-') {
            let parts: Vec<&str> = s.split('-').collect();
            if parts.len() == 2 {
                if let (Ok(start), Ok(end)) = (parts[0].trim().parse::<u32>(), parts[1].trim().parse::<u32>()) {
                    if start <= end {
                        return Some(CronField::Range(start, end));
                    }
                }
            }
            return None;
        }
        if let Ok(val) = s.parse::<u32>() {
            return Some(CronField::Exact(val));
        }
        None
    }

    pub fn matches(&self, val: u32) -> bool {
        match self {
            CronField::Any => true,
            CronField::Exact(expected) => *expected == val,
            CronField::Step(step) => val % *step == 0,
            CronField::Range(start, end) => val >= *start && val <= *end,
            CronField::List(list) => list.contains(&val),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CronSchedule {
    pub minute: CronField,
    pub hour: CronField,
    pub day_of_month: CronField,
    pub month: CronField,
    pub day_of_week: CronField,
}

impl CronSchedule {
    pub fn parse(expr: &str) -> Option<Self> {
        let parts: Vec<&str> = expr.split_whitespace().collect();
        if parts.len() != 5 {
            return None;
        }

        Some(Self {
            minute: CronField::parse(parts[0])?,
            hour: CronField::parse(parts[1])?,
            day_of_month: CronField::parse(parts[2])?,
            month: CronField::parse(parts[3])?,
            day_of_week: CronField::parse(parts[4])?,
        })
    }

    pub fn matches(&self, dt: &DateTime<Utc>) -> bool {
        let minute = dt.minute();
        let hour = dt.hour();
        let dom = dt.day();
        let month = dt.month();
        // chrono Sunday is 0 or 7 depending on convention, num_days_from_sunday gives 0..=6
        let dow = dt.weekday().num_days_from_sunday();

        self.minute.matches(minute)
            && self.hour.matches(hour)
            && self.day_of_month.matches(dom)
            && self.month.matches(month)
            && (self.day_of_week.matches(dow) || (dow == 0 && self.day_of_week.matches(7)))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackupSchedule {
    Cron(CronSchedule),
    Interval(Duration),
}

impl BackupSchedule {
    pub fn parse(s: &str) -> Option<Self> {
        let trimmed = s.trim();
        // Try cron first (5 whitespace separated components)
        if let Some(cron) = CronSchedule::parse(trimmed) {
            return Some(BackupSchedule::Cron(cron));
        }

        // Try natural interval
        let lower = trimmed.to_lowercase();
        let rest = lower.strip_prefix("every ").unwrap_or(&lower).trim();

        if let Some(h) = rest.strip_suffix('h') {
            if let Ok(hours) = h.trim().parse::<u64>() {
                return Some(BackupSchedule::Interval(Duration::from_secs(hours * 3600)));
            }
        }
        if let Some(m) = rest.strip_suffix('m') {
            if let Ok(mins) = m.trim().parse::<u64>() {
                return Some(BackupSchedule::Interval(Duration::from_secs(mins * 60)));
            }
        }
        if let Some(d) = rest.strip_suffix('d') {
            if let Ok(days) = d.trim().parse::<u64>() {
                return Some(BackupSchedule::Interval(Duration::from_secs(days * 86400)));
            }
        }
        if let Ok(hours) = rest.parse::<u64>() {
            return Some(BackupSchedule::Interval(Duration::from_secs(hours * 3600)));
        }

        None
    }

    pub fn is_due(&self, last_run: Option<DateTime<Utc>>, now: DateTime<Utc>) -> bool {
        match self {
            BackupSchedule::Interval(dur) => match last_run {
                None => true,
                Some(last) => {
                    let elapsed = now.signed_duration_since(last);
                    elapsed.to_std().unwrap_or_default() >= *dur
                }
            },
            BackupSchedule::Cron(cron) => {
                if !cron.matches(&now) {
                    return false;
                }
                // Avoid re-running within the same minute
                match last_run {
                    None => true,
                    Some(last) => {
                        now.signed_duration_since(last) > chrono::Duration::seconds(59)
                    }
                }
            }
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BackupScheduleInfo {
    pub server_name: String,
    pub enabled: bool,
    pub schedule: String,
    pub retention_count: usize,
    pub format: String,
    pub last_backup: Option<String>,
}

pub struct DaemonScheduler;

impl DaemonScheduler {
    pub fn start(paths: CraftPaths, supervisor: Supervisor) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            info!("Daemon backup scheduler worker initialized.");
            let mut tick_interval = tokio::time::interval(Duration::from_secs(15));

            loop {
                tick_interval.tick().await;

                let mut registry = match GlobalBackupRegistry::load(&paths) {
                    Ok(r) => r,
                    Err(e) => {
                        warn!("Scheduler: Failed to load backup registry: {}", e);
                        continue;
                    }
                };

                let now = Utc::now();
                let mut servers_to_backup = Vec::new();

                for (server_name, policy) in registry.server_policies.iter_mut() {
                    if !policy.enabled {
                        continue;
                    }

                    let schedule = if let Some(ref expr) = policy.cron_expression {
                        BackupSchedule::parse(expr)
                    } else {
                        Some(BackupSchedule::Interval(Duration::from_secs(
                            policy.interval_hours as u64 * 3600,
                        )))
                    };

                    let sched = match schedule {
                        Some(s) => s,
                        None => {
                            warn!(
                                "Server '{}' has invalid backup schedule/cron expression.",
                                server_name
                            );
                            continue;
                        }
                    };

                    let last_dt = policy
                        .last_backup_timestamp
                        .and_then(|ts| DateTime::from_timestamp(ts, 0));

                    if sched.is_due(last_dt, now) {
                        policy.last_backup_timestamp = Some(now.timestamp());
                        servers_to_backup.push((server_name.clone(), policy.clone()));
                    }
                }

                if !servers_to_backup.is_empty() {
                    // Save updated timestamps
                    if let Err(e) = registry.save(&paths) {
                        warn!("Scheduler: Failed to update last backup timestamps: {}", e);
                    }

                    // Execute backups
                    for (server_name, policy) in servers_to_backup {
                        let p = paths.clone();
                        let sup = supervisor.clone();
                        tokio::spawn(async move {
                            Self::execute_scheduled_backup(p, sup, server_name, policy).await;
                        });
                    }
                }
            }
        })
    }

    async fn execute_scheduled_backup(
        paths: CraftPaths,
        supervisor: Supervisor,
        server_name: String,
        policy: craft_core::AutoBackupPolicy,
    ) {
        info!(
            "Scheduler: Starting automated backup for server '{}' (format: {})...",
            server_name,
            policy.compression_format()
        );

        let s_reg = match ServersRegistry::load(&paths) {
            Ok(r) => r,
            Err(e) => {
                error!(
                    "Scheduler: Could not load servers registry for '{}': {}",
                    server_name, e
                );
                return;
            }
        };

        let server_cfg = match s_reg.servers.iter().find(|s| s.name == server_name) {
            Some(s) => s.clone(),
            None => {
                warn!(
                    "Scheduler: Server '{}' not found in servers registry.",
                    server_name
                );
                return;
            }
        };

        let engine = BackupEngine::new(&paths);
        let is_running = supervisor.is_running(&server_cfg.path).await;

        // RCON credentials if running
        let mut rcon_info: Option<(String, u16, String)> = None;
        if is_running {
            if let Some(rcon_port) = server_cfg.rcon_port {
                // Read rcon password from server.properties if available
                let props_file = server_cfg.path.join("server.properties");
                let mut password = String::new();
                if props_file.exists() {
                    if let Ok(content) = std::fs::read_to_string(&props_file) {
                        for line in content.lines() {
                            if let Some(pass) = line.strip_prefix("rcon.password=") {
                                password = pass.trim().to_string();
                                break;
                            }
                        }
                    }
                }
                if !password.is_empty() {
                    rcon_info = Some(("127.0.0.1".to_string(), rcon_port, password));
                }
            }
        }

        let rcon_param = rcon_info
            .as_ref()
            .map(|(h, p, pw)| (h.as_str(), *p, pw.as_str()));

        let format = BackupFormat::from_str_opt(policy.compression_format())
            .unwrap_or(BackupFormat::TarZstd);

        let mut start_ctx = craft_scripting::HookContext::new(craft_scripting::LifecycleEvent::BackupStart);
        start_ctx.server_name = Some(server_name.clone());
        start_ctx.server_path = Some(server_cfg.path.to_string_lossy().to_string());
        craft_scripting::HookBus::dispatch_async(
            paths.clone(),
            craft_scripting::LifecycleEvent::BackupStart,
            start_ctx,
            10,
        );

        let backup_result = engine
            .create_backup(
                &server_name,
                &server_cfg.path,
                rcon_param,
                policy.world_only,
                Some(format),
            )
            .await;

        match backup_result {
            Ok(archive_path) => {
                let meta = std::fs::metadata(&archive_path).ok();
                let size_bytes = meta.as_ref().map(|m| m.len()).unwrap_or(0);
                let size_mb = (size_bytes as f64) / (1024.0 * 1024.0);

                info!(
                    "[OK] Scheduler: Backup created for '{}': {} ({:.2} MB)",
                    server_name,
                    archive_path.display(),
                    size_mb
                );

                let mut comp_ctx = craft_scripting::HookContext::new(craft_scripting::LifecycleEvent::BackupComplete);
                comp_ctx.server_name = Some(server_name.clone());
                comp_ctx.server_path = Some(server_cfg.path.to_string_lossy().to_string());
                comp_ctx.backup_file = Some(archive_path.to_string_lossy().to_string());
                comp_ctx.backup_bytes = Some(size_bytes);
                craft_scripting::HookBus::dispatch_async(
                    paths.clone(),
                    craft_scripting::LifecycleEvent::BackupComplete,
                    comp_ctx,
                    10,
                );

                // Enforce local retention
                enforce_retention(&engine, &server_name, policy.retention_count);

                // Check S3 target upload
                if policy.upload_to_s3 {
                    if let Ok(g_reg) = GlobalBackupRegistry::load(&paths) {
                        if let Some(target) = g_reg.s3_targets.first() {
                            let s3_config: craft_core::S3BackupConfig = target.into();
                            let s3_provider = craft_backup::S3StorageProvider::new(s3_config);
                            let filename = archive_path
                                .file_name()
                                .and_then(|f| f.to_str())
                                .unwrap_or("backup.tar.zst");
                            let remote_key = format!("{}/{}", server_name, filename);
                            info!(
                                "Scheduler: Uploading backup to S3 bucket '{}' key '{}'...",
                                target.bucket, remote_key
                            );
                            use craft_backup::StorageProvider;
                            if let Err(e) = s3_provider.upload_file(&archive_path, &remote_key).await {
                                error!("Scheduler: S3 upload failed for '{}': {}", server_name, e);
                            } else {
                                info!("[OK] Scheduler: S3 upload complete for '{}'", server_name);
                            }
                        }
                    }
                }
                // Dispatch webhook notification
                let size_bytes = std::fs::metadata(&archive_path).map(|m| m.len()).unwrap_or(0);
                let payload = crate::webhooks::WebhookPayload::backup_complete(
                    &server_name,
                    &server_cfg.path,
                    size_bytes,
                    true,
                    None,
                );
                crate::webhooks::WebhookDispatcher::dispatch(payload, &paths);
            }
            Err(e) => {
                error!(
                    "[ERROR] Scheduler: Backup failed for server '{}': {}",
                    server_name, e
                );

                // Dispatch webhook failure notification
                let payload = crate::webhooks::WebhookPayload::backup_complete(
                    &server_name,
                    &server_cfg.path,
                    0,
                    false,
                    Some(&e.to_string()),
                );
                crate::webhooks::WebhookDispatcher::dispatch(payload, &paths);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cron_field_parsing() {
        let any = CronField::parse("*").unwrap();
        assert!(any.matches(0));
        assert!(any.matches(59));

        let step = CronField::parse("*/15").unwrap();
        assert!(step.matches(0));
        assert!(step.matches(15));
        assert!(step.matches(30));
        assert!(!step.matches(10));

        let list = CronField::parse("1,3,5").unwrap();
        assert!(list.matches(1));
        assert!(list.matches(3));
        assert!(list.matches(5));
        assert!(!list.matches(2));

        let range = CronField::parse("1-5").unwrap();
        assert!(range.matches(1));
        assert!(range.matches(3));
        assert!(range.matches(5));
        assert!(!range.matches(6));
    }

    #[test]
    fn test_cron_schedule_and_interval() {
        let cron = CronSchedule::parse("*/30 3 * * *").unwrap();
        // 03:00 UTC
        let dt1 = DateTime::parse_from_rfc3339("2026-06-15T03:00:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert!(cron.matches(&dt1));

        // 03:15 UTC
        let dt2 = DateTime::parse_from_rfc3339("2026-06-15T03:15:00Z")
            .unwrap()
            .with_timezone(&Utc);
        assert!(!cron.matches(&dt2));

        // Natural intervals
        let sched1 = BackupSchedule::parse("every 6h").unwrap();
        assert_eq!(sched1, BackupSchedule::Interval(Duration::from_secs(6 * 3600)));

        let sched2 = BackupSchedule::parse("30m").unwrap();
        assert_eq!(sched2, BackupSchedule::Interval(Duration::from_secs(30 * 60)));
    }
}
